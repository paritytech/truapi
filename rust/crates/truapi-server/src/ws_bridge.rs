//! Localhost WebSocket bridge. Binds to `127.0.0.1:<port>`, gates each
//! connection on a session token, and relays SCALE-encoded
//! [`ProtocolMessage`] frames into a [`TrUApiCore`].
//!
//! Feature-gated (`ws-bridge`) so wasm32 and no-tokio build paths stay lean.
//!
//! The bridge owns a `tokio` runtime spawned at [`WsBridge::start`] time and
//! shuts down both the accept loop and the runtime when the handle is dropped
//! or [`WsBridge::stop`] is called.
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
use std::sync::{Arc, Mutex};
use std::thread;

use futures::{SinkExt, StreamExt};
use parity_scale_codec::{Decode, Encode};
use rand::RngCore;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{Response as HttpResponse, StatusCode};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use crate::{ProtocolMessage, TrUApiCore, Transport};

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

/// Running bridge handle. Drop or call [`WsBridge::stop`] to shut down.
///
/// The bridge owns a dedicated OS thread that runs a `tokio` current-thread
/// runtime + `LocalSet`. Using `spawn_local` is required because the
/// dispatcher's per-method futures are `LocalBoxFuture` (the truapi trait
/// uses `async fn`, whose auto-generated futures are not `Send`).
pub struct WsBridge {
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl WsBridge {
    /// Bind a localhost listener and start the accept loop on a dedicated
    /// OS thread. Returns the [`WsBridgeEndpoint`] descriptor the host
    /// hands to the product alongside the bridge handle.
    pub fn start(
        bind_port: u16,
        core: Arc<TrUApiCore>,
        logger: BridgeLogger,
    ) -> io::Result<(Self, WsBridgeEndpoint)> {
        let mut token_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut token_bytes);
        let token = hex::encode(token_bytes);

        // Bind synchronously so we can surface bind errors back to the
        // caller and discover the actual port the OS handed back. The
        // listener is registered with tokio inside the worker thread
        // because a `tokio::net::TcpListener` is bound to the runtime that
        // created it.
        let std_listener =
            std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], bind_port)))?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<io::Result<()>>();
        let accept_token = token.clone();
        let accept_logger = logger.clone();
        let worker_logger = logger.clone();
        let thread = thread::Builder::new()
            .name("truapi-ws-bridge".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => {
                        let _ = ready_tx.send(Err(io::Error::other(err.to_string())));
                        return;
                    }
                };
                let local = LocalSet::new();
                let listener_setup = rt.block_on(async { TcpListener::from_std(std_listener) });
                let listener = match listener_setup {
                    Ok(listener) => {
                        let _ = ready_tx.send(Ok(()));
                        listener
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                local.block_on(
                    &rt,
                    accept_loop(listener, core, accept_token, accept_logger, shutdown_rx),
                );
                worker_logger("truapi.ws_bridge.worker_exit", "worker thread exiting");
            })?;

        // Block until the worker thread reports the listener is registered
        // with its runtime, so the caller knows the bridge is ready to
        // accept connections by the time `start` returns.
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(err) => return Err(io::Error::other(err.to_string())),
        }

        logger(
            "truapi.ws_bridge.started",
            &format!("port={port} token_len={}", token.len()),
        );

        Ok((
            Self {
                shutdown: Some(shutdown_tx),
                thread: Some(thread),
            },
            WsBridgeEndpoint { port, token },
        ))
    }

    /// Signal the accept loop to exit and join the worker thread.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
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
    core: Arc<TrUApiCore>,
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
                let core = core.clone();
                let logger = logger.clone();
                let expected = expected_token.clone();
                handles.push(tokio::task::spawn_local(async move {
                    handle_connection(stream, peer, core, expected, logger).await;
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
    core: Arc<TrUApiCore>,
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
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(out_tx));

    let pump_logger = logger.clone();
    let pump = tokio::task::spawn_local(async move {
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

    // Dispatch each inbound frame on its own local task so a slow request
    // handler (e.g. a login pending on the pairing prompt) cannot stall the
    // read loop and starve later frames on the same connection. Responses may
    // interleave; the wire protocol matches them by request id, and
    // `WsTransport::send` is safe to call from concurrent local tasks.
    let mut in_flight: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    while let Some(frame) = source.next().await {
        match frame {
            Ok(WsMessage::Binary(bytes)) => {
                let message = match ProtocolMessage::decode(&mut &*bytes) {
                    Ok(m) => m,
                    Err(err) => {
                        logger("truapi.ws_bridge.decode_error", &err.to_string());
                        continue;
                    }
                };
                in_flight.retain(|task| !task.is_finished());
                let core = core.clone();
                let transport = transport.clone();
                in_flight.push(tokio::task::spawn_local(async move {
                    core.dispatch(message, transport).await;
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

    drop(transport);
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

struct WsTransport {
    outbound: mpsc::Sender<Vec<u8>>,
    closed: Mutex<bool>,
}

impl WsTransport {
    fn new(outbound: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            outbound,
            closed: Mutex::new(false),
        }
    }
}

impl Transport for WsTransport {
    fn send(&self, message: ProtocolMessage) {
        if *self.closed.lock().unwrap() {
            return;
        }
        // Non-blocking: a full queue means the peer stopped reading, so the
        // connection is treated as closed rather than buffering without bound.
        if self.outbound.try_send(message.encode()).is_err() {
            *self.closed.lock().unwrap() = true;
        }
    }

    fn on_message(
        &self,
        _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
    ) -> Box<dyn FnOnce()> {
        Box::new(|| {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi::versioned::account::HostRequestLoginRequest;
    use truapi::versioned::system::HostFeatureSupportedRequest;

    use crate::frame::{Payload, request_ids};
    use crate::test_support::{
        StubPlatform, first_pairing_deeplink, runtime_config, stub_platform,
    };
    use std::sync::atomic::Ordering;
    use truapi_platform::AuthState;

    fn test_core() -> Arc<TrUApiCore> {
        core_for(Arc::new(StubPlatform::default()))
    }

    fn core_for(platform: Arc<StubPlatform>) -> Arc<TrUApiCore> {
        Arc::new(TrUApiCore::from_platform_with_config(
            platform,
            runtime_config("dotli.dot"),
            crate::subscription::thread_per_subscription_spawner(),
        ))
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

    /// Spin the bridge up on `127.0.0.1:0`, dial it with a real
    /// `tokio-tungstenite` client, send a known SCALE frame, and verify
    /// the bridge echoes the SCALE-encoded `feature_supported` response.
    #[test]
    fn round_trip_feature_supported_through_bridge() {
        let core = test_core();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        // Use a fresh `tokio` runtime on the test thread so we don't fight
        // the bridge's runtime, which lives on a different worker thread.
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

    fn request_frame(request_id: &str, method: &str, value: Vec<u8>) -> WsMessage {
        let ids = request_ids(method).expect("known request method");
        WsMessage::Binary(
            ProtocolMessage {
                request_id: request_id.into(),
                payload: Payload {
                    id: ids.request_id,
                    value,
                },
            }
            .encode(),
        )
    }

    fn login_frame(request_id: &str) -> WsMessage {
        request_frame(
            request_id,
            "account_request_login",
            HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None }).encode(),
        )
    }

    /// A request whose handler pends (a login waiting on the pairing prompt)
    /// must not serialize the connection: a concurrent `feature_supported`
    /// on the same socket still round-trips while the login is in flight.
    #[test]
    fn slow_request_does_not_block_concurrent_round_trip() {
        let platform = Arc::new(StubPlatform {
            chain_connect_pending: true,
            ..Default::default()
        });
        let auth_states = platform.auth_states.clone();
        let core = core_for(platform);
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let feature_ids = request_ids("system_feature_supported").expect("known request method");
        let response = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");
            ws.send(login_frame("p:login")).await.expect("send login");

            // Wait until the login handler has emitted the pairing state and
            // is pending on the statement-store connect.
            for _ in 0..1000 {
                if first_pairing_deeplink(&auth_states).is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                first_pairing_deeplink(&auth_states).is_some(),
                "login handler did not reach the pairing prompt"
            );

            ws.send(request_frame(
                "p:feature",
                "system_feature_supported",
                HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
                    genesis_hash: vec![0u8; 32],
                })
                .encode(),
            ))
            .await
            .expect("send feature_supported");

            let bytes = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    match ws.next().await {
                        Some(Ok(WsMessage::Binary(bytes))) => break bytes,
                        Some(Ok(_)) => continue,
                        Some(Err(err)) => panic!("ws error: {err}"),
                        None => panic!("connection closed before response"),
                    }
                }
            })
            .await
            .expect("feature_supported must answer while the login is pending");
            ProtocolMessage::decode(&mut &bytes[..]).expect("decode response")
        });

        assert_eq!(response.request_id, "p:feature");
        assert_eq!(response.payload.id, feature_ids.response_id);
        bridge.stop();
    }

    /// Host-side login cancellation must resolve the pending request with
    /// `Rejected` on the same WebSocket connection.
    #[test]
    fn host_cancel_resolves_pending_request_login() {
        let platform = stub_platform();
        let auth_states = platform.auth_states.clone();
        let core = core_for(platform);
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) =
            WsBridge::start(0, core.clone(), logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let login_ids = request_ids("account_request_login").expect("known request method");
        let response_bytes = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");
            ws.send(login_frame("p:login")).await.expect("send login");

            for _ in 0..1000 {
                if first_pairing_deeplink(&auth_states).is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                first_pairing_deeplink(&auth_states).is_some(),
                "login handler did not reach the pairing prompt"
            );

            core.cancel_login();

            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    match ws.next().await {
                        Some(Ok(WsMessage::Binary(bytes))) => break bytes,
                        Some(Ok(_)) => continue,
                        Some(Err(err)) => panic!("websocket error before response: {err}"),
                        None => panic!("connection closed before response"),
                    }
                }
            })
            .await
            .expect("cancelled login must answer")
        });

        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:login");
        assert_eq!(response.payload.id, login_ids.response_id);
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x02]);

        bridge.stop();
    }

    /// Dropping the client connection cancels in-flight requests: a pending
    /// `request_login` unwinds (its statement-store connect future is
    /// dropped) instead of outliving the connection, and the abandoned
    /// pairing state is reset for the host UI.
    #[test]
    fn connection_drop_cancels_pending_request_login() {
        let platform = Arc::new(StubPlatform {
            chain_connect_pending: true,
            ..Default::default()
        });
        let auth_states = platform.auth_states.clone();
        let pending_connect_dropped = platform.pending_connect_dropped.clone();
        let core = core_for(platform);
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");
            ws.send(login_frame("p:login")).await.expect("send login");

            for _ in 0..1000 {
                if first_pairing_deeplink(&auth_states).is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                first_pairing_deeplink(&auth_states).is_some(),
                "login handler did not reach the pairing prompt"
            );

            ws.close(None).await.ok();
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !pending_connect_dropped.load(Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < deadline,
                "pending request_login was not cancelled on connection drop"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let last = auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .last()
                .cloned();
            if last == Some(AuthState::Disconnected) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "abandoned pairing was not reset to Disconnected, last state: {last:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        bridge.stop();
    }

    /// A handshake with the wrong `?t=` token must be rejected at the HTTP
    /// upgrade step with a 401, not silently dropped.
    #[test]
    fn wrong_token_is_rejected_at_handshake() {
        let core = test_core();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
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
    /// shut the worker thread down cleanly. `Drop::drop` calls `stop`, and
    /// a second `stop` (from drop after the test's explicit one) is a
    /// no-op.
    #[test]
    fn drop_calls_stop_idempotently() {
        let core = test_core();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (bridge, _endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
        // Drop the bridge; the worker thread must join via Drop.
        drop(bridge);

        // Build a second bridge and explicitly stop twice. The second
        // call has no shutdown sender and no thread handle left to join,
        // so it returns without panicking.
        let core = test_core();
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, _endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
        bridge.stop();
        bridge.stop();
    }
}
