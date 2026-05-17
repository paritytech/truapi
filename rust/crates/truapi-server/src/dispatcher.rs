//! Request dispatcher.
//!
//! Routes incoming frames to the appropriate trait method based on the
//! action tag. The handler set is registered by the auto-generated
//! [`crate::generated::dispatcher::register`] function; this module
//! provides the framework that owns the registration table and the
//! routing logic.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use futures::future::LocalBoxFuture;

use crate::frame::{FrameKind, Payload, ProtocolMessage, compose_action};
use crate::subscription::{SubscriptionManager, SubscriptionStream};
use crate::transport::Transport;

/// Latest wire-codec version this server implements. Used as the default
/// negotiated version until the system `handshake` resolves; clients on
/// the same major version keep working without an explicit handshake.
pub const LATEST_PROTOCOL_VERSION: u8 = 1;

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

/// Routes incoming protocol messages to registered handlers.
pub struct Dispatcher {
    request_handlers: HashMap<String, RequestHandler>,
    subscription_handlers: HashMap<String, SubscriptionHandler>,
    subscriptions: SubscriptionManager,
    /// Wire-codec version negotiated through `handshake`. The handshake
    /// handler stores the requested version here (writing through the
    /// shared `Arc`) so subsequent responses encode in matching wire bytes.
    negotiated_version: Arc<AtomicU8>,
}

impl Dispatcher {
    /// Construct a dispatcher with a fresh negotiated-version slot
    /// (defaults to [`LATEST_PROTOCOL_VERSION`]).
    pub fn new() -> Self {
        Self::with_negotiated_version(Arc::new(AtomicU8::new(LATEST_PROTOCOL_VERSION)))
    }

    /// Construct a dispatcher that shares its negotiated-version slot
    /// with another component (typically the host's handshake handler).
    pub fn with_negotiated_version(negotiated_version: Arc<AtomicU8>) -> Self {
        Self {
            request_handlers: HashMap::new(),
            subscription_handlers: HashMap::new(),
            subscriptions: SubscriptionManager::new(),
            negotiated_version,
        }
    }

    /// Clone the shared negotiated-version slot. The handshake handler
    /// writes to this; per-method registration captures it for response
    /// wrapping.
    pub fn negotiated_version(&self) -> Arc<AtomicU8> {
        self.negotiated_version.clone()
    }

    /// Register a request-response handler for a method. Returns the
    /// previously registered handler if any; callers (the generated
    /// `dispatcher::register`) should treat `Some` as a programming error
    /// since each wire method must own exactly one handler.
    pub fn on_request<F>(&mut self, method: &str, handler: F) -> Option<RequestHandler>
    where
        F: Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<Vec<u8>, Vec<u8>>>
            + Send
            + Sync
            + 'static,
    {
        self.request_handlers
            .insert(method.to_string(), Arc::new(handler))
    }

    /// Register a subscription handler for a method. Returns the previously
    /// registered handler if any.
    pub fn on_subscription<F>(&mut self, method: &str, handler: F) -> Option<SubscriptionHandler>
    where
        F: Fn(String, Vec<u8>) -> LocalBoxFuture<'static, Result<SubscriptionStream, Vec<u8>>>
            + Send
            + Sync
            + 'static,
    {
        self.subscription_handlers
            .insert(method.to_string(), Arc::new(handler))
    }

    /// Process an incoming protocol message, sending any responses or
    /// subscription frames through `transport`.
    pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
        let Some((method, kind)) = FrameKind::from_tag(&message.payload.tag) else {
            return;
        };

        match kind {
            FrameKind::Request => {
                if let Some(handler) = self.request_handlers.get(&method) {
                    let request_id = message.request_id.clone();
                    let result = handler(request_id, message.payload.value).await;
                    // On the wire, every response is `Result<Ok, Err>`-shaped:
                    // the handler returns `Ok(bytes)` already prefixed with a
                    // `0x00` discriminant for success, and `Err(bytes)` whose
                    // bytes are the SCALE-encoded `CallError`. The error path
                    // prepends `0x01` here so the wire payload is always
                    // `[disc][value...]`.
                    let payload = match result {
                        Ok(value) => Payload {
                            tag: compose_action(&method, FrameKind::Response),
                            value,
                        },
                        Err(err_bytes) => {
                            let mut value = Vec::with_capacity(1 + err_bytes.len());
                            value.push(1u8);
                            value.extend_from_slice(&err_bytes);
                            Payload {
                                tag: compose_action(&method, FrameKind::Response),
                                value,
                            }
                        }
                    };
                    transport.send(ProtocolMessage {
                        request_id: message.request_id,
                        payload,
                    });
                }
            }
            FrameKind::Start => {
                if let Some(handler) = self.subscription_handlers.get(&method) {
                    let request_id = message.request_id.clone();
                    let stream_result = handler(request_id, message.payload.value).await;
                    match stream_result {
                        Ok(stream) => {
                            self.subscriptions.register(
                                message.request_id,
                                &method,
                                stream,
                                transport,
                            );
                        }
                        Err(err_bytes) => {
                            transport.send(ProtocolMessage {
                                request_id: message.request_id,
                                payload: Payload {
                                    tag: compose_action(&method, FrameKind::Interrupt),
                                    value: err_bytes,
                                },
                            });
                        }
                    }
                }
            }
            FrameKind::Stop => {
                self.subscriptions.handle_stop(&message.request_id);
            }
            // Response, Receive, Interrupt are handled by the client side.
            _ => {}
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;
    use std::sync::Mutex;
    use truapi::CallError;

    use crate::frame::{FrameKind, Payload, compose_action};

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

    fn make_frame(tag: &str, value: Vec<u8>) -> ProtocolMessage {
        ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                tag: tag.to_string(),
                value,
            },
        }
    }

    /// An unregistered method discriminant must not be answered. The
    /// dispatcher reaches into its handler maps; a miss should be a silent
    /// drop rather than panicking or sending a bogus response.
    #[test]
    fn dispatch_unknown_method_silently_drops() {
        let dispatcher = Dispatcher::new();
        let transport = Arc::new(RecordingTransport::default());
        let transport_dyn: Arc<dyn Transport> = transport.clone();
        let frame = make_frame(
            &compose_action("missing_method", FrameKind::Request),
            Vec::new(),
        );
        futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
        assert!(
            transport.sent().is_empty(),
            "unknown method must not send any frames"
        );
    }

    /// A handler that returns `Err(CallError::Denied)` must produce a
    /// response frame whose payload begins with the `0x01` Err
    /// discriminant byte (the Result wire shape).
    #[test]
    fn dispatch_request_handler_error_emits_response_with_err_discriminant() {
        let mut dispatcher = Dispatcher::new();
        dispatcher.on_request("fake_method", |_request_id, _bytes| {
            Box::pin(async move {
                let err: CallError<()> = CallError::Denied;
                Err(crate::frame::encode_call_error_payload(err))
            })
        });
        let transport = Arc::new(RecordingTransport::default());
        let frame = make_frame(
            &compose_action("fake_method", FrameKind::Request),
            Vec::new(),
        );
        futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "exactly one response expected");
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
        let mut dispatcher = Dispatcher::new();
        let prev = dispatcher.on_request("fake_method", |_request_id, _bytes| {
            Box::pin(async move { Ok(Vec::new()) })
        });
        assert!(prev.is_none(), "first registration has no predecessor");
        let prev = dispatcher.on_request("fake_method", |_request_id, _bytes| {
            Box::pin(async move { Ok(Vec::new()) })
        });
        assert!(
            prev.is_some(),
            "second registration must return the previous handler"
        );
    }
}
