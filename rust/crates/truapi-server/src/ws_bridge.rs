//! Localhost WebSocket bridge. Binds to `127.0.0.1:<port>`, gates each
//! connection on a session token, and relays SCALE-encoded
//! [`ProtocolMessage`] frames into a [`TrUApiCore`].
//!
//! Feature-gated (`ws-bridge`) so wasm32 and no-tokio build paths stay lean.
//!
//! The bridge owns a `tokio` runtime spawned at [`WsBridge::start`] time and
//! shuts down both the accept loop and the runtime when the handle is dropped
//! or [`WsBridge::stop`] is called.

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
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use crate::{ProtocolMessage, TrUApiCore, Transport};

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
                let core = core.clone();
                let logger = logger.clone();
                let expected = expected_token.clone();
                handles.push(tokio::task::spawn_local(async move {
                    handle_connection(stream, peer, core, expected, logger).await;
                }));
                handles.retain(|h| !h.is_finished());
            }
        }
    }
}

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

    let ws = match tokio_tungstenite::accept_hdr_async(stream, callback).await {
        Ok(ws) => ws,
        Err(err) => {
            logger("truapi.ws_bridge.handshake_error", &err.to_string());
            return;
        }
    };

    logger("truapi.ws_bridge.connection_open", &peer.to_string());
    let (mut sink, mut source) = ws.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
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
                core.dispatch(message, transport.clone()).await;
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
        if key == "t" && value == expected {
            return true;
        }
    }
    false
}

struct WsTransport {
    outbound: mpsc::UnboundedSender<Vec<u8>>,
    closed: Mutex<bool>,
}

impl WsTransport {
    fn new(outbound: mpsc::UnboundedSender<Vec<u8>>) -> Self {
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
        if self.outbound.send(message.encode()).is_err() {
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
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi::versioned::account::{
        HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofRequest,
        HostAccountCreateProofResponse, HostAccountGetAliasRequest, HostAccountGetAliasResponse,
        HostAccountGetRequest, HostAccountGetResponse, HostGetLegacyAccountsRequest,
        HostGetLegacyAccountsResponse, HostGetUserIdRequest, HostGetUserIdResponse,
    };
    use truapi::versioned::preimage::{
        RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
    };
    use truapi::versioned::signing::{
        HostSignPayloadRequest, HostSignPayloadResponse, HostSignRawRequest, HostSignRawResponse,
    };
    use truapi::versioned::statement_store::{
        RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
        RemoteStatementStoreSubmitRequest, RemoteStatementStoreSubscribeItem,
        RemoteStatementStoreSubscribeRequest,
    };
    use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
    use truapi_platform::{
        Accounts as PlatformAccounts, ChainProvider, Features, GenesisHash, JsonRpcConnection,
        Navigation, Notifications, Permissions, Preimage as PlatformPreimage,
        Signing as PlatformSigning, StatementStore as PlatformStatementStore, Storage,
    };

    use crate::frame::{FrameKind, Payload, compose_action};

    struct StubPlatform;

    #[async_trait]
    impl Storage for StubPlatform {
        async fn read(
            &self,
            _key: String,
        ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
            Ok(None)
        }
        async fn write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn clear(&self, _key: String) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
    }

    #[async_trait]
    impl Navigation for StubPlatform {
        async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
            Ok(())
        }
    }

    #[async_trait]
    impl Notifications for StubPlatform {
        async fn push_notification(
            &self,
            _notification: v01::HostPushNotificationRequest,
        ) -> Result<(), v01::GenericError> {
            Ok(())
        }
    }

    #[async_trait]
    impl Permissions for StubPlatform {
        async fn device_permission(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
            Ok(v01::HostDevicePermissionResponse { granted: true })
        }
        async fn remote_permission(
            &self,
            _request: v01::RemotePermissionRequest,
        ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
            Ok(v01::RemotePermissionResponse { granted: true })
        }
    }

    #[async_trait]
    impl Features for StubPlatform {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
            let HostFeatureSupportedRequest::V1(_) = request;
            Ok(HostFeatureSupportedResponse::V1(
                v01::HostFeatureSupportedResponse { supported: true },
            ))
        }
    }

    struct DeadConnection;
    impl JsonRpcConnection for DeadConnection {
        fn send(&self, _request: String) {}
        fn responses(&self) -> BoxStream<'static, String> {
            Box::pin(stream::empty())
        }
    }

    #[async_trait]
    impl ChainProvider for StubPlatform {
        async fn connect(
            &self,
            _genesis_hash: GenesisHash,
        ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
            Ok(Box::new(DeadConnection))
        }
    }

    #[async_trait]
    impl PlatformAccounts for StubPlatform {
        async fn host_account_get(
            &self,
            _request: HostAccountGetRequest,
        ) -> Result<HostAccountGetResponse, v01::HostAccountGetError> {
            Err(v01::HostAccountGetError::NotConnected)
        }
        async fn host_account_get_alias(
            &self,
            _request: HostAccountGetAliasRequest,
        ) -> Result<HostAccountGetAliasResponse, v01::HostAccountGetError> {
            Err(v01::HostAccountGetError::NotConnected)
        }
        async fn host_account_create_proof(
            &self,
            _request: HostAccountCreateProofRequest,
        ) -> Result<HostAccountCreateProofResponse, v01::HostAccountCreateProofError> {
            Err(v01::HostAccountCreateProofError::RingNotFound)
        }
        async fn host_get_legacy_accounts(
            &self,
            _request: HostGetLegacyAccountsRequest,
        ) -> Result<HostGetLegacyAccountsResponse, v01::HostAccountGetError> {
            Ok(HostGetLegacyAccountsResponse::V1(
                v01::HostGetLegacyAccountsResponse { accounts: vec![] },
            ))
        }
        async fn host_account_connection_status_subscribe(
            &self,
        ) -> BoxStream<'static, HostAccountConnectionStatusSubscribeItem> {
            Box::pin(stream::empty())
        }
        async fn host_get_user_id(
            &self,
            _request: HostGetUserIdRequest,
        ) -> Result<HostGetUserIdResponse, v01::HostGetUserIdError> {
            Err(v01::HostGetUserIdError::NotConnected)
        }
    }

    #[async_trait]
    impl PlatformSigning for StubPlatform {
        async fn host_sign_payload(
            &self,
            _request: HostSignPayloadRequest,
        ) -> Result<HostSignPayloadResponse, v01::HostSignPayloadError> {
            Err(v01::HostSignPayloadError::Rejected)
        }
        async fn host_sign_raw(
            &self,
            _request: HostSignRawRequest,
        ) -> Result<HostSignRawResponse, v01::HostSignPayloadError> {
            Err(v01::HostSignPayloadError::Rejected)
        }
    }

    #[async_trait]
    impl PlatformStatementStore for StubPlatform {
        async fn remote_statement_store_subscribe(
            &self,
            _request: RemoteStatementStoreSubscribeRequest,
        ) -> BoxStream<'static, RemoteStatementStoreSubscribeItem> {
            Box::pin(stream::empty())
        }
        async fn remote_statement_store_submit(
            &self,
            _request: RemoteStatementStoreSubmitRequest,
        ) -> Result<(), v01::GenericError> {
            Ok(())
        }
        async fn remote_statement_store_create_proof(
            &self,
            _request: RemoteStatementStoreCreateProofRequest,
        ) -> Result<
            RemoteStatementStoreCreateProofResponse,
            v01::RemoteStatementStoreCreateProofError,
        > {
            Err(v01::RemoteStatementStoreCreateProofError::UnableToSign)
        }
    }

    #[async_trait]
    impl PlatformPreimage for StubPlatform {
        async fn remote_preimage_lookup_subscribe(
            &self,
            _request: RemotePreimageLookupSubscribeRequest,
        ) -> BoxStream<'static, RemotePreimageLookupSubscribeItem> {
            Box::pin(stream::empty())
        }
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
        let core = Arc::new(TrUApiCore::from_platform(Arc::new(StubPlatform)));
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        // Use a fresh `tokio` runtime on the test thread so we don't fight
        // the bridge's runtime, which lives on a different worker thread.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let response_bytes = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

            let request_frame = ProtocolMessage {
                request_id: "p:1".into(),
                payload: Payload {
                    tag: compose_action("system_feature_supported", FrameKind::Request),
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
        assert_eq!(
            response.payload.tag,
            compose_action("system_feature_supported", FrameKind::Response),
        );
        // Wire payload is `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);

        bridge.stop();
    }
}
