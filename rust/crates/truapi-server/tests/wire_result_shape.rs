//! Result-wire-shape regression test.
//!
//! The TS host/client codec expects every request response to be
//! `Result<Ok, Err>`-shaped on the wire (one leading discriminant byte
//! followed by the SCALE-encoded value). This test stands up a
//! `TrUApiCore::from_platform` with a `StubPlatform` whose `Features`
//! impl returns `Ok(supported = true)` and asserts:
//!
//! - A `system_feature_supported_request` produces a response whose
//!   payload begins with `0x00` (Ok), followed by the encoded
//!   `HostFeatureSupportedResponse::V1(true)`.
//! - A `local_storage_read_request` whose stub returns
//!   `Err(HostLocalStorageReadError::Full)` produces a response whose
//!   payload begins with `0x01` (Err), followed by the encoded
//!   `CallError::Domain(Full)`.
//!
//! Both halves prove the wire layout stays in lockstep with the TS
//! `S.Result(ok, err)` codec.

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use parity_scale_codec::{Decode, Encode};

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
    Accounts, ChainProvider, Features, GenesisHash, JsonRpcConnection, Navigation, Notifications,
    Permissions, Preimage, Signing, StatementStore, Storage,
};

use truapi_server::{FrameKind, Payload, ProtocolMessage, TrUApiCore, compose_action};

struct StubPlatform;

#[async_trait]
impl Storage for StubPlatform {
    async fn read(&self, _key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        // Drive the error-path test: return `Full` so we can assert the
        // wire-Err discriminant precedes the SCALE-encoded `CallError::Domain(Full)`.
        Err(v01::HostLocalStorageReadError::Full)
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
        _request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
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
impl Accounts for StubPlatform {
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
impl Signing for StubPlatform {
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
impl StatementStore for StubPlatform {
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
    ) -> Result<RemoteStatementStoreCreateProofResponse, v01::RemoteStatementStoreCreateProofError>
    {
        Err(v01::RemoteStatementStoreCreateProofError::UnableToSign)
    }
}

#[async_trait]
impl Preimage for StubPlatform {
    async fn remote_preimage_lookup_subscribe(
        &self,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> BoxStream<'static, RemotePreimageLookupSubscribeItem> {
        Box::pin(stream::empty())
    }
}

fn dispatch(core: &TrUApiCore, frame: ProtocolMessage) -> ProtocolMessage {
    let encoded = frame.encode();
    let response_bytes = core
        .receive_from_product(&encoded)
        .expect("dispatcher emitted a response frame");
    ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response")
}

#[test]
fn feature_supported_ok_response_uses_ok_discriminant() {
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
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:1");
    assert_eq!(
        response.payload.tag,
        compose_action("system_feature_supported", FrameKind::Response),
    );

    // Wire payload: [Ok disc=0x00][encoded versioned response]
    let mut expected = vec![0x00u8];
    HostFeatureSupportedResponse::V1(v01::HostFeatureSupportedResponse { supported: true })
        .encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
    // The Result-disc byte is unambiguously 0x00 for Ok.
    assert_eq!(response.payload.value.first(), Some(&0x00));
}

#[test]
fn local_storage_read_err_response_uses_err_discriminant() {
    let core = TrUApiCore::from_platform(Arc::new(StubPlatform));
    let request = truapi::versioned::local_storage::HostLocalStorageReadRequest::V1(
        v01::HostLocalStorageReadRequest {
            key: "missing".to_string(),
        },
    );
    let frame = ProtocolMessage {
        request_id: "p:2".into(),
        payload: Payload {
            tag: compose_action("local_storage_read", FrameKind::Request),
            value: request.encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:2");
    assert_eq!(
        response.payload.tag,
        compose_action("local_storage_read", FrameKind::Response),
    );

    // Wire payload: `[Err disc=0x01][SCALE-encoded CallError]`. The stub
    // returns `HostLocalStorageReadError::Full`; the runtime wraps it in
    // `CallError::Domain(HostLocalStorageReadError::V1(Full))`, encoded as:
    //   [0x01 Err disc]
    //   [0x00 CallError::Domain variant]
    //   [0x00 HostLocalStorageReadError::V1 variant]
    //   [0x00 v01::HostLocalStorageReadError::Full variant]
    assert_eq!(response.payload.value, vec![0x01, 0x00, 0x00, 0x00]);
    assert_eq!(response.payload.value.first(), Some(&0x01));
}
