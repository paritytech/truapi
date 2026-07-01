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
use tracing::instrument;

use crate::frame::{Payload, ProtocolMessage};
use crate::generated::wire_table::{RequestFrameIds, SubscriptionFrameIds};
use crate::subscription::{Spawner, SubscriptionManager, SubscriptionStream};
use crate::transport::Transport;

/// A handler for a request-response method. The returned future is not
/// required to be `Send` because the truapi trait uses `async fn`, whose
/// auto-Send-ness is not guaranteed. The `request_id` is the per-frame
/// identifier; handlers thread it into the `CallContext` so trait methods
/// can correlate logs/cancellation with the originating request. On the
/// error path handlers return the complete SCALE-encoded response payload.
pub type RequestHandler =
    Arc<dyn Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<Vec<u8>, Vec<u8>>> + Send + Sync>;

/// A handler for a subscription method. On the error path the handler returns
/// the complete SCALE-encoded `_interrupt` payload.
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
    #[instrument(skip_all, fields(runtime.method = "dispatcher.dispatch"))]
    pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
        let id = message.payload.id;

        if let Some(entry) = self.by_request.get(&id) {
            let request_id = message.request_id.clone();
            let value = (entry.handler)(request_id, message.payload.value)
                .await
                .unwrap_or_else(|value| value);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    /// A handler error already owns the complete response payload. The
    /// dispatcher only routes it to the registered response id.
    #[test]
    fn dispatch_request_handler_error_emits_response_payload() {
        let mut dispatcher = Dispatcher::new(test_spawner());
        let ids = RequestFrameIds {
            request_id: 200,
            response_id: 201,
        };
        dispatcher.on_request(ids, |_request_id, _bytes| {
            Box::pin(async move { Err(vec![9, 8, 7]) })
        });
        let transport = Arc::new(RecordingTransport::default());
        let frame = make_frame(200, Vec::new());
        futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "exactly one response expected");
        assert_eq!(sent[0].payload.id, 201);
        assert_eq!(sent[0].payload.value, vec![9, 8, 7]);
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
