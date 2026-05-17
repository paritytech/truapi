//! `TrUApiCore`: the entrypoint a host wraps around a `truapi::api::TrUApi`
//! implementation (direct path) or a `truapi_platform::Platform`
//! implementation (platform path).
//!
//! Direct path: `TrUApiCore::new(host)` accepts anything implementing
//! the unified [`truapi::api::TrUApi`] super-trait. Useful for unit tests
//! and bespoke hosts.
//!
//! Platform path: [`TrUApiCore::from_platform`] takes a
//! [`truapi_platform::Platform`] and wires it through
//! [`crate::runtime::PlatformRuntimeHost`] before registering with the
//! generated dispatcher. This is the path real platform shims (UniFFI,
//! wasm-bindgen, ws-bridge, ...) take.

use std::sync::{Arc, Mutex};

use parity_scale_codec::{Decode, Encode};
use truapi::api::TrUApi;
use truapi_platform::Platform;

use crate::generated::dispatcher;
use crate::host_logic::session::SessionState;
use crate::runtime::PlatformRuntimeHost;
use crate::{Dispatcher, ProtocolMessage, Transport};

/// Top-level core. Owns the dispatcher and, on the platform path, the shared
/// session-state holder.
pub struct TrUApiCore {
    dispatcher: Dispatcher,
    /// Always present; empty for [`Self::new`] (no platform feeding it),
    /// connected to a [`PlatformRuntimeHost`] for [`Self::from_platform`].
    session_state: Arc<SessionState>,
}

impl TrUApiCore {
    /// Build a core around a direct `TrUApi` implementation. The session
    /// state holder is unused on this path (no platform pushes updates),
    /// but is created anyway so the public API surface stays consistent.
    pub fn new<P>(host: Arc<P>) -> Self
    where
        P: TrUApi + 'static,
    {
        let mut dispatcher = Dispatcher::new();
        dispatcher::register(&mut dispatcher, host);
        Self {
            dispatcher,
            session_state: SessionState::new(),
        }
    }

    /// Build a core around a [`Platform`] implementation. Wraps the platform
    /// in a [`PlatformRuntimeHost`] before registering with the dispatcher.
    pub fn from_platform<P>(platform: Arc<P>) -> Self
    where
        P: Platform + 'static,
    {
        let runtime = Arc::new(PlatformRuntimeHost::new(platform));
        let session_state = runtime.session_state();
        let mut dispatcher = Dispatcher::new();
        dispatcher::register(&mut dispatcher, runtime);
        Self {
            dispatcher,
            session_state,
        }
    }

    /// Handle to the shared session-state holder. Platform bridges push
    /// `setActiveSession` / `clearActiveSession` notifications through this.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Asynchronous form of [`Self::receive_from_product`]. Decodes the
    /// incoming frame, runs it through the dispatcher, and returns the
    /// SCALE-encoded response (if any).
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
    pub fn receive_from_product(&self, frame: &[u8]) -> Option<Vec<u8>> {
        futures::executor::block_on(self.receive_from_product_async(frame))
    }

    /// Dispatch an already-decoded protocol message through the underlying
    /// dispatcher. Bridges that own a long-lived transport (e.g. WebSocket,
    /// JS callback) call this directly so subscription items flow back
    /// through the bridge transport instead of the single-slot capture used
    /// by [`Self::receive_from_product`].
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
    fn from_platform_dispatches_feature_supported() {
        let core = TrUApiCore::from_platform(Arc::new(StubPlatform));
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                tag: compose_action("system_feature_supported", FrameKind::Request),
                value: request.encode(),
            },
        };
        let encoded = frame.encode();
        let response_bytes = core
            .receive_from_product(&encoded)
            .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(
            response.payload.tag,
            compose_action("system_feature_supported", FrameKind::Response),
        );
        // Wire payload is `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);
    }
}
