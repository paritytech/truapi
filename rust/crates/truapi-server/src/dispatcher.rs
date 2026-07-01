//! Request dispatcher.
//!
//! Routes incoming frames to the appropriate trait method based on the
//! numeric wire discriminant. The handler set is registered by the
//! auto-generated [`crate::generated::dispatcher::register`] function; this
//! module provides the framework that owns the registration tables and the
//! routing logic.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::future::LocalBoxFuture;

use crate::frame::{Payload, ProtocolMessage};
use crate::generated::wire_table::{RequestFrameIds, SubscriptionFrameIds};
use crate::subscription::{Spawner, SubscriptionManager, SubscriptionStream};
use crate::transport::Transport;

/// A handler for a request-response method. The returned future is not
/// required to be `Send` because the truapi trait uses `async fn`, whose
/// auto-Send-ness is not guaranteed. The `request_id` is the per-frame
/// identifier; handlers thread it into the `CallContext` so trait methods
/// can correlate logs/cancellation with the originating request. On the
/// error path handlers return the SCALE-encoded `CallError` payload bytes
/// (typically via [`crate::frame::encode_decode_error`] or
/// [`crate::frame::encode_call_error_payload`]); the dispatcher wraps them
/// into the response envelope.
pub type RequestHandler =
    Arc<dyn Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<Vec<u8>, Vec<u8>>> + Send + Sync>;

/// A handler for a subscription method. On the error path the handler
/// returns the SCALE-encoded `CallError` payload bytes; the dispatcher
/// wraps them into an `_interrupt` envelope.
pub type SubscriptionHandler = Arc<
    dyn Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<SubscriptionStream, Vec<u8>>>
        + Send
        + Sync,
>;

/// A registered request handler plus the discriminants it replies on.
pub struct RequestEntry {
    ids: RequestFrameIds,
    handler: RequestHandler,
}

/// A registered subscription handler plus the discriminants its frames carry.
pub struct SubscriptionEntry {
    ids: SubscriptionFrameIds,
    handler: SubscriptionHandler,
}

/// Routes incoming protocol messages to registered handlers, keyed on the
/// numeric wire discriminant.
pub struct Dispatcher {
    by_request: HashMap<u8, RequestEntry>,
    by_start: HashMap<u8, SubscriptionEntry>,
    stop_ids: HashSet<u8>,
    subscriptions: SubscriptionManager,
}

impl Dispatcher {
    /// Construct a dispatcher whose subscriptions are driven on `spawner`.
    pub fn new(spawner: Spawner) -> Self {
        Self {
            by_request: HashMap::new(),
            by_start: HashMap::new(),
            stop_ids: HashSet::new(),
            subscriptions: SubscriptionManager::new(spawner),
        }
    }

    /// Register a request-response handler, keyed on `ids.request_id`. Returns
    /// the previously registered entry if any; callers (the generated
    /// `dispatcher::register`) should treat `Some` as a programming error
    /// since each request id must own exactly one handler.
    pub fn on_request<F>(&mut self, ids: RequestFrameIds, handler: F) -> Option<RequestEntry>
    where
        F: Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<Vec<u8>, Vec<u8>>>
            + Send
            + Sync
            + 'static,
    {
        self.by_request.insert(
            ids.request_id,
            RequestEntry {
                ids,
                handler: Arc::new(handler),
            },
        )
    }

    /// Register a subscription handler, keyed on `ids.start_id`, and record
    /// `ids.stop_id` so a matching `_stop` frame tears the subscription down.
    /// Returns the previously registered entry if any.
    pub fn on_subscription<F>(
        &mut self,
        ids: SubscriptionFrameIds,
        handler: F,
    ) -> Option<SubscriptionEntry>
    where
        F: Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<SubscriptionStream, Vec<u8>>>
            + Send
            + Sync
            + 'static,
    {
        self.stop_ids.insert(ids.stop_id);
        self.by_start.insert(
            ids.start_id,
            SubscriptionEntry {
                ids,
                handler: Arc::new(handler),
            },
        )
    }

    /// Process an incoming protocol message, sending any responses or
    /// subscription frames through `transport`. A discriminant with no
    /// registered handler is dropped.
    pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
        let id = message.payload.id;

        if let Some(entry) = self.by_request.get(&id) {
            // On the wire, every response is `Result<Ok, Err>`-shaped: the
            // handler returns `Ok(bytes)` already prefixed with a `0x00`
            // discriminant for success, and `Err(bytes)` whose bytes are the
            // SCALE-encoded `CallError`. The error path prepends `0x01` so the
            // wire payload is always `[disc][value...]`.
            let request_id = message.request_id.clone();
            let value = match (entry.handler)(request_id, message.payload.value).await {
                Ok(value) => value,
                Err(err_bytes) => prefix_err(err_bytes),
            };
            transport.send(ProtocolMessage {
                request_id: message.request_id,
                payload: Payload {
                    id: entry.ids.response_id,
                    value,
                },
            });
        } else if let Some(entry) = self.by_start.get(&id) {
            // Reserve the slot before awaiting the handler so a `_stop`
            // arriving while the handler resolves cancels the pending
            // subscription instead of racing the registration.
            let token = self.subscriptions.reserve(message.request_id.clone());
            let request_id = message.request_id.clone();
            match (entry.handler)(request_id, message.payload.value).await {
                Ok(stream) => {
                    self.subscriptions.activate(
                        token,
                        entry.ids.receive_id,
                        entry.ids.interrupt_id,
                        stream,
                        transport,
                    );
                }
                Err(err_bytes) => {
                    self.subscriptions.cancel_reservation(token);
                    transport.send(ProtocolMessage {
                        request_id: message.request_id,
                        payload: Payload {
                            id: entry.ids.interrupt_id,
                            value: err_bytes,
                        },
                    });
                }
            }
        } else if self.stop_ids.contains(&id) {
            self.subscriptions.handle_stop(&message.request_id);
        }
        // Unknown discriminant: drop. Response / receive / interrupt frames are
        // handled by the client side and never registered here.
    }
}

/// Prepend the `0x01` Err discriminant to SCALE-encoded `CallError` bytes,
/// producing the `[disc][value...]` Result wire shape the response envelope
/// expects.
fn prefix_err(err_bytes: Vec<u8>) -> Vec<u8> {
    let mut value = Vec::with_capacity(1 + err_bytes.len());
    value.push(1u8);
    value.extend_from_slice(&err_bytes);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;
    use std::sync::Mutex;
    use truapi::CallError;

    fn test_spawner() -> Spawner {
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::subscription::thread_per_subscription_spawner()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Arc::new(futures::executor::block_on)
        }
    }

    #[derive(Default)]
    struct RecordingTransport {
        sent: Mutex<Vec<ProtocolMessage>>,
    }

    impl RecordingTransport {
        fn sent(&self) -> Vec<ProtocolMessage> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl Transport for RecordingTransport {
        fn send(&self, message: ProtocolMessage) {
            self.sent.lock().unwrap().push(message);
        }
        fn on_message(
            &self,
            _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
        ) -> Box<dyn FnOnce()> {
            Box::new(|| {})
        }
    }

    fn make_frame(id: u8, value: Vec<u8>) -> ProtocolMessage {
        ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload { id, value },
        }
    }

    /// A frame whose discriminant has no registered handler is dropped: no
    /// response, no interrupt. (In production `register` registers every wire
    /// method, so this only happens for malformed or client-bound ids.)
    #[test]
    fn dispatch_unregistered_id_sends_nothing() {
        let dispatcher = Dispatcher::new(test_spawner());
        let transport = Arc::new(RecordingTransport::default());
        let transport_dyn: Arc<dyn Transport> = transport.clone();
        let frame = make_frame(250, Vec::new());
        futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
        assert!(
            transport.sent().is_empty(),
            "an unregistered discriminant must produce no frame"
        );
    }

    /// A handler that returns `Err(CallError::Denied)` must produce a response
    /// frame on the registered `response_id` whose payload begins with the
    /// `0x01` Err discriminant byte (the Result wire shape).
    #[test]
    fn dispatch_request_handler_error_emits_response_with_err_discriminant() {
        let mut dispatcher = Dispatcher::new(test_spawner());
        let ids = RequestFrameIds {
            request_id: 200,
            response_id: 201,
        };
        dispatcher.on_request(ids, |_request_id, _bytes| {
            Box::pin(async move {
                let err: CallError<()> = CallError::Denied;
                Err(crate::frame::encode_call_error_payload(err))
            })
        });
        let transport = Arc::new(RecordingTransport::default());
        let frame = make_frame(200, Vec::new());
        futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "exactly one response expected");
        assert_eq!(sent[0].payload.id, 201);
        let payload = &sent[0].payload.value;
        assert_eq!(payload.first(), Some(&1u8), "first byte must be Err disc");
        // After the Err disc comes the SCALE-encoded CallError; `Denied` is
        // variant 1, so the full payload is `[0x01 disc][0x01 variant]`.
        let err: CallError<()> = CallError::Denied;
        let mut expected_inner = Vec::new();
        match &err {
            CallError::Denied => 1u8.encode_to(&mut expected_inner),
            _ => unreachable!(),
        }
        let mut expected = vec![1u8];
        expected.extend_from_slice(&expected_inner);
        assert_eq!(payload, &expected);
    }

    /// Registering two handlers under the same key must not silently
    /// overwrite. The contract chosen here is "loud": `on_request`
    /// returns the previous handler, so callers can detect collisions.
    #[test]
    fn register_request_twice_returns_previous_handler() {
        let mut dispatcher = Dispatcher::new(test_spawner());
        let ids = RequestFrameIds {
            request_id: 200,
            response_id: 201,
        };
        let prev = dispatcher.on_request(ids, |_request_id, _bytes| {
            Box::pin(async move { Ok(Vec::new()) })
        });
        assert!(prev.is_none(), "first registration has no predecessor");
        let prev = dispatcher.on_request(ids, |_request_id, _bytes| {
            Box::pin(async move { Ok(Vec::new()) })
        });
        assert!(
            prev.is_some(),
            "second registration must return the previous handler"
        );
    }
}
