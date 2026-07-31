//! Native (non-wasm) [`DebugSink`]: streams tapped frames to a loopback
//! `@parity/truapi-debugger` over a WebSocket.
//!
//! The native counterpart of the wasm [`crate::wasm`] `WasmDebugSink`: a dumb,
//! payload-blind byte-forwarder. Each [`DebugEvent::Frame`] is serialized to the
//! debugger's wire envelope - `{channelId, dir, frame}`, where `frame` is the
//! base64 of the untouched SCALE `ProtocolMessage` bytes - and sent as one WS
//! text message. Decoding and the sensitive-frame denylist live in the debugger
//! app, never here.
//!
//! Fire-and-forget by construction, per the [`DebugSink`] contract:
//! [`WsDebugSink::emit`] never blocks and never fails a dispatch. It only
//! serializes and pushes onto a bounded queue; a background task owns the socket,
//! reconnects with capped backoff, and drops frames (counted) when the queue is
//! full. A slow, absent, or crashed debugger loses traces, never a session.
//!
//! Localhost only: the target URL must be `ws://` on a loopback host. No `wss`,
//! no certificates, no LAN. Construct via [`WsDebugSink::connect`] from within a
//! Tokio runtime and install with [`crate::ProductRuntime::set_debug_sink`];
//! constructing one is a dev-only opt-in, so a host that never calls it leaves
//! the tap inert.

use core::net::SocketAddr;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::debug;

use crate::host_core::{DebugEvent, DebugSink};

/// Bounded so a stalled or absent debugger applies backpressure as counted
/// drops, never unbounded memory growth on the observed session.
const QUEUE_CAPACITY: usize = 4096;

/// Initial reconnect delay; doubles on each failed dial up to [`MAX_BACKOFF`].
const INITIAL_BACKOFF: Duration = Duration::from_millis(200);

/// Cap on the reconnect backoff.
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Cap on a single dial + WS handshake; a port that accepts TCP but never
/// completes the upgrade must not park the writer task forever.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Failure building a [`WsDebugSink`].
#[derive(Debug, Error)]
pub enum DebugSinkError {
    /// The debug URL did not parse.
    #[error("invalid debug url: {0}")]
    Url(#[from] url::ParseError),
    /// The debug URL was not `ws://` on a loopback host.
    #[error("debug url must be ws:// on a loopback host, got {0}")]
    NotLoopback(String),
    /// The debug URL host could not be resolved.
    #[error("could not resolve debug url host: {0}")]
    Resolve(#[from] std::io::Error),
    /// `connect` was called outside a Tokio runtime.
    #[error("WsDebugSink::connect must be called from within a Tokio runtime")]
    NoRuntime,
}

/// A dev-only [`DebugSink`] that forwards tapped frames to a loopback debugger
/// over a WebSocket, using the same `{channelId, dir, frame: base64}` envelope
/// the browser host sends.
pub struct WsDebugSink {
    outbound: mpsc::Sender<String>,
    dropped: Arc<AtomicU64>,
}

/// The wire envelope, matching the debugger's `parseWireMessage` / ingest
/// `DebugFrameEnvelope`: `dir` is product-vantage, `frame` is base64 SCALE bytes.
#[derive(Serialize)]
struct WireMessage<'a> {
    #[serde(rename = "channelId")]
    channel_id: &'a str,
    dir: &'a str,
    frame: String,
}

impl WsDebugSink {
    /// Build a sink targeting `url` and spawn its writer task.
    ///
    /// `url` must be `ws://` on `127.0.0.1`, `localhost`, or `[::1]`. Returns
    /// immediately even if the debugger is not yet listening; the writer task
    /// dials lazily and reconnects. Must be called from within a Tokio runtime.
    pub fn connect(url: &str) -> Result<Arc<Self>, DebugSinkError> {
        // Require ws://, then RESOLVE the host and require every resolved
        // address to be loopback. Resolving (rather than string-matching the
        // host) accepts all genuine loopback forms - 127.0.0.0/8, ::1, and a
        // `localhost` that resolves to them - and rejects anything resolving
        // off-loopback, closing the "validate one string, dial another" gap.
        let parsed = url::Url::parse(url)?;
        if parsed.scheme() != "ws" {
            return Err(DebugSinkError::NotLoopback(url.to_string()));
        }
        // `Url::socket_addrs` resolves the host (IP literal or DNS) and handles
        // IPv6 bracket-stripping and the default port; requiring every resolved
        // address to be loopback accepts all genuine loopback forms (127.0.0.0/8,
        // ::1, a `localhost` that resolves to them) and rejects anything that
        // resolves off-loopback.
        let addrs = parsed.socket_addrs(|| Some(80))?;
        if !addrs.iter().all(|addr| addr.ip().is_loopback()) {
            return Err(DebugSinkError::NotLoopback(url.to_string()));
        }
        // Capture the resolved loopback address and dial *it* directly (in
        // `writer_loop`), rather than re-resolving the URL string on every dial.
        // The WS handshake is therefore only ever sent to this checked loopback
        // peer - closing the resolve-then-dial gap where a mid-session resolver
        // change could send the handshake off-box.
        let Some(addr) = addrs.first().copied() else {
            return Err(DebugSinkError::NotLoopback(url.to_string()));
        };

        // Return a Result rather than panicking inside tokio::spawn when called
        // outside a runtime.
        if Handle::try_current().is_err() {
            return Err(DebugSinkError::NoRuntime);
        }

        let (outbound, inbox) = mpsc::channel::<String>(QUEUE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        tokio::spawn(writer_loop(
            url.to_string(),
            addr,
            inbox,
            Arc::clone(&dropped),
        ));
        Ok(Arc::new(Self { outbound, dropped }))
    }

    /// Number of frames dropped because the outbound queue was full (debugger
    /// absent or slower than the observed session). Never affects the session.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl DebugSink for WsDebugSink {
    fn emit(&self, event: DebugEvent) {
        let DebugEvent::Frame {
            channel_id,
            dir,
            bytes,
        } = event;
        let message = WireMessage {
            channel_id: &channel_id.0,
            // Product-vantage string; never hand-mapped, so it cannot invert.
            dir: dir.wire_str(),
            frame: BASE64.encode(&bytes),
        };
        let Ok(line) = serde_json::to_string(&message) else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if self.outbound.try_send(line).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Own the socket for the sink's lifetime: dial with capped backoff, then drain
/// the queue to the wire until the sink is dropped.
async fn writer_loop(
    url: String,
    addr: SocketAddr,
    mut inbox: mpsc::Receiver<String>,
    dropped: Arc<AtomicU64>,
) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        // Dial the pre-validated loopback address directly, then run the WS
        // handshake over that socket. The address is not re-resolved, so the
        // handshake can never reach an off-box peer. The whole dial+handshake is
        // bounded so a TCP-accepting but non-upgrading port can't park the task.
        let dialed = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            let tcp = TcpStream::connect(addr).await.ok()?;
            client_async(url.as_str(), tcp).await.ok()
        })
        .await;
        let stream = match dialed {
            Ok(Some((stream, _response))) => Some(stream),
            Ok(None) => {
                debug!("truapi debug sink: dial/handshake failed, retrying");
                None
            }
            Err(_) => {
                debug!("truapi debug sink: handshake timed out, retrying");
                None
            }
        };
        let Some(stream) = stream else {
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
            // The sink was dropped while we were retrying: give up.
            if inbox.is_closed() {
                return;
            }
            continue;
        };
        let (mut write, mut read) = stream.split();
        // Drain queued frames to the wire, and also poll the read half so
        // tokio-tungstenite answers server pings and observes a Close; being
        // forward-only, any inbound message is ignored. Reset backoff only on a
        // *delivered* frame, so an accept-then-close server still backs off
        // instead of spinning on zero-delay reconnects.
        loop {
            tokio::select! {
                queued = inbox.recv() => match queued {
                    Some(line) => match write.send(Message::Text(line)).await {
                        Ok(()) => backoff = INITIAL_BACKOFF,
                        Err(_) => {
                            debug!("truapi debug sink: socket closed, reconnecting");
                            // The in-flight line is lost across this reconnect.
                            dropped.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    },
                    // All senders dropped: the sink is gone, so is the host. Done.
                    None => return,
                },
                inbound = read.next() => match inbound {
                    Some(Ok(_)) => {} // forward-only: ignore any inbound message
                    Some(Err(_)) | None => {
                        debug!("truapi debug sink: read side closed, reconnecting");
                        break;
                    }
                },
            }
        }
        // Reconnect after an established socket dropped: back off here too.
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
        if inbox.is_closed() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_core::{ChannelId, FrameDirection};

    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio_tungstenite::accept_async;

    #[tokio::test]
    async fn emits_base64_envelope_with_product_vantage_dir() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Server side: accept one connection, capture the first text message.
        let (tx, rx) = oneshot::channel::<String>();
        tokio::spawn(async move {
            let (stream, _peer) = listener.accept().await.unwrap();
            let ws = accept_async(stream).await.unwrap();
            let (_write, mut read) = ws.split();
            let message = read.next().await.unwrap().unwrap();
            tx.send(message.into_text().unwrap()).unwrap();
        });

        let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
        // `In` = product→core, i.e. the frame *left* the product → product-vantage "out".
        sink.emit(DebugEvent::Frame {
            channel_id: ChannelId("myapp.dot".to_string()),
            dir: FrameDirection::In,
            bytes: vec![1, 2, 3, 4],
        });

        let text = tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .expect("debugger did not receive a frame")
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(value["channelId"], "myapp.dot");
        // Guard against re-inversion: In must serialize as product-vantage "out".
        assert_eq!(value["dir"], FrameDirection::In.wire_str());
        assert_eq!(value["dir"], "out");
        assert_eq!(value["frame"], BASE64.encode([1, 2, 3, 4]));
    }

    #[test]
    fn rejects_non_loopback_and_non_ws_urls() {
        // 192.0.2.1 (TEST-NET-1) is a non-loopback IP literal, so no DNS is hit.
        assert!(WsDebugSink::connect("wss://127.0.0.1:9231").is_err());
        assert!(WsDebugSink::connect("ws://192.0.2.1:9231").is_err());
        assert!(WsDebugSink::connect("http://127.0.0.1:9231").is_err());
        assert!(WsDebugSink::connect("not a url").is_err());
    }

    #[tokio::test]
    async fn accepts_loopback_forms() {
        for url in [
            "ws://127.0.0.1:9231",
            "ws://localhost:9231",
            "ws://[::1]:9231",
        ] {
            assert!(WsDebugSink::connect(url).is_ok(), "should accept {url}");
        }
    }

    #[tokio::test]
    async fn emit_is_nonblocking_and_counts_drops_when_debugger_absent() {
        // A loopback port with nothing listening: dials never succeed, so the
        // bounded queue fills and further frames are dropped, never blocking emit.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // free the port; nothing is listening now

        let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
        for _ in 0..(QUEUE_CAPACITY + 50) {
            sink.emit(DebugEvent::Frame {
                channel_id: ChannelId("myapp.dot".to_string()),
                dir: FrameDirection::Out,
                bytes: vec![1],
            });
        }
        assert!(
            sink.dropped() > 0,
            "a full queue must count drops, not block"
        );
    }
}
