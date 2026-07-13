//! Remote WebSocket JSON-RPC backend.
//!
//! The connection is a raw string pipe over a jsonrpsee WebSocket transport:
//! a writer task drains queued requests into the socket and the responses
//! stream yields every inbound text frame untouched. Reconnection is
//! deliberately not handled here — a dropped socket ends the responses
//! stream, and the consumer recovers by connecting again.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::mpsc;
use futures::stream::{self, AbortHandle, BoxStream, Stream, StreamExt};
use jsonrpsee_client_transport::ws::WsTransportClientBuilder;
use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use truapi::latest::GenericError;
use truapi_platform::JsonRpcConnection;
use url::Url;

/// Open a WebSocket connection to `url`.
///
/// Requires an ambient tokio runtime; both the handshake and the spawned
/// writer task run on it.
pub(crate) async fn connect(url: Url) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
    if tokio::runtime::Handle::try_current().is_err() {
        return Err(GenericError {
            reason: "the WebSocket backend requires an ambient tokio runtime".to_owned(),
        });
    }

    let (sender, receiver) = WsTransportClientBuilder::default()
        .build(url.clone())
        .await
        .map_err(|err| GenericError {
            reason: format!("WebSocket handshake with {url} failed: {err}"),
        })?;

    Ok(Box::new(WsConnection::start(sender, receiver)))
}

/// A live WebSocket connection exposed as a raw JSON-RPC pipe.
struct WsConnection {
    requests: mpsc::UnboundedSender<String>,
    responses: Mutex<Option<BoxStream<'static, String>>>,
    stream_abort: AbortHandle,
    closed: AtomicBool,
}

impl WsConnection {
    /// Spawn the writer task and set up the responses stream.
    ///
    /// Generic over the jsonrpsee transport traits so tests can inject
    /// in-memory transports.
    fn start<S, R>(sender: S, receiver: R) -> Self
    where
        S: TransportSenderT + Send,
        R: TransportReceiverT + Send,
    {
        let (requests, request_queue) = mpsc::unbounded();
        tokio::spawn(writer_pump(sender, request_queue));
        let (responses, stream_abort) = stream::abortable(response_stream(receiver));
        WsConnection {
            requests,
            responses: Mutex::new(Some(responses.boxed())),
            stream_abort,
            closed: AtomicBool::new(false),
        }
    }
}

impl JsonRpcConnection for WsConnection {
    fn send(&self, request: String) {
        // Infallible by contract: after a close or socket death the request
        // is dropped and the consumer notices via the ended responses stream.
        let _ = self.requests.unbounded_send(request);
    }

    fn responses(&self) -> BoxStream<'static, String> {
        match self
            .responses
            .lock()
            .expect("responses mutex poisoned")
            .take()
        {
            Some(responses) => responses,
            None => stream::empty().boxed(),
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.requests.close_channel();
        self.stream_abort.abort();
        self.responses
            .lock()
            .expect("responses mutex poisoned")
            .take();
    }
}

impl Drop for WsConnection {
    fn drop(&mut self) {
        self.close();
    }
}

/// Drain queued requests into the transport; on queue close, close the
/// transport gracefully.
async fn writer_pump<S: TransportSenderT>(
    mut sender: S,
    mut request_queue: mpsc::UnboundedReceiver<String>,
) {
    while let Some(request) = request_queue.next().await {
        if let Err(err) = sender.send(request).await {
            tracing::warn!("WebSocket send failed: {err}");
            return;
        }
    }
    if let Err(err) = sender.close().await {
        tracing::debug!("WebSocket close failed: {err}");
    }
}

/// Yield every inbound text frame; the stream ends on transport error or EOF,
/// which is the disconnect signal consumers rely on.
fn response_stream<R: TransportReceiverT + Send>(receiver: R) -> impl Stream<Item = String> + Send {
    stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.receive().await {
                Ok(ReceivedMessage::Text(text)) => return Some((text, receiver)),
                Ok(ReceivedMessage::Bytes(bytes)) => match String::from_utf8(bytes) {
                    Ok(text) => return Some((text, receiver)),
                    Err(_) => tracing::warn!("dropping non-UTF-8 binary WebSocket frame"),
                },
                Ok(ReceivedMessage::Pong) => {}
                Err(err) => {
                    tracing::debug!("WebSocket receive ended: {err}");
                    return None;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::channel::mpsc;
    use futures::stream::StreamExt;
    use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
    use truapi_platform::JsonRpcConnection;

    use super::WsConnection;

    /// Local error type so the fakes need no extra dev-deps.
    #[derive(Debug)]
    struct FakeError(&'static str);

    impl std::fmt::Display for FakeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for FakeError {}

    struct FakeSender {
        sent: mpsc::UnboundedSender<String>,
        closed: std::sync::Arc<Mutex<bool>>,
    }

    #[truapi_platform::async_trait]
    impl TransportSenderT for FakeSender {
        type Error = FakeError;

        async fn send(&mut self, msg: String) -> Result<(), Self::Error> {
            self.sent
                .unbounded_send(msg)
                .map_err(|_| FakeError("sink gone"))
        }

        async fn close(&mut self) -> Result<(), Self::Error> {
            *self.closed.lock().expect("lock") = true;
            Ok(())
        }
    }

    struct FakeReceiver {
        frames: mpsc::UnboundedReceiver<Result<ReceivedMessage, FakeError>>,
    }

    #[truapi_platform::async_trait]
    impl TransportReceiverT for FakeReceiver {
        type Error = FakeError;

        async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
            match self.frames.next().await {
                Some(frame) => frame,
                None => Err(FakeError("eof")),
            }
        }
    }

    struct Harness {
        connection: WsConnection,
        sent: mpsc::UnboundedReceiver<String>,
        frames: mpsc::UnboundedSender<Result<ReceivedMessage, FakeError>>,
        sender_closed: std::sync::Arc<Mutex<bool>>,
    }

    fn harness() -> Harness {
        let (sent_tx, sent_rx) = mpsc::unbounded();
        let (frames_tx, frames_rx) = mpsc::unbounded();
        let sender_closed = std::sync::Arc::new(Mutex::new(false));
        let connection = WsConnection::start(
            FakeSender {
                sent: sent_tx,
                closed: std::sync::Arc::clone(&sender_closed),
            },
            FakeReceiver { frames: frames_rx },
        );
        Harness {
            connection,
            sent: sent_rx,
            frames: frames_tx,
            sender_closed,
        }
    }

    #[tokio::test]
    async fn requests_reach_the_transport() {
        let mut harness = harness();
        harness.connection.send("one".to_owned());
        harness.connection.send("two".to_owned());
        assert_eq!(harness.sent.next().await.as_deref(), Some("one"));
        assert_eq!(harness.sent.next().await.as_deref(), Some("two"));
    }

    #[tokio::test]
    async fn text_and_utf8_bytes_are_yielded_and_pongs_skipped() {
        let harness = harness();
        let mut responses = harness.connection.responses();
        harness
            .frames
            .unbounded_send(Ok(ReceivedMessage::Pong))
            .expect("send frame");
        harness
            .frames
            .unbounded_send(Ok(ReceivedMessage::Text("hello".to_owned())))
            .expect("send frame");
        harness
            .frames
            .unbounded_send(Ok(ReceivedMessage::Bytes(b"raw".to_vec())))
            .expect("send frame");
        assert_eq!(responses.next().await.as_deref(), Some("hello"));
        assert_eq!(responses.next().await.as_deref(), Some("raw"));
    }

    #[tokio::test]
    async fn receive_error_ends_the_stream() {
        let harness = harness();
        let mut responses = harness.connection.responses();
        harness
            .frames
            .unbounded_send(Err(FakeError("boom")))
            .expect("send frame");
        assert_eq!(responses.next().await, None);
    }

    #[tokio::test]
    async fn second_responses_call_is_empty() {
        let harness = harness();
        let _live = harness.connection.responses();
        let mut second = harness.connection.responses();
        assert_eq!(second.next().await, None);
    }

    #[tokio::test]
    async fn close_is_idempotent_ends_stream_and_closes_transport() {
        let mut harness = harness();
        let mut responses = harness.connection.responses();
        harness.connection.close();
        harness.connection.close();
        assert_eq!(responses.next().await, None);
        harness.connection.send("late".to_owned());
        // The writer drains the closed queue and then closes the transport.
        assert_eq!(harness.sent.next().await, None);
        tokio::task::yield_now().await;
        assert!(*harness.sender_closed.lock().expect("lock"));
    }

    #[tokio::test]
    async fn drop_closes_the_connection() {
        let harness = harness();
        let mut responses = harness.connection.responses();
        drop(harness.connection);
        assert_eq!(responses.next().await, None);
    }
}
