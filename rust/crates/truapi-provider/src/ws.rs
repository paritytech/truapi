//! Remote WebSocket JSON-RPC backend.
//!
//! The connection is a raw string pipe over a jsonrpsee WebSocket transport:
//! a writer task drains queued requests into the socket and the responses
//! stream yields every inbound text frame untouched. Reconnection is
//! deliberately not handled here — a dropped socket ends the responses
//! stream, and the consumer recovers by connecting again.

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use futures::channel::mpsc;
use futures::stream::{self, AbortHandle, BoxStream, Stream, StreamExt};
use jsonrpsee_client_transport::ws::WsTransportClientBuilder;
use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use truapi_platform::JsonRpcConnection;
use url::Url;

use crate::error::ProviderError;

/// Bounded depth of the outbound request buffer. The sole producer is a trusted
/// in-process consumer, so this rarely fills; when it does the socket is not
/// draining, and [`send`](WsConnection::send) ends the response stream rather
/// than letting the buffer grow without bound.
const REQUEST_BUFFER: usize = 1024;

/// Open a WebSocket connection to `url`.
///
/// Requires an ambient tokio runtime; both the handshake and the spawned
/// writer task run on it.
pub(crate) async fn connect(url: Url) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
    if tokio::runtime::Handle::try_current().is_err() {
        return Err(ProviderError::MissingRuntime);
    }

    let (sender, receiver) = WsTransportClientBuilder::default()
        .build(url.clone())
        .await
        .map_err(|err| ProviderError::Handshake {
            url: url.to_string(),
            reason: err.to_string(),
        })?;

    Ok(Box::new(WsConnection::start(sender, receiver)))
}

/// A live WebSocket connection exposed as a raw JSON-RPC pipe.
struct WsConnection {
    requests: Mutex<mpsc::Sender<String>>,
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
        Self::start_with_buffer(sender, receiver, REQUEST_BUFFER)
    }

    /// [`start`](Self::start) with an explicit request-buffer depth, so tests
    /// can force an overflow deterministically.
    fn start_with_buffer<S, R>(sender: S, receiver: R, buffer: usize) -> Self
    where
        S: TransportSenderT + Send,
        R: TransportReceiverT + Send,
    {
        let (requests, request_queue) = mpsc::channel(buffer);
        let (responses, stream_abort) = stream::abortable(response_stream(receiver));
        tokio::spawn(writer_pump(sender, request_queue, stream_abort.clone()));
        WsConnection {
            requests: Mutex::new(requests),
            responses: Mutex::new(Some(responses.boxed())),
            stream_abort,
            closed: AtomicBool::new(false),
        }
    }
}

impl JsonRpcConnection for WsConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        // A full buffer means the socket is not draining, and a disconnected
        // channel means the writer is already gone. Either way, end the
        // response stream so the id-correlating consumer reconnects rather than
        // buffering without bound or waiting on a request that will not be sent.
        if self
            .requests
            .lock()
            .expect("requests mutex poisoned")
            .try_send(request)
            .is_err()
        {
            self.close();
        }
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
        self.requests
            .lock()
            .expect("requests mutex poisoned")
            .close_channel();
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
///
/// A send failure means the socket is gone, so it also aborts the response
/// stream: without that, in-flight requests would never be answered and the
/// consumer — which correlates responses by id — would hang. Ending the stream
/// is the disconnect signal it acts on.
async fn writer_pump<S: TransportSenderT>(
    mut sender: S,
    mut request_queue: mpsc::Receiver<String>,
    stream_abort: AbortHandle,
) {
    while let Some(request) = request_queue.next().await {
        if let Err(err) = sender.send(request).await {
            tracing::warn!("WebSocket send failed: {err}");
            stream_abort.abort();
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

    /// Sender whose `send` always fails, modelling a dead socket.
    struct FailingSender;

    #[truapi_platform::async_trait]
    impl TransportSenderT for FailingSender {
        type Error = FakeError;

        async fn send(&mut self, _msg: String) -> Result<(), Self::Error> {
            Err(FakeError("dead socket"))
        }

        async fn close(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    /// Sender whose `send` never resolves, so the writer parks after taking one
    /// request and the bounded buffer fills.
    struct StalledSender;

    #[truapi_platform::async_trait]
    impl TransportSenderT for StalledSender {
        type Error = FakeError;

        async fn send(&mut self, _msg: String) -> Result<(), Self::Error> {
            core::future::pending().await
        }

        async fn close(&mut self) -> Result<(), Self::Error> {
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
    async fn writer_failure_ends_the_response_stream() {
        // A dead socket (send fails) must end the responses stream so the
        // consumer sees a disconnect instead of hanging on the request.
        let (_frames_tx, frames_rx) = mpsc::unbounded::<Result<ReceivedMessage, FakeError>>();
        let connection = WsConnection::start(FailingSender, FakeReceiver { frames: frames_rx });
        let mut responses = connection.responses();
        connection.send("req".to_owned());
        assert_eq!(responses.next().await, None);
    }

    #[tokio::test]
    async fn a_full_request_buffer_ends_the_response_stream() {
        // The writer stalls on the first request, so a tiny buffer fills and a
        // later send overflows; the consumer must see the stream end, not hang.
        let (_frames_tx, frames_rx) = mpsc::unbounded::<Result<ReceivedMessage, FakeError>>();
        let connection =
            WsConnection::start_with_buffer(StalledSender, FakeReceiver { frames: frames_rx }, 2);
        let mut responses = connection.responses();
        for _ in 0..64 {
            connection.send("req".to_owned());
        }
        assert_eq!(responses.next().await, None);
    }

    #[tokio::test]
    async fn drop_closes_the_connection() {
        let harness = harness();
        let mut responses = harness.connection.responses();
        drop(harness.connection);
        assert_eq!(responses.next().await, None);
    }
}
