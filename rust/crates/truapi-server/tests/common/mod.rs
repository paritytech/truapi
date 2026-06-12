#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

use futures::stream::{self, BoxStream};
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{
    AuthPresenter, ChainProvider, ChatHost, Features, JsonRpcConnection, Navigation, Notifications,
    PairingDeeplinkScheme, PaymentHost, Permissions, PreimageHost, RuntimeConfig, SessionStore,
    Storage, ThemeHost, UserConfirmation,
};

pub fn test_spawner() -> truapi_server::subscription::Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    {
        truapi_server::subscription::thread_per_subscription_spawner()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(futures::executor::block_on)
    }
}

pub fn test_runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        product_label: "dotli".to_string(),
        product_id: "dotli.dot".to_string(),
        site_id: "dot.li".to_string(),
        host_name: "Polkadot Web".to_string(),
        host_icon: Some("https://dot.li/dotli.png".to_string()),
        host_version: None,
        platform_type: None,
        platform_version: None,
        people_chain_genesis_hash: [0xa2; 32],
        pairing_deeplink_scheme: PairingDeeplinkScheme::PolkadotApp,
    }
}

pub struct WireShapePlatform;

impl Storage for WireShapePlatform {
    async fn read(&self, _key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
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

impl Navigation for WireShapePlatform {
    async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
        Ok(())
    }
}

impl Notifications for WireShapePlatform {
    async fn push_notification(
        &self,
        _notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        Ok(v01::HostPushNotificationResponse { id: 0 })
    }

    async fn cancel_notification(&self, _id: u32) -> Result<(), v01::GenericError> {
        Ok(())
    }
}

impl Permissions for WireShapePlatform {
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

impl Features for WireShapePlatform {
    async fn feature_supported(
        &self,
        _request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
        Ok(HostFeatureSupportedResponse::V1(
            v01::HostFeatureSupportedResponse { supported: true },
        ))
    }
}

impl ChatHost for WireShapePlatform {
    async fn create_chat_room(
        &self,
        _room_id: String,
        _name: String,
        _icon: String,
    ) -> Result<v01::ChatRoomRegistrationStatus, v01::HostChatCreateRoomError> {
        Ok(v01::ChatRoomRegistrationStatus::New)
    }

    async fn post_chat_message(
        &self,
        _room_id: String,
        _payload: v01::ChatMessageContent,
    ) -> Result<String, v01::HostChatPostMessageError> {
        Ok("message-1".to_string())
    }
}

impl PaymentHost for WireShapePlatform {
    async fn subscribe_payment_balance(
        &self,
    ) -> Result<BoxStream<'static, v01::Balance>, v01::HostPaymentBalanceSubscribeError> {
        Ok(Box::pin(stream::iter([0])))
    }

    async fn request_payment(
        &self,
        _amount: v01::Balance,
        _destination: [u8; 32],
    ) -> Result<String, v01::HostPaymentError> {
        Ok("payment-1".to_string())
    }

    async fn top_up_payment(
        &self,
        _amount: v01::Balance,
        _source: v01::PaymentTopUpSource,
    ) -> Result<(), v01::HostPaymentTopUpError> {
        Ok(())
    }

    async fn subscribe_payment_status(
        &self,
        _payment_id: String,
    ) -> Result<
        BoxStream<'static, v01::HostPaymentStatusSubscribeItem>,
        v01::HostPaymentStatusSubscribeError,
    > {
        Ok(Box::pin(stream::iter([
            v01::HostPaymentStatusSubscribeItem::Completed,
        ])))
    }
}

struct DeadConnection;

impl JsonRpcConnection for DeadConnection {
    fn send(&self, _request: String) {}
    fn responses(&self) -> BoxStream<'static, String> {
        Box::pin(stream::empty())
    }
}

impl ChainProvider for WireShapePlatform {
    async fn connect(
        &self,
        _genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        Ok(Box::new(DeadConnection))
    }
}

impl AuthPresenter for WireShapePlatform {}

impl SessionStore for WireShapePlatform {
    async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(None)
    }
    async fn write_session(&self, _value: Vec<u8>) -> Result<(), v01::GenericError> {
        Ok(())
    }
    async fn clear_session(&self) -> Result<(), v01::GenericError> {
        Ok(())
    }
    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        Box::pin(stream::once(async { Ok(()) }))
    }
}

impl UserConfirmation for WireShapePlatform {
    async fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
    async fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
    async fn confirm_create_transaction(
        &self,
        _review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
    async fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
    async fn confirm_resource_allocation(
        &self,
        _review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
}

impl ThemeHost for WireShapePlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

impl PreimageHost for WireShapePlatform {
    async fn confirm_preimage_submit(&self, _size: u64) -> Result<(), v01::PreimageSubmitError> {
        Ok(())
    }
    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        Ok(value)
    }
    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}
