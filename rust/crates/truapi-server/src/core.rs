//! `TrUApiCore`: the entrypoint a host wraps around a `truapi::api::TrUApi`
//! implementation (direct path) or a `truapi_platform::Platform`
//! implementation (platform path).
//!
//! Direct path: `TrUApiCore::new(host)` accepts anything implementing
//! the unified [`truapi::api::TrUApi`] super-trait. Useful for unit tests
//! and bespoke hosts.
//!
//! Platform path: [`TrUApiCore::from_platform_with_config`] takes a
//! [`truapi_platform::Platform`] and wires it through
//! [`crate::runtime::PlatformRuntimeHost`] before registering with the
//! generated dispatcher. This is the path real platform shims (UniFFI,
//! wasm-bindgen, ws-bridge, ...) take.

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use parity_scale_codec::{Decode, Encode};
use tracing::instrument;
use truapi::api::TrUApi;
use truapi::v01;
use truapi::versioned::chat::HostChatActionSubscribeItem;
use truapi_platform::{Platform, RuntimeConfig};

use crate::generated::dispatcher;
use crate::host_logic::session::SessionState;
use crate::runtime::{ChatEvents, PlatformRuntimeHost};
use crate::subscription::Spawner;
use crate::{Dispatcher, ProtocolMessage, Transport};

type DisconnectFn = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;
type CancelLoginFn = Arc<dyn Fn() + Send + Sync>;

/// Top-level core. Owns the dispatcher and, on the platform path, the shared
/// session-state holder.
pub struct TrUApiCore {
    dispatcher: Dispatcher,
    /// Always present; empty for [`Self::new`] (direct host path), connected
    /// to a [`PlatformRuntimeHost`] for [`Self::from_platform_with_config`].
    session_state: Arc<SessionState>,
    chat_events: Arc<ChatEvents>,
    disconnect: DisconnectFn,
    cancel_login: CancelLoginFn,
}

impl TrUApiCore {
    /// Build a core around a direct `TrUApi` implementation. The session
    /// state holder is unused on this path (no platform pushes updates),
    /// but is created anyway so the public API surface stays consistent.
    /// Subscription work runs on `spawner`.
    #[instrument(skip_all, fields(runtime.method = "core.new"))]
    pub fn new<P>(host: Arc<P>, spawner: Spawner) -> Self
    where
        P: TrUApi + 'static,
    {
        let mut dispatcher = Dispatcher::new(spawner);
        dispatcher::register(&mut dispatcher, host);
        let session_state = SessionState::new();
        let disconnect_state = session_state.clone();
        Self {
            dispatcher,
            session_state,
            chat_events: Arc::new(ChatEvents::default()),
            disconnect: Arc::new(move || {
                let state = disconnect_state.clone();
                Box::pin(async move {
                    state.clear_session();
                })
            }),
            cancel_login: Arc::new(|| {}),
        }
    }

    /// Build a core around a [`Platform`] implementation and explicit product
    /// runtime configuration.
    #[instrument(skip_all, fields(runtime.method = "core.from_platform_with_config"))]
    pub fn from_platform_with_config<P>(
        platform: Arc<P>,
        runtime_config: RuntimeConfig,
        spawner: Spawner,
    ) -> Self
    where
        P: Platform + 'static,
    {
        let runtime = Arc::new(PlatformRuntimeHost::new(
            platform,
            runtime_config,
            spawner.clone(),
        ));
        runtime.start_session_store_sync(spawner.clone());
        let session_state = runtime.session_state();
        let chat_events = runtime.chat_events();
        let disconnect_runtime = runtime.clone();
        let cancel_login_runtime = runtime.clone();
        let mut dispatcher = Dispatcher::new(spawner);
        dispatcher::register(&mut dispatcher, runtime);
        Self {
            dispatcher,
            session_state,
            chat_events,
            disconnect: Arc::new(move || {
                let runtime = disconnect_runtime.clone();
                Box::pin(async move {
                    runtime.disconnect().await;
                })
            }),
            cancel_login: Arc::new(move || cancel_login_runtime.cancel_login()),
        }
    }

    /// Handle to the shared session-state holder used by subscriptions and
    /// tests. Real host lifecycle flows through `SessionStore` and
    /// `disconnect`.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Publish a plain-text chat message to active `chat.actionSubscribe()`
    /// subscribers. Platform-backed cores use this for host-originated chat
    /// input; direct cores have no internal chat event stream to dispatch to.
    pub fn notify_chat_message_posted(&self, room_id: String, peer: String, text: String) {
        self.chat_events
            .notify_action(HostChatActionSubscribeItem::V1(
                v01::HostChatActionSubscribeItem {
                    room_id,
                    peer,
                    payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
                        text,
                    }),
                },
            ));
    }

    /// Publish a custom chat message to active `chat.actionSubscribe()`
    /// subscribers.
    pub fn notify_chat_custom_message_posted(
        &self,
        room_id: String,
        peer: String,
        message_type: String,
        payload: Vec<u8>,
    ) {
        self.chat_events
            .notify_action(HostChatActionSubscribeItem::V1(
                v01::HostChatActionSubscribeItem {
                    room_id,
                    peer,
                    payload: v01::ChatActionPayload::MessagePosted(
                        v01::ChatMessageContent::Custom(v01::ChatCustomMessage {
                            message_type,
                            payload,
                        }),
                    ),
                },
            ));
    }

    /// Publish a chat action-button trigger to active
    /// `chat.actionSubscribe()` subscribers.
    pub fn notify_chat_action_triggered(
        &self,
        room_id: String,
        peer: String,
        message_id: String,
        action_id: String,
        payload: Option<Vec<u8>>,
    ) {
        self.chat_events
            .notify_action(HostChatActionSubscribeItem::V1(
                v01::HostChatActionSubscribeItem {
                    room_id,
                    peer,
                    payload: v01::ChatActionPayload::ActionTriggered(v01::ActionTrigger {
                        message_id,
                        action_id,
                        payload,
                    }),
                },
            ));
    }

    /// Core-owned logout/disconnect. Platform-backed cores best-effort notify
    /// the SSO peer and clear the host-global session store; direct cores only
    /// clear their in-memory session state.
    #[instrument(skip_all, fields(runtime.method = "core.disconnect"))]
    pub async fn disconnect_async(&self) {
        (self.disconnect)().await;
    }

    /// Blocking wrapper for embedders that do not drive async directly.
    #[instrument(skip_all, fields(runtime.method = "core.disconnect_blocking"))]
    pub fn disconnect(&self) {
        futures::executor::block_on(self.disconnect_async());
    }

    /// Cancel any in-flight `request_login` pairing. The host UI receives a
    /// `Disconnected` auth state immediately and the pending login resolves
    /// to `Rejected`. A no-op when no login is in progress (and always a
    /// no-op on the direct host path).
    #[instrument(skip_all, fields(runtime.method = "core.cancel_login"))]
    pub fn cancel_login(&self) {
        (self.cancel_login)();
    }

    /// Asynchronous form of [`Self::receive_from_product`]. Decodes the
    /// incoming frame, runs it through the dispatcher, and returns the
    /// SCALE-encoded response (if any).
    #[instrument(skip_all, fields(runtime.method = "core.receive_from_product"))]
    pub async fn receive_from_product_async(&self, frame: &[u8]) -> Option<Vec<u8>> {
        let message = ProtocolMessage::decode(&mut &*frame).ok()?;
        let transport = Arc::new(ResponseTransport::default());
        self.dispatcher
            .dispatch(message, transport.clone() as Arc<dyn Transport>)
            .await;
        transport.take().map(|response| response.encode())
    }

    /// Synchronous wrapper that blocks the current thread until the inner
    /// future resolves. Convenient for embedding contexts (e.g. UniFFI) that
    /// don't already drive an async runtime.
    #[instrument(skip_all, fields(runtime.method = "core.receive_from_product_blocking"))]
    pub fn receive_from_product(&self, frame: &[u8]) -> Option<Vec<u8>> {
        futures::executor::block_on(self.receive_from_product_async(frame))
    }

    /// Dispatch an already-decoded protocol message through the underlying
    /// dispatcher. Bridges that own a long-lived transport (e.g. WebSocket,
    /// JS callback) call this directly so subscription items flow back
    /// through the bridge transport instead of the single-slot capture used
    /// by [`Self::receive_from_product`].
    #[instrument(skip_all, fields(runtime.method = "core.dispatch"))]
    pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
        self.dispatcher.dispatch(message, transport).await;
    }
}

/// Single-slot transport that captures the next response the dispatcher
/// emits. Used by [`TrUApiCore::receive_from_product`] to bridge between the
/// dispatcher's push model and the synchronous "decode in, encoded out"
/// shape exposed to embedders.
#[derive(Default)]
struct ResponseTransport {
    response: Mutex<Option<ProtocolMessage>>,
}

impl ResponseTransport {
    fn take(&self) -> Option<ProtocolMessage> {
        self.response.lock().unwrap().take()
    }
}

impl Transport for ResponseTransport {
    fn send(&self, message: ProtocolMessage) {
        *self.response.lock().unwrap() = Some(message);
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
    use truapi::versioned::chat::{
        HostChatCreateRoomRequest, HostChatCreateRoomResponse, HostChatPostMessageRequest,
        HostChatPostMessageResponse,
    };
    use truapi::versioned::local_storage::{
        HostLocalStorageClearRequest, HostLocalStorageReadRequest, HostLocalStorageWriteRequest,
    };
    use truapi::versioned::notifications::HostPushNotificationRequest;
    use truapi::versioned::payment::{
        HostPaymentBalanceSubscribeItem, HostPaymentBalanceSubscribeRequest, HostPaymentRequest,
        HostPaymentResponse, HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeRequest,
        HostPaymentTopUpRequest, HostPaymentTopUpResponse,
    };
    use truapi::versioned::permissions::RemotePermissionRequest;
    use truapi::versioned::system::HostFeatureSupportedRequest;

    use crate::frame::{Payload, request_ids, subscription_ids};
    use crate::test_support::{StubPlatform, runtime_config, test_spawner};

    #[test]
    fn from_platform_dispatches_feature_supported() {
        let core = TrUApiCore::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            runtime_config("dotli.dot"),
            test_spawner(),
        );
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let ids = request_ids("system_feature_supported").expect("known request method");
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        };
        let encoded = frame.encode();
        let response_bytes = core
            .receive_from_product(&encoded)
            .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(response.payload.id, ids.response_id);
        // Wire payload is `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);
    }

    /// Drive a request frame through `TrUApiCore::receive_from_product`,
    /// decode the response envelope, and return its payload bytes (without
    /// the wrapping ProtocolMessage). Shared by the runtime-delegation
    /// tests below.
    fn run_request(core: &TrUApiCore, method: &str, request_bytes: Vec<u8>) -> Vec<u8> {
        let ids = request_ids(method).expect("known request method");
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request_bytes,
            },
        };
        let response_bytes = core
            .receive_from_product(&frame.encode())
            .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(response.payload.id, ids.response_id);
        response.payload.value
    }

    fn make_core() -> TrUApiCore {
        TrUApiCore::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            runtime_config("dotli.dot"),
            test_spawner(),
        )
    }

    struct ChannelTransport {
        tx: std::sync::mpsc::Sender<ProtocolMessage>,
    }

    impl Transport for ChannelTransport {
        fn send(&self, message: ProtocolMessage) {
            self.tx
                .send(message)
                .expect("test transport receives message");
        }

        fn on_message(
            &self,
            _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
        ) -> Box<dyn FnOnce()> {
            Box::new(|| {})
        }
    }

    #[test]
    fn chat_action_subscribe_receives_core_notified_message() {
        let core = make_core();
        let (tx, rx) = std::sync::mpsc::channel();
        let transport = Arc::new(ChannelTransport { tx });
        let sub_ids = subscription_ids("chat_action_subscribe").expect("known subscription");
        let start = ProtocolMessage {
            request_id: "chat:1".into(),
            payload: Payload {
                id: sub_ids.start_id,
                value: Vec::new(),
            },
        };

        futures::executor::block_on(core.dispatch(start, transport));
        core.notify_chat_message_posted(
            "room-1".to_string(),
            "peer-1".to_string(),
            "hello".to_string(),
        );

        let message = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("subscription receive frame");
        assert_eq!(message.request_id, "chat:1");
        assert_eq!(message.payload.id, sub_ids.receive_id);
        let item = HostChatActionSubscribeItem::decode(&mut &message.payload.value[..])
            .expect("decode chat action item");
        assert_eq!(
            item,
            HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
                room_id: "room-1".to_string(),
                peer: "peer-1".to_string(),
                payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
                    text: "hello".to_string(),
                }),
            })
        );
    }

    #[test]
    fn chat_action_subscribe_receives_core_notified_custom_message() {
        let core = make_core();
        let (tx, rx) = std::sync::mpsc::channel();
        let transport = Arc::new(ChannelTransport { tx });
        let sub_ids = subscription_ids("chat_action_subscribe").expect("known subscription");
        let start = ProtocolMessage {
            request_id: "chat:1".into(),
            payload: Payload {
                id: sub_ids.start_id,
                value: Vec::new(),
            },
        };

        futures::executor::block_on(core.dispatch(start, transport));
        core.notify_chat_custom_message_posted(
            "room-1".to_string(),
            "peer-1".to_string(),
            "product.card".to_string(),
            vec![1, 2, 3],
        );

        let message = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("subscription receive frame");
        assert_eq!(message.request_id, "chat:1");
        assert_eq!(message.payload.id, sub_ids.receive_id);
        let item = HostChatActionSubscribeItem::decode(&mut &message.payload.value[..])
            .expect("decode chat action item");
        assert_eq!(
            item,
            HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
                room_id: "room-1".to_string(),
                peer: "peer-1".to_string(),
                payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Custom(
                    v01::ChatCustomMessage {
                        message_type: "product.card".to_string(),
                        payload: vec![1, 2, 3],
                    }
                )),
            })
        );
    }

    #[test]
    fn chat_create_room_round_trips_through_platform() {
        let core = make_core();
        let request = HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
            room_id: "room-1".to_string(),
            name: "Room 1".to_string(),
            icon: "https://example.com/icon.png".to_string(),
        });
        let payload = run_request(&core, "chat_create_room", request.encode());

        assert_eq!(payload[0], 0x00);
        let response = HostChatCreateRoomResponse::decode(&mut &payload[1..])
            .expect("decode chat create room response");
        assert_eq!(
            response,
            HostChatCreateRoomResponse::V1(v01::HostChatCreateRoomResponse {
                status: v01::ChatRoomRegistrationStatus::New,
            })
        );
    }

    #[test]
    fn chat_post_message_round_trips_through_platform() {
        let core = make_core();
        let request = HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
            room_id: "room-1".to_string(),
            payload: v01::ChatMessageContent::Text {
                text: "hello".to_string(),
            },
        });
        let payload = run_request(&core, "chat_post_message", request.encode());

        assert_eq!(payload[0], 0x00);
        let response = HostChatPostMessageResponse::decode(&mut &payload[1..])
            .expect("decode chat post response");
        assert_eq!(
            response,
            HostChatPostMessageResponse::V1(v01::HostChatPostMessageResponse {
                message_id: "message-1".to_string(),
            })
        );
    }

    #[test]
    fn payment_request_round_trips_through_platform() {
        let core = make_core();
        let request = HostPaymentRequest::V1(v01::HostPaymentRequest {
            from: None,
            amount: 123,
            destination: [7u8; 32],
        });
        let payload = run_request(&core, "payment_request", request.encode());

        assert_eq!(payload[0], 0x00);
        let response = HostPaymentResponse::decode(&mut &payload[1..])
            .expect("decode payment request response");
        assert_eq!(
            response,
            HostPaymentResponse::V1(v01::HostPaymentResponse {
                id: "payment-1".to_string(),
            })
        );
    }

    #[test]
    fn payment_balance_subscribe_round_trips_through_platform() {
        let platform = Arc::new(StubPlatform::default());
        *platform
            .payment_balances
            .lock()
            .expect("payment balances mutex poisoned") = vec![123];
        let core = TrUApiCore::from_platform_with_config(
            platform,
            runtime_config("dotli.dot"),
            test_spawner(),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let transport = Arc::new(ChannelTransport { tx });
        let sub_ids = subscription_ids("payment_balance_subscribe").expect("known subscription");
        let request =
            HostPaymentBalanceSubscribeRequest::V1(v01::HostPaymentBalanceSubscribeRequest {
                purse: None,
            });
        let start = ProtocolMessage {
            request_id: "payment-balance:1".into(),
            payload: Payload {
                id: sub_ids.start_id,
                value: request.encode(),
            },
        };

        futures::executor::block_on(core.dispatch(start, transport));

        let message = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("subscription receive frame");
        assert_eq!(message.request_id, "payment-balance:1");
        assert_eq!(message.payload.id, sub_ids.receive_id);
        let item = HostPaymentBalanceSubscribeItem::decode(&mut &message.payload.value[..])
            .expect("decode payment balance item");
        assert_eq!(
            item,
            HostPaymentBalanceSubscribeItem::V1(v01::HostPaymentBalanceSubscribeItem {
                available: 123,
            })
        );
    }

    #[test]
    fn payment_status_subscribe_round_trips_through_platform() {
        let platform = Arc::new(StubPlatform::default());
        *platform
            .payment_statuses
            .lock()
            .expect("payment statuses mutex poisoned") =
            vec![v01::HostPaymentStatusSubscribeItem::Processing];
        let core = TrUApiCore::from_platform_with_config(
            platform,
            runtime_config("dotli.dot"),
            test_spawner(),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let transport = Arc::new(ChannelTransport { tx });
        let sub_ids = subscription_ids("payment_status_subscribe").expect("known subscription");
        let request =
            HostPaymentStatusSubscribeRequest::V1(v01::HostPaymentStatusSubscribeRequest {
                payment_id: "payment-1".to_string(),
            });
        let start = ProtocolMessage {
            request_id: "payment-status:1".into(),
            payload: Payload {
                id: sub_ids.start_id,
                value: request.encode(),
            },
        };

        futures::executor::block_on(core.dispatch(start, transport));

        let message = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("subscription receive frame");
        assert_eq!(message.request_id, "payment-status:1");
        assert_eq!(message.payload.id, sub_ids.receive_id);
        let item = HostPaymentStatusSubscribeItem::decode(&mut &message.payload.value[..])
            .expect("decode payment status item");
        assert_eq!(
            item,
            HostPaymentStatusSubscribeItem::V1(v01::HostPaymentStatusSubscribeItem::Processing)
        );
    }

    #[test]
    fn payment_top_up_round_trips_through_platform() {
        let core = make_core();
        let request = HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
            into: None,
            amount: 123,
            source: v01::PaymentTopUpSource::ProductAccount {
                derivation_index: 0,
            },
        });
        let payload = run_request(&core, "payment_top_up", request.encode());

        assert_eq!(payload[0], 0x00);
        let response = HostPaymentTopUpResponse::decode(&mut &payload[1..])
            .expect("decode payment top-up response");
        assert_eq!(response, HostPaymentTopUpResponse::V1);
    }

    #[test]
    fn local_storage_read_round_trips_none() {
        let core = make_core();
        let request = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
            key: "missing".into(),
        });
        let payload = run_request(&core, "local_storage_read", request.encode());
        // Ok disc 0x00, V1 variant 0x00, Option::None = 0x00.
        assert_eq!(payload, vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn local_storage_write_round_trips_unit_ok() {
        let core = make_core();
        let request = HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest {
            key: "k".into(),
            value: vec![1, 2, 3],
        });
        let payload = run_request(&core, "local_storage_write", request.encode());
        // Ok disc 0x00, V1 variant 0x00.
        assert_eq!(payload, vec![0x00, 0x00]);
    }

    #[test]
    fn local_storage_clear_round_trips_unit_ok() {
        let core = make_core();
        let request =
            HostLocalStorageClearRequest::V1(v01::HostLocalStorageClearRequest { key: "k".into() });
        let payload = run_request(&core, "local_storage_clear", request.encode());
        // Ok disc 0x00, V1 variant 0x00.
        assert_eq!(payload, vec![0x00, 0x00]);
    }

    #[test]
    fn send_push_notification_delegates_to_platform() {
        let core = make_core();
        let request = HostPushNotificationRequest::V1(v01::HostPushNotificationRequest {
            text: "hi".into(),
            deeplink: None,
            scheduled_at: None,
        });
        let payload = run_request(
            &core,
            "notifications_send_push_notification",
            request.encode(),
        );
        // Ok disc 0x00, V1 variant 0x00, notification id 0.
        let mut expected = vec![0x00u8];
        truapi::versioned::notifications::HostPushNotificationResponse::V1(
            v01::HostPushNotificationResponse { id: 0 },
        )
        .encode_to(&mut expected);
        assert_eq!(payload, expected);
    }

    #[test]
    fn request_remote_permission_round_trips_granted() {
        let core = make_core();
        let request = RemotePermissionRequest::V1(v01::RemotePermissionRequest {
            permission: v01::RemotePermission::ChainSubmit,
        });
        let payload = run_request(
            &core,
            "permissions_request_remote_permission",
            request.encode(),
        );
        // Stub permissions grants every request. Wire is Ok disc 0x00, V1
        // variant 0x00, granted=1.
        assert_eq!(payload, vec![0x00, 0x00, 0x01]);
    }

    /// `connection_status_subscribe` produces a stream whose first item is
    /// the current session state. Drive it through the dispatcher with a
    /// recording transport and assert exactly one `_receive` frame appears.
    #[test]
    fn connection_status_subscribe_yields_initial_disconnected() {
        use std::sync::Mutex;

        #[derive(Default)]
        struct RecordingTransport {
            sent: Mutex<Vec<ProtocolMessage>>,
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

        let core = make_core();
        let transport = Arc::new(RecordingTransport::default());
        let dyn_transport: Arc<dyn Transport> = transport.clone();

        let sub_ids =
            subscription_ids("account_connection_status_subscribe").expect("known subscription");
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: sub_ids.start_id,
                value: Vec::new(),
            },
        };
        futures::executor::block_on(core.dispatch(frame, dyn_transport));

        // Wait briefly for the spawned thread to emit the initial item.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if !transport.sent.lock().unwrap().is_empty() {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("subscription did not yield an item in time");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let sent = transport.sent.lock().unwrap().clone();
        assert!(!sent.is_empty(), "expected at least one _receive frame");
        let first = &sent[0];
        assert_eq!(first.payload.id, sub_ids.receive_id);
        // V1(Disconnected): V1 variant 0x00, Disconnected discriminant 0x00.
        assert_eq!(first.payload.value, vec![0x00, 0x00]);
    }
}
