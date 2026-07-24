//! Localhost WebSocket bridge. Binds to `127.0.0.1:<port>`, gates each
//! connection on a session token, and relays SCALE-encoded
//! [`ProtocolMessage`](crate::frame::ProtocolMessage) frames into a
//! product-scoped runtime.
//!
//! Feature-gated (`ws-bridge`) so wasm32 and no-tokio build paths stay lean.
//!
//! Native bridges share one process-wide `tokio` runtime. Each [`WsBridge`]
//! owns only its accept loop and connection tasks; dropping or stopping one
//! bridge leaves the executor available to other products.
//!
//! Security model: the listener binds to `127.0.0.1` only, and every
//! connection must present the per-session 256-bit token (`?t=<token>`,
//! drawn from the OS CSPRNG) before the WebSocket upgrade completes. The token
//! is the sole authentication gate and is compared in constant time. It is
//! handed only to the host's embedded WebView, so the bridge does not also pin
//! the `Origin` header (the WebView's origin is not known a priori). Inbound
//! messages are size-capped, and the per-connection outbound queue and the
//! total connection count are bounded to contain a misbehaving local peer.

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};

use futures::{SinkExt, StreamExt};
use rand::RngCore;
use tokio::net::TcpListener;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{Response as HttpResponse, StatusCode};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use crate::{FrameSink, ProductRuntime};

/// Maximum simultaneous connections the bridge will service. The product uses
/// a single connection; the cap bounds resource use from a buggy or hostile
/// local peer opening many sockets.
const MAX_WS_BRIDGE_CONNECTIONS: usize = 32;

/// Bound on the per-connection outbound frame queue. A peer that stops reading
/// cannot make the core buffer responses without limit; once the queue fills
/// the connection is treated as closed.
const OUTBOUND_QUEUE_CAP: usize = 4096;

/// Ceiling on a single inbound WebSocket message / frame. `ProtocolMessage`
/// frames on this SCALE control channel are small; the cap prevents a
/// memory-amplification DoS well below tungstenite's 64 MiB default.
const MAX_WS_MESSAGE_BYTES: usize = 8 << 20;

/// Per-session descriptor returned to the host: product uses `port + token`
/// to build its WebSocket URL (e.g. `ws://127.0.0.1:<port>/?t=<token>`).
#[derive(Clone, Debug, uniffi::Record)]
pub struct WsBridgeEndpoint {
    /// Localhost port the bridge is listening on.
    pub port: u16,
    /// Session token; the connecting client must supply this as the
    /// `?t=<token>` query parameter to be accepted.
    pub token: String,
}

/// Failure modes returned from host-facing `start_ws_bridge` wrappers.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum WsBridgeStartError {
    /// A bridge is already running for this host.
    #[error("ws bridge already running")]
    AlreadyRunning,
    /// Anything else (bind failure, runtime spin-up failure, ...).
    #[error("ws bridge start failed: {0}")]
    Io(String),
}

impl From<io::Error> for WsBridgeStartError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::AlreadyExists {
            WsBridgeStartError::AlreadyRunning
        } else {
            WsBridgeStartError::Io(err.to_string())
        }
    }
}

/// Logger callback shape used by the bridge for lifecycle events. The
/// Android and iOS wrappers adapt their per-platform callback interfaces to
/// this platform-neutral shape.
pub type BridgeLogger = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Factory used by the bridge to create one product runtime per WebSocket
/// connection.
pub trait WsProductRuntimeFactory: Send + Sync {
    /// Create a runtime that emits outgoing frames into `sink`.
    fn product_runtime(&self, sink: Arc<dyn FrameSink>) -> ProductRuntime;
}

impl<F> WsProductRuntimeFactory for F
where
    F: Fn(Arc<dyn FrameSink>) -> ProductRuntime + Send + Sync,
{
    fn product_runtime(&self, sink: Arc<dyn FrameSink>) -> ProductRuntime {
        self(sink)
    }
}

/// Process-wide executor shared by every native product bridge.
///
/// The runtime intentionally lives until process exit. Native products have
/// independent bridge lifecycles, so shutting the executor down with any one
/// bridge would interrupt the others.
struct SharedNativeExecutor {
    runtime: Runtime,
}

impl SharedNativeExecutor {
    fn new() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .thread_name("truapi-native-worker")
            .enable_all()
            .build()
            .map_err(|err| io::Error::other(err.to_string()))?;
        Ok(Self { runtime })
    }

    fn handle(&self) -> Handle {
        self.runtime.handle().clone()
    }

    fn worker_threads(&self) -> usize {
        self.runtime.metrics().num_workers()
    }
}

static SHARED_NATIVE_EXECUTOR: OnceLock<SharedNativeExecutor> = OnceLock::new();
static SHARED_NATIVE_EXECUTOR_INIT: Mutex<()> = Mutex::new(());

fn shared_native_executor() -> io::Result<(&'static SharedNativeExecutor, bool)> {
    if let Some(executor) = SHARED_NATIVE_EXECUTOR.get() {
        return Ok((executor, false));
    }

    // Serialize fallible initialization without caching a transient thread
    // creation failure for the rest of the process.
    let _guard = SHARED_NATIVE_EXECUTOR_INIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(executor) = SHARED_NATIVE_EXECUTOR.get() {
        return Ok((executor, false));
    }

    let initialized = SHARED_NATIVE_EXECUTOR
        .set(SharedNativeExecutor::new()?)
        .is_ok();
    let executor = SHARED_NATIVE_EXECUTOR
        .get()
        .ok_or_else(|| io::Error::other("shared native executor initialization failed"))?;
    Ok((executor, initialized))
}

/// Running bridge handle. Drop or call [`WsBridge::stop`] to shut down.
///
/// The bridge's tasks run on the process-wide native executor. TrUAPI dispatch
/// futures are `Send`, so connections and independent frames from all products
/// can execute across the shared worker pool.
pub struct WsBridge {
    shutdown: Option<oneshot::Sender<()>>,
    stopped: Option<std::sync::mpsc::Receiver<()>>,
    accept_task: Option<tokio::task::JoinHandle<()>>,
    runtime_id: tokio::runtime::Id,
}

impl WsBridge {
    /// Bind a localhost listener and start the accept loop on the shared
    /// native executor. Returns the [`WsBridgeEndpoint`] descriptor the host
    /// hands to the product alongside the bridge handle.
    pub fn start(
        bind_port: u16,
        runtime_factory: Arc<dyn WsProductRuntimeFactory>,
        logger: BridgeLogger,
    ) -> io::Result<(Self, WsBridgeEndpoint)> {
        let mut token_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut token_bytes);
        let token = hex::encode(token_bytes);

        // Bind synchronously so we can surface bind errors and discover the
        // actual port before returning the endpoint.
        let std_listener =
            std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], bind_port)))?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();

        let (executor, initialized) = shared_native_executor()?;
        let handle = executor.handle();
        let runtime_id = handle.id();
        if initialized {
            logger(
                "truapi.native.executor.started",
                &format!(
                    "runtime_id={runtime_id} worker_threads={}",
                    executor.worker_threads()
                ),
            );
        }

        // Register the listener with the shared runtime's I/O driver before
        // returning so a successful start always yields a ready endpoint.
        let listener = {
            let _entered = handle.enter();
            TcpListener::from_std(std_listener)?
        };
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (stopped_tx, stopped_rx) = std::sync::mpsc::channel::<()>();
        let accept_token = token.clone();
        let accept_logger = logger.clone();
        let accept_task = handle.spawn(async move {
            accept_loop(
                listener,
                runtime_factory,
                accept_token,
                accept_logger,
                shutdown_rx,
            )
            .await;
            let _ = stopped_tx.send(());
        });

        logger(
            "truapi.ws_bridge.started",
            &format!(
                "port={port} token_len={} runtime_id={runtime_id}",
                token.len()
            ),
        );

        Ok((
            Self {
                shutdown: Some(shutdown_tx),
                stopped: Some(stopped_rx),
                accept_task: Some(accept_task),
                runtime_id,
            },
            WsBridgeEndpoint { port, token },
        ))
    }

    /// Signal this bridge's accept loop to exit without stopping the shared
    /// native executor used by other products.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }

        // UniFFI hosts call stop synchronously from outside Rust's executor,
        // where waiting preserves the existing "fully stopped on return"
        // behavior. Avoid blocking if a Rust caller drops the bridge from one
        // of the shared runtime's own workers, especially on a single-core
        // runtime; the shutdown signal still lets the task clean itself up.
        let called_from_shared_executor =
            Handle::try_current().is_ok_and(|handle| handle.id() == self.runtime_id);
        let stopped_cleanly = if called_from_shared_executor {
            drop(self.stopped.take());
            true
        } else {
            self.stopped
                .take()
                .is_none_or(|stopped| stopped.recv().is_ok())
        };

        if let Some(task) = self.accept_task.take()
            && !stopped_cleanly
            && !task.is_finished()
        {
            task.abort();
        }
    }
}

impl Drop for WsBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn accept_loop(
    listener: TcpListener,
    runtime_factory: Arc<dyn WsProductRuntimeFactory>,
    expected_token: String,
    logger: BridgeLogger,
    mut shutdown: oneshot::Receiver<()>,
) {
    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                logger("truapi.ws_bridge.shutdown", "accept loop exiting");
                for h in &handles {
                    h.abort();
                }
                for h in handles {
                    let _ = h.await;
                }
                break;
            }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(pair) => pair,
                    Err(err) => {
                        logger("truapi.ws_bridge.accept_error", &err.to_string());
                        continue;
                    }
                };
                handles.retain(|h| !h.is_finished());
                if handles.len() >= MAX_WS_BRIDGE_CONNECTIONS {
                    logger("truapi.ws_bridge.connection_limit", &peer.to_string());
                    drop(stream);
                    continue;
                }
                let runtime_factory = runtime_factory.clone();
                let logger = logger.clone();
                let expected = expected_token.clone();
                handles.push(tokio::spawn(async move {
                    handle_connection(stream, peer, runtime_factory, expected, logger).await;
                }));
            }
        }
    }
}

// `clippy::result_large_err` fires on the handshake callback because
// tokio-tungstenite's `ErrorResponse` type carries the full HTTP response
// (~136 bytes). The closure signature is dictated by tokio-tungstenite's
// API, so the lint can only be silenced at the call site.
#[allow(clippy::result_large_err)]
async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer: SocketAddr,
    runtime_factory: Arc<dyn WsProductRuntimeFactory>,
    expected_token: String,
    logger: BridgeLogger,
) {
    let auth_logger = logger.clone();
    let callback = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        if path_token_matches(
            req.uri().path_and_query().map(|p| p.as_str()),
            &expected_token,
        ) {
            Ok(resp)
        } else {
            auth_logger("truapi.ws_bridge.reject_unauthorized", &peer.to_string());
            let mut err: ErrorResponse = HttpResponse::new(Some("invalid token".to_string()));
            *err.status_mut() = StatusCode::UNAUTHORIZED;
            Err(err)
        }
    };

    // Cap inbound message/frame size so a peer cannot force the runtime to
    // buffer up to tungstenite's 64 MiB default on this small control channel.
    let config = WebSocketConfig {
        max_message_size: Some(MAX_WS_MESSAGE_BYTES),
        max_frame_size: Some(MAX_WS_MESSAGE_BYTES),
        ..Default::default()
    };
    let ws = match tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config))
        .await
    {
        Ok(ws) => ws,
        Err(err) => {
            logger("truapi.ws_bridge.handshake_error", &err.to_string());
            return;
        }
    };

    logger("truapi.ws_bridge.connection_open", &peer.to_string());
    let (mut sink, mut source) = ws.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_QUEUE_CAP);
    let frame_sink = Arc::new(WsFrameSink::new(out_tx));
    let product_runtime = Arc::new(runtime_factory.product_runtime(frame_sink));

    let pump_logger = logger.clone();
    let pump = tokio::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
            if let Err(err) = sink.send(WsMessage::Binary(bytes)).await {
                pump_logger("truapi.ws_bridge.send_error", &err.to_string());
                break;
            }
        }
        let _ = sink
            .send(WsMessage::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "bridge closing".into(),
            })))
            .await;
        let _ = sink.close().await;
    });

    // Dispatch each inbound frame on its own `Send` task so a slow request
    // handler cannot stall the read loop and independent frames can run on
    // different executor workers. Responses may interleave; the wire protocol
    // matches them by request id, and `WsFrameSink::emit_frame` is thread-safe.
    let mut in_flight: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    while let Some(frame) = source.next().await {
        match frame {
            Ok(WsMessage::Binary(bytes)) => {
                in_flight.retain(|task| !task.is_finished());
                let product_runtime = product_runtime.clone();
                in_flight.push(tokio::spawn(async move {
                    let _ = product_runtime.receive_frame(bytes.to_vec()).await;
                }));
            }
            Ok(WsMessage::Text(_)) => {
                logger("truapi.ws_bridge.text_frame_ignored", "");
            }
            Ok(WsMessage::Close(_)) => break,
            Ok(_) => {}
            Err(err) => {
                logger("truapi.ws_bridge.read_error", &err.to_string());
                break;
            }
        }
    }

    // The connection is gone: cancel in-flight dispatches so long-pending
    // handlers unwind instead of outliving the connection.
    for task in &in_flight {
        task.abort();
    }

    product_runtime.dispose();
    let _ = pump.await;
    logger("truapi.ws_bridge.connection_closed", &peer.to_string());
}

fn path_token_matches(path_and_query: Option<&str>, expected: &str) -> bool {
    let Some(raw) = path_and_query else {
        return false;
    };
    let query = match raw.find('?') {
        Some(idx) => &raw[idx + 1..],
        None => return false,
    };
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        if key == "t" && constant_time_eq(value.as_bytes(), expected.as_bytes()) {
            return true;
        }
    }
    false
}

/// Constant-time byte-slice equality, used for the session-token check so a
/// local peer cannot recover the token via early-exit comparison timing. The
/// token length is fixed and public, so a length mismatch may short-circuit;
/// only the value comparison must be constant time.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

struct WsFrameSink {
    outbound: mpsc::Sender<Vec<u8>>,
    closed: Mutex<bool>,
}

impl WsFrameSink {
    fn new(outbound: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            outbound,
            closed: Mutex::new(false),
        }
    }
}

impl FrameSink for WsFrameSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        if *self.closed.lock().unwrap() {
            return;
        }
        // Non-blocking: a full queue means the peer stopped reading, so the
        // connection is treated as closed rather than buffering without bound.
        if self.outbound.try_send(frame).is_err() {
            *self.closed.lock().unwrap() = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Decode;
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi::versioned::system::HostFeatureSupportedRequest;
    use truapi_platform::{HostInfo, PlatformInfo, ProductContext, SigningHostConfig};

    use crate::SigningHostRuntime;
    use crate::frame::{Payload, ProtocolMessage, request_ids};
    use crate::test_support::{StubPlatform, test_spawner};

    fn test_runtime_factory() -> Arc<dyn WsProductRuntimeFactory> {
        runtime_factory_for(Arc::new(StubPlatform::default()))
    }

    fn runtime_factory_for(platform: Arc<StubPlatform>) -> Arc<dyn WsProductRuntimeFactory> {
        let config = SigningHostConfig::new(
            HostInfo {
                name: "Polkadot Mobile".to_string(),
                icon: Some("https://example.invalid/dotli.png".to_string()),
                version: None,
            },
            PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
        )
        .expect("test signing host config is valid");
        let runtime = Arc::new(SigningHostRuntime::new(platform, config, test_spawner()));
        let product =
            ProductContext::new("dotli.dot".to_string()).expect("test product context is valid");
        Arc::new(move |sink| runtime.product_runtime(product.clone(), sink))
    }

    #[test]
    fn path_token_matches_exact() {
        assert!(path_token_matches(Some("/?t=abc"), "abc"));
        assert!(path_token_matches(Some("/?foo=1&t=abc"), "abc"));
        assert!(!path_token_matches(Some("/?t=other"), "abc"));
        assert!(!path_token_matches(Some("/?token=abc"), "abc"));
        assert!(!path_token_matches(Some("/"), "abc"));
        assert!(!path_token_matches(None, "abc"));
    }

    #[test]
    fn shared_executor_uses_multithread_scheduler() {
        let (executor, _) = shared_native_executor().expect("shared native executor");
        let handle = executor.handle();
        assert_eq!(
            handle.runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread
        );

        // Each task blocks one runtime worker at the barrier. They can only
        // both complete if the executor actually schedules them concurrently
        // on distinct worker threads.
        if executor.worker_threads() < 2 {
            return;
        }
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let first = handle.spawn({
            let barrier = barrier.clone();
            async move {
                let worker = std::thread::current().id();
                barrier.wait();
                worker
            }
        });
        let second = handle.spawn(async move {
            let worker = std::thread::current().id();
            barrier.wait();
            worker
        });

        let client = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let (first, second) = client.block_on(async { tokio::join!(first, second) });
        assert_ne!(
            first.expect("first dispatch task"),
            second.expect("second dispatch task"),
        );
    }

    #[test]
    fn shared_executor_is_reused() {
        let (first, _) = shared_native_executor().expect("first executor access");
        let (second, initialized) = shared_native_executor().expect("second executor access");

        assert!(!initialized);
        assert_eq!(first.handle().id(), second.handle().id());
    }

    #[test]
    fn drop_from_shared_executor_does_not_block_worker() {
        let (bridge, _) =
            WsBridge::start(0, test_runtime_factory(), Arc::new(|_, _| {})).expect("start bridge");
        let (executor, _) = shared_native_executor().expect("shared native executor");
        let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();

        executor.handle().spawn(async move {
            drop(bridge);
            let _ = dropped_tx.send(());
        });

        dropped_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("dropping from an executor worker must not deadlock");
    }

    /// Spin the bridge up on `127.0.0.1:0`, dial it with a real
    /// `tokio-tungstenite` client, send a known SCALE frame, and verify
    /// the bridge echoes the SCALE-encoded `feature_supported` response.
    #[test]
    fn round_trip_feature_supported_through_bridge() {
        let runtime_factory = test_runtime_factory();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) =
            WsBridge::start(0, runtime_factory, logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        // Use a fresh `tokio` runtime on the test thread so the client does
        // not depend on the native executor under test.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let ids = request_ids("system_feature_supported").expect("known request method");
        let response_bytes = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

            let request_frame = ProtocolMessage {
                request_id: "p:1".into(),
                payload: Payload {
                    id: ids.request_id,
                    value: HostFeatureSupportedRequest::V1(
                        v01::HostFeatureSupportedRequest::Chain {
                            genesis_hash: vec![0u8; 32],
                        },
                    )
                    .encode(),
                },
            };
            ws.send(WsMessage::Binary(request_frame.encode()))
                .await
                .expect("send");

            // Block until the bridge replies with the response frame.
            loop {
                match ws.next().await {
                    Some(Ok(WsMessage::Binary(bytes))) => break bytes,
                    Some(Ok(_)) => continue,
                    Some(Err(err)) => panic!("ws error: {err}"),
                    None => panic!("connection closed before response"),
                }
            }
        });

        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(response.payload.id, ids.response_id);
        // Wire payload is `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);

        bridge.stop();
    }

    /// Multiple product bridges use the same executor, and stopping one
    /// product must not interrupt another product's bridge.
    #[test]
    fn stopping_one_bridge_leaves_another_operational() {
        let runtime_ids = Arc::new(Mutex::new(Vec::<String>::new()));
        let logger: BridgeLogger = {
            let runtime_ids = runtime_ids.clone();
            Arc::new(move |marker, detail| {
                if marker == "truapi.ws_bridge.started"
                    && let Some(runtime_id) = detail.split("runtime_id=").nth(1)
                {
                    runtime_ids.lock().unwrap().push(runtime_id.to_string());
                }
            })
        };
        let (mut first, _) =
            WsBridge::start(0, test_runtime_factory(), logger.clone()).expect("first bridge");
        let (mut second, endpoint) =
            WsBridge::start(0, test_runtime_factory(), logger).expect("second bridge");

        let ids = runtime_ids.lock().unwrap().clone();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], ids[1]);

        first.stop();

        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
        let client = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        client.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("second bridge remains reachable");
            ws.close(None).await.expect("close client");
        });

        second.stop();
    }

    /// A handshake with the wrong `?t=` token must be rejected at the HTTP
    /// upgrade step with a 401, not silently dropped.
    #[test]
    fn wrong_token_is_rejected_at_handshake() {
        let runtime_factory = test_runtime_factory();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) =
            WsBridge::start(0, runtime_factory, logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t=bogus", endpoint.port);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let err = rt
            .block_on(async { tokio_tungstenite::connect_async(&url).await })
            .expect_err("connection must be refused");
        let msg = format!("{err}");
        assert!(
            msg.contains("401") || msg.to_lowercase().contains("unauthorized"),
            "expected 401/unauthorized rejection, got: {msg}",
        );

        bridge.stop();
    }

    /// Dropping a `WsBridge` handle without an explicit `stop()` must still
    /// shut its accept task down cleanly. `Drop::drop` calls `stop`, and a
    /// second `stop` (from drop after the test's explicit one) is a no-op.
    #[test]
    fn drop_calls_stop_idempotently() {
        let runtime_factory = test_runtime_factory();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (bridge, _endpoint) =
            WsBridge::start(0, runtime_factory, logger).expect("start bridge");
        // Drop the bridge; the accept task must finish via Drop.
        drop(bridge);

        // Build a second bridge and explicitly stop twice. The second
        // call has no shutdown sender or accept task left to wait for,
        // so it returns without panicking.
        let runtime_factory = test_runtime_factory();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, _endpoint) =
            WsBridge::start(0, runtime_factory, logger).expect("start bridge");
        bridge.stop();
        bridge.stop();
    }
}
