#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

use std::sync::Mutex;

use futures::stream::{self, BoxStream};
use truapi::v01;
use truapi_platform::{
    AuthPresenter, BulletinAllowanceKey, ChainProvider, CoreStorage, CoreStorageKey, Features,
    HostInfo, JsonRpcConnection, Navigation, Notifications, PairingHostConfig, Permissions,
    PlatformInfo, PreimageHost, ProductContext, ProductStorage, ThemeHost, UserConfirmation,
    UserConfirmationReview,
};
use truapi_server::frame::ProtocolMessage;
use truapi_server::transport::Transport;

/// Transport stub that records every frame sent through it, for asserting
/// what the core emits during a dispatch.
#[derive(Default)]
pub struct RecordingTransport {
    /// Frames captured in send order.
    pub sent: Mutex<Vec<ProtocolMessage>>,
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

/// Test spawner that matches the current target.
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

/// Runtime configuration shared by integration tests.
pub fn test_runtime_config() -> (PairingHostConfig, ProductContext) {
    (
        PairingHostConfig::new(
            HostInfo {
                name: "Polkadot Web".to_string(),
                icon: Some("https://dot.li/dotli.png".to_string()),
                version: None,
            },
            PlatformInfo::default(),
            [0xa2; 32],
            "polkadotapp".to_string(),
        )
        .expect("test host runtime config is valid"),
        ProductContext::new("dotli.dot".to_string()).expect("test product context is valid"),
    )
}

pub struct WireShapePlatform;

#[truapi_platform::async_trait]
impl ProductStorage for WireShapePlatform {
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

#[truapi_platform::async_trait]
impl Navigation for WireShapePlatform {
    async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
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

#[truapi_platform::async_trait]
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

#[truapi_platform::async_trait]
impl Features for WireShapePlatform {
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        Ok(v01::HostFeatureSupportedResponse { supported: true })
    }
}

struct DeadConnection;

impl JsonRpcConnection for DeadConnection {
    fn send(&self, _request: String) {}
    fn responses(&self) -> BoxStream<'static, String> {
        Box::pin(stream::empty())
    }
    fn close(&self) {}
}

#[truapi_platform::async_trait]
impl ChainProvider for WireShapePlatform {
    async fn connect(
        &self,
        _genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        Ok(Box::new(DeadConnection))
    }
}

impl AuthPresenter for WireShapePlatform {}

#[truapi_platform::async_trait]
impl CoreStorage for WireShapePlatform {
    async fn read_core_storage(
        &self,
        _key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(None)
    }
    async fn write_core_storage(
        &self,
        _key: CoreStorageKey,
        _value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        Ok(())
    }
    async fn clear_core_storage(&self, _key: CoreStorageKey) -> Result<(), v01::GenericError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl UserConfirmation for WireShapePlatform {
    async fn confirm_user_action(
        &self,
        _review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
}

impl ThemeHost for WireShapePlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

#[truapi_platform::async_trait]
impl PreimageHost for WireShapePlatform {
    async fn submit_preimage(
        &self,
        value: Vec<u8>,
        _bulletin_allowance_key: BulletinAllowanceKey,
    ) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        Ok(value)
    }
    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}
