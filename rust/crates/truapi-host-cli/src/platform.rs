//! `Platform` implementation for the headless hosts.
//!
//! In-memory product and core storage, a WebSocket chain provider pointed at
//! the real People-chain statement store, and an auto-approving
//! [`UserConfirmation`]. Auth-state transitions are published on a channel so
//! the CLI can print the pairing deeplink and observe connection status.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use blake2_rfc::blake2b::blake2b;
use futures::stream::{self, BoxStream};
use truapi::v01;
use truapi_platform::{
    AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features, JsonRpcConnection, Navigation,
    Notifications, Permissions, PreimageHost, ProductStorage, ThemeHost, UserConfirmation,
    UserConfirmationReview,
};

use crate::chain::WsChainProvider;

/// How the host answers [`UserConfirmation`] prompts.
#[derive(Clone, Copy)]
pub enum ApprovalPolicy {
    /// Approve every sensitive action (default for e2e).
    Always,
    /// Reject every sensitive action (for negative tests).
    Never,
}

impl ApprovalPolicy {
    fn approves(self) -> bool {
        matches!(self, ApprovalPolicy::Always)
    }
}

/// Headless-host platform shared by both roles.
pub struct CliPlatform {
    chain: WsChainProvider,
    product_storage: Mutex<HashMap<String, Vec<u8>>>,
    core_storage: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    preimages: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    approval: ApprovalPolicy,
}

impl CliPlatform {
    /// Build a platform whose chain provider connects to `statement_store_url`.
    pub fn new(statement_store_url: impl Into<String>, approval: ApprovalPolicy) -> Arc<Self> {
        Arc::new(Self {
            chain: WsChainProvider::new(statement_store_url),
            product_storage: Mutex::new(HashMap::new()),
            core_storage: Mutex::new(HashMap::new()),
            preimages: Mutex::new(HashMap::new()),
            approval,
        })
    }

    fn core_key(key: &CoreStorageKey) -> Vec<u8> {
        use parity_scale_codec::Encode;
        key.encode()
    }
}

#[async_trait]
impl ProductStorage for CliPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        Ok(self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned")
            .get(&key)
            .cloned())
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        self.product_storage
            .lock()
            .expect("product storage mutex poisoned")
            .insert(key, value);
        Ok(())
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        self.product_storage
            .lock()
            .expect("product storage mutex poisoned")
            .remove(&key);
        Ok(())
    }
}

#[async_trait]
impl CoreStorage for CliPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(self
            .core_storage
            .lock()
            .expect("core storage mutex poisoned")
            .get(&Self::core_key(&key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.core_storage
            .lock()
            .expect("core storage mutex poisoned")
            .insert(Self::core_key(&key), value);
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        self.core_storage
            .lock()
            .expect("core storage mutex poisoned")
            .remove(&Self::core_key(&key));
        Ok(())
    }
}

#[async_trait]
impl ChainProvider for CliPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        self.chain.connect(genesis_hash).await
    }
}

#[async_trait]
impl Navigation for CliPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        tracing::info!(%url, "navigate_to");
        Ok(())
    }
}

#[async_trait]
impl Notifications for CliPlatform {
    async fn push_notification(
        &self,
        _notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        Ok(v01::HostPushNotificationResponse { id: 1 })
    }
}

#[async_trait]
impl Permissions for CliPlatform {
    async fn device_permission(
        &self,
        _request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        Ok(v01::HostDevicePermissionResponse {
            granted: self.approval.approves(),
        })
    }

    async fn remote_permission(
        &self,
        _request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        Ok(v01::RemotePermissionResponse {
            granted: self.approval.approves(),
        })
    }
}

#[async_trait]
impl Features for CliPlatform {
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        Ok(v01::HostFeatureSupportedResponse { supported: true })
    }
}

impl truapi_platform::AuthPresenter for CliPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        // Machine-readable lines for orchestrators to observe pairing.
        match &state {
            AuthState::Pairing { deeplink } => println!("PAIRING_DEEPLINK {deeplink}"),
            AuthState::Connected(_) => println!("PAIRING_CONNECTED"),
            AuthState::Disconnected => println!("PAIRING_DISCONNECTED"),
            AuthState::LoginFailed { reason } => println!("PAIRING_FAILED {reason}"),
        }
    }
}

#[async_trait]
impl UserConfirmation for CliPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        tracing::debug!(
            ?review,
            approved = self.approval.approves(),
            "confirm_user_action"
        );
        Ok(self.approval.approves())
    }
}

impl ThemeHost for CliPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        Box::pin(stream::once(async { Ok(v01::ThemeVariant::Dark) }))
    }
}

#[async_trait]
impl PreimageHost for CliPlatform {
    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        let key = blake2b(32, &[], &value).as_bytes().to_vec();
        self.preimages
            .lock()
            .expect("preimage mutex poisoned")
            .insert(key.clone(), value);
        Ok(key)
    }

    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let value = self
            .preimages
            .lock()
            .expect("preimage mutex poisoned")
            .get(&key)
            .cloned();
        Box::pin(stream::once(async move { Ok(value) }))
    }
}
