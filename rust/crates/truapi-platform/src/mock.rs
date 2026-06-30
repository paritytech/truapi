//! In-memory mock [`Platform`](crate::Platform) for tests and host simulators.
//!
//! `MockPlatform` implements every capability trait with deterministic,
//! configurable behavior and no OS, device, or network dependency: storage is
//! an in-memory map, permission prompts answer from a fixed per-capability
//! policy (no UI), navigation and notifications are recorded, and chain access
//! returns a configurable connection. Because the protocol logic lives in
//! `truapi-server`, a `MockPlatform` wired into the core yields a faithful host
//! whose only mocked surface is the OS-primitive seam.
//!
//! Behavior is a [`MockConfig`] read on every call: per-capability permission
//! policy, feature support, theme, confirmation answer, [`ChainBehavior`], and
//! [`MockFaults`] error injection. Recordings (`navigations`,
//! `pushed_notifications`, `confirmations`, `auth_states`, `sent_rpc`, …) are
//! the test oracles.
//!
//! Signing and login require a paired wallet answering over the statement-store
//! channel. With [`ChainBehavior::Silent`] the chain connection records
//! outbound requests and never answers, so those flows park; use
//! [`ChainBehavior::Scripted`] to feed canned response frames.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::stream::{self, BoxStream};

use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};

use crate::{
    AuthPresenter, AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features,
    JsonRpcConnection, Navigation, Notifications, Permissions, PreimageHost, ProductStorage,
    ThemeHost, UserConfirmation, UserConfirmationReview,
};

/// How the mock answers a permission prompt for one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionPolicy {
    /// Grant without prompting.
    #[default]
    AllowAll,
    /// Deny.
    DenyAll,
}

impl PermissionPolicy {
    fn granted(self) -> bool {
        matches!(self, PermissionPolicy::AllowAll)
    }
}

/// The kind of action the core asked the host to confirm. Recorded on every
/// `confirm_user_action` so tests can assert what the core tried to do
/// (e.g. that a sign-payload review fired) even when the chain parks afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    /// [`UserConfirmationReview::SignPayload`].
    SignPayload,
    /// [`UserConfirmationReview::SignRaw`].
    SignRaw,
    /// [`UserConfirmationReview::CreateTransaction`].
    CreateTransaction,
    /// [`UserConfirmationReview::AccountAlias`].
    AccountAlias,
    /// [`UserConfirmationReview::ResourceAllocation`].
    ResourceAllocation,
    /// [`UserConfirmationReview::PreimageSubmit`].
    PreimageSubmit,
}

impl ConfirmKind {
    fn of(review: &UserConfirmationReview) -> Self {
        match review {
            UserConfirmationReview::SignPayload(_) => ConfirmKind::SignPayload,
            UserConfirmationReview::SignRaw(_) => ConfirmKind::SignRaw,
            UserConfirmationReview::CreateTransaction(_) => ConfirmKind::CreateTransaction,
            UserConfirmationReview::AccountAlias(_) => ConfirmKind::AccountAlias,
            UserConfirmationReview::ResourceAllocation(_) => ConfirmKind::ResourceAllocation,
            UserConfirmationReview::PreimageSubmit(_) => ConfirmKind::PreimageSubmit,
        }
    }
}

/// How the mock's chain connection behaves.
#[derive(Debug, Clone, Default)]
pub enum ChainBehavior {
    /// Record outbound requests, never answer. Chain-dependent flows (login,
    /// signing, statement store) park rather than complete, so drive any test
    /// that reaches them under a timeout; use [`ChainBehavior::Closed`] to make
    /// a disconnect observable instead.
    #[default]
    Silent,
    /// Record outbound requests and replay these response frames in order,
    /// then end the stream.
    Scripted(Vec<String>),
    /// Record outbound requests; the response stream ends immediately, so
    /// disconnect/timeout paths can be asserted (fail-fast) rather than parked.
    Closed,
    /// `connect` fails with this reason.
    ConnectError(String),
}

/// Optional error injection. When a field is `Some`, the matching host call
/// returns that error instead of succeeding, exercising the core's
/// error-handling paths.
#[derive(Debug, Clone, Default)]
pub struct MockFaults {
    /// Product and core storage reads/writes/clears fail with this reason.
    pub storage_error: Option<String>,
    /// `navigate_to` fails with this reason.
    pub navigate_error: Option<String>,
    /// `push_notification` fails with this reason.
    pub notification_error: Option<String>,
    /// `submit_preimage` fails with this reason.
    pub preimage_submit_error: Option<String>,
}

/// Behavior knobs for [`MockPlatform`], read on every call.
#[derive(Debug, Clone)]
pub struct MockConfig {
    /// Answer for `device_permission`.
    pub device_permissions: PermissionPolicy,
    /// Answer for `remote_permission`.
    pub remote_permissions: PermissionPolicy,
    /// Whether `feature_supported` reports support.
    pub feature_supported: bool,
    /// Theme emitted by `subscribe_theme`.
    pub theme: v01::ThemeVariant,
    /// Whether `confirm_user_action` confirms reviewed actions.
    pub confirm_user_actions: bool,
    /// Chain connection behavior.
    pub chain: ChainBehavior,
    /// Error injection.
    pub faults: MockFaults,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            device_permissions: PermissionPolicy::AllowAll,
            remote_permissions: PermissionPolicy::AllowAll,
            feature_supported: true,
            theme: v01::ThemeVariant::Dark,
            confirm_user_actions: true,
            chain: ChainBehavior::Silent,
            faults: MockFaults::default(),
        }
    }
}

/// In-memory mock host platform. Cheap to `clone`; clones share recordings and
/// storage (state is `Arc`ed), so a recording made through one clone is visible
/// through another.
#[derive(Clone)]
pub struct MockPlatform {
    config: Arc<MockConfig>,
    storage: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    preimages: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
    navigations: Arc<Mutex<Vec<String>>>,
    notifications: Arc<Mutex<Vec<v01::HostPushNotificationRequest>>>,
    cancelled_notifications: Arc<Mutex<Vec<v01::NotificationId>>>,
    confirmations: Arc<Mutex<Vec<ConfirmKind>>>,
    auth_states: Arc<Mutex<Vec<AuthState>>>,
    sent_rpc: Arc<Mutex<Vec<String>>>,
    next_notification_id: Arc<AtomicU32>,
}

impl Default for MockPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl MockPlatform {
    /// Build a mock platform with default behavior (allow-all permissions,
    /// feature support on, dark theme, auto-confirm, silent chain, no faults).
    pub fn new() -> Self {
        Self::with_config(MockConfig::default())
    }

    /// Build a mock platform with explicit behavior.
    pub fn with_config(config: MockConfig) -> Self {
        Self {
            config: Arc::new(config),
            storage: Arc::new(Mutex::new(HashMap::new())),
            preimages: Arc::new(Mutex::new(HashMap::new())),
            navigations: Arc::new(Mutex::new(Vec::new())),
            notifications: Arc::new(Mutex::new(Vec::new())),
            cancelled_notifications: Arc::new(Mutex::new(Vec::new())),
            confirmations: Arc::new(Mutex::new(Vec::new())),
            auth_states: Arc::new(Mutex::new(Vec::new())),
            sent_rpc: Arc::new(Mutex::new(Vec::new())),
            next_notification_id: Arc::new(AtomicU32::new(0)),
        }
    }

    /// URLs the core asked the host to open, in order.
    pub fn navigations(&self) -> Vec<String> {
        self.navigations
            .lock()
            .expect("navigations poisoned")
            .clone()
    }

    /// Notifications the core asked the host to show, in order.
    pub fn pushed_notifications(&self) -> Vec<v01::HostPushNotificationRequest> {
        self.notifications
            .lock()
            .expect("notifications poisoned")
            .clone()
    }

    /// Notification ids the core asked the host to cancel, in order.
    pub fn cancelled_notifications(&self) -> Vec<v01::NotificationId> {
        self.cancelled_notifications
            .lock()
            .expect("cancellations poisoned")
            .clone()
    }

    /// Confirmation kinds the core requested, in order.
    pub fn confirmations(&self) -> Vec<ConfirmKind> {
        self.confirmations
            .lock()
            .expect("confirmations poisoned")
            .clone()
    }

    /// Auth state transitions the core emitted, in order.
    pub fn auth_states(&self) -> Vec<AuthState> {
        self.auth_states
            .lock()
            .expect("auth states poisoned")
            .clone()
    }

    /// Raw JSON-RPC requests the core sent over the chain connection.
    pub fn sent_rpc(&self) -> Vec<String> {
        self.sent_rpc.lock().expect("sent rpc poisoned").clone()
    }
}

/// Product keys are namespaced from core slots so neither can shadow the other.
fn product_key(key: &str) -> String {
    format!("product:{key}")
}

/// Stable string key for a typed core-storage slot.
fn core_key(key: &CoreStorageKey) -> String {
    match key {
        CoreStorageKey::AuthSession => "core:auth-session".to_string(),
        CoreStorageKey::PairingDeviceIdentity => "core:pairing-device-identity".to_string(),
        CoreStorageKey::PermissionAuthorization { storage_key } => {
            format!("core:permission:{storage_key}")
        }
    }
}

/// Deterministic short key for a preimage value, so `submit` then `lookup`
/// round-trips without storing the full value as its own key.
fn preimage_key(value: &[u8]) -> Vec<u8> {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish().to_le_bytes().to_vec()
}

impl ProductStorage for MockPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.clone(),
            });
        }
        Ok(self
            .storage
            .lock()
            .expect("storage poisoned")
            .get(&product_key(&key))
            .cloned())
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .insert(product_key(&key), value);
        Ok(())
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .remove(&product_key(&key));
        Ok(())
    }
}

impl CoreStorage for MockPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(v01::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(self
            .storage
            .lock()
            .expect("storage poisoned")
            .get(&core_key(&key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(v01::GenericError {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .insert(core_key(&key), value);
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(v01::GenericError {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .remove(&core_key(&key));
        Ok(())
    }
}

impl Navigation for MockPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        if let Some(reason) = &self.config.faults.navigate_error {
            return Err(v01::HostNavigateToError::Unknown {
                reason: reason.clone(),
            });
        }
        self.navigations
            .lock()
            .expect("navigations poisoned")
            .push(url);
        Ok(())
    }
}

impl Notifications for MockPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        if let Some(reason) = &self.config.faults.notification_error {
            return Err(v01::GenericError {
                reason: reason.clone(),
            });
        }
        self.notifications
            .lock()
            .expect("notifications poisoned")
            .push(notification);
        let id = self.next_notification_id.fetch_add(1, Ordering::SeqCst);
        Ok(v01::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: v01::NotificationId) -> Result<(), v01::GenericError> {
        self.cancelled_notifications
            .lock()
            .expect("cancellations poisoned")
            .push(id);
        Ok(())
    }
}

impl Permissions for MockPlatform {
    async fn device_permission(
        &self,
        _request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        Ok(v01::HostDevicePermissionResponse {
            granted: self.config.device_permissions.granted(),
        })
    }

    async fn remote_permission(
        &self,
        _request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        Ok(v01::RemotePermissionResponse {
            granted: self.config.remote_permissions.granted(),
        })
    }
}

impl Features for MockPlatform {
    async fn feature_supported(
        &self,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
        let HostFeatureSupportedRequest::V1(_) = request;
        Ok(HostFeatureSupportedResponse::V1(
            v01::HostFeatureSupportedResponse {
                supported: self.config.feature_supported,
            },
        ))
    }
}

/// A configurable chain connection: records outbound requests, and either
/// stays silent (`responses` `None`) or replays canned frames.
struct MockConnection {
    sent: Arc<Mutex<Vec<String>>>,
    responses: Option<Vec<String>>,
}

impl JsonRpcConnection for MockConnection {
    fn send(&self, request: String) {
        self.sent.lock().expect("sent rpc poisoned").push(request);
    }

    fn responses(&self) -> BoxStream<'static, String> {
        match &self.responses {
            None => Box::pin(stream::pending()),
            Some(frames) => Box::pin(stream::iter(frames.clone())),
        }
    }
}

impl ChainProvider for MockPlatform {
    async fn connect(
        &self,
        _genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        match &self.config.chain {
            ChainBehavior::ConnectError(reason) => Err(v01::GenericError {
                reason: reason.clone(),
            }),
            ChainBehavior::Silent => Ok(Box::new(MockConnection {
                sent: self.sent_rpc.clone(),
                responses: None,
            })),
            ChainBehavior::Scripted(frames) => Ok(Box::new(MockConnection {
                sent: self.sent_rpc.clone(),
                responses: Some(frames.clone()),
            })),
            ChainBehavior::Closed => Ok(Box::new(MockConnection {
                sent: self.sent_rpc.clone(),
                responses: Some(Vec::new()),
            })),
        }
    }
}

impl AuthPresenter for MockPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        self.auth_states
            .lock()
            .expect("auth states poisoned")
            .push(state);
    }
}

impl UserConfirmation for MockPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        self.confirmations
            .lock()
            .expect("confirmations poisoned")
            .push(ConfirmKind::of(&review));
        Ok(self.config.confirm_user_actions)
    }
}

impl ThemeHost for MockPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        let theme = self.config.theme;
        // Emit the current theme, then stay open (a live subscription never
        // ends), matching the real host contract.
        Box::pin(
            stream::once(async move { Ok::<v01::ThemeVariant, v01::GenericError>(theme) }).chain(
                stream::pending::<Result<v01::ThemeVariant, v01::GenericError>>(),
            ),
        )
    }
}

impl PreimageHost for MockPlatform {
    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        if let Some(reason) = &self.config.faults.preimage_submit_error {
            return Err(v01::PreimageSubmitError::Unknown {
                reason: reason.clone(),
            });
        }
        let key = preimage_key(&value);
        self.preimages
            .lock()
            .expect("preimages poisoned")
            .insert(key.clone(), value);
        Ok(key)
    }

    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let found = self
            .preimages
            .lock()
            .expect("preimages poisoned")
            .get(&key)
            .cloned();
        Box::pin(stream::once(async move { Ok(found) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use futures::executor::block_on;

    fn resource_review() -> UserConfirmationReview {
        UserConfirmationReview::ResourceAllocation(v01::HostRequestResourceAllocationRequest {
            resources: vec![],
        })
    }

    #[test]
    fn implements_platform() {
        fn assert_platform<P: crate::Platform>(_: &P) {}
        assert_platform(&MockPlatform::new());
    }

    #[test]
    fn product_storage_round_trips_and_is_namespaced() {
        let p = MockPlatform::new();
        block_on(p.write("k".into(), vec![1, 2, 3])).unwrap();
        assert_eq!(block_on(p.read("k".into())).unwrap(), Some(vec![1, 2, 3]));
        // A product key never collides with a core slot.
        assert_eq!(
            block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(),
            None
        );
        block_on(p.clear("k".into())).unwrap();
        assert_eq!(block_on(p.read("k".into())).unwrap(), None);
    }

    #[test]
    fn core_storage_round_trips() {
        let p = MockPlatform::new();
        block_on(p.write_core_storage(CoreStorageKey::AuthSession, vec![7])).unwrap();
        assert_eq!(
            block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(),
            Some(vec![7])
        );
        block_on(p.clear_core_storage(CoreStorageKey::AuthSession)).unwrap();
        assert_eq!(
            block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(),
            None
        );
    }

    #[test]
    fn core_and_product_keys_do_not_collide() {
        let p = MockPlatform::new();
        block_on(p.write_core_storage(CoreStorageKey::AuthSession, vec![1])).unwrap();
        // Reading the same logical name as a product key must miss the core slot.
        assert_eq!(block_on(p.read("auth-session".into())).unwrap(), None);
        assert_eq!(block_on(p.read("core:auth-session".into())).unwrap(), None);
        // ...and a product key must not be visible through core storage.
        block_on(p.write("x".into(), vec![2])).unwrap();
        assert_eq!(
            block_on(
                p.read_core_storage(CoreStorageKey::PermissionAuthorization {
                    storage_key: "x".into()
                })
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn permissions_deny_all_denies_device_and_remote() {
        let p = MockPlatform::with_config(MockConfig {
            device_permissions: PermissionPolicy::DenyAll,
            remote_permissions: PermissionPolicy::DenyAll,
            ..Default::default()
        });
        assert!(
            !block_on(p.device_permission(v01::HostDevicePermissionRequest::Notifications))
                .unwrap()
                .granted
        );
        assert!(
            !block_on(p.remote_permission(v01::RemotePermissionRequest {
                permission: v01::RemotePermission::WebRtc
            }))
            .unwrap()
            .granted
        );
    }

    #[test]
    fn permissions_split_allows_device_denies_remote() {
        let p = MockPlatform::with_config(MockConfig {
            device_permissions: PermissionPolicy::AllowAll,
            remote_permissions: PermissionPolicy::DenyAll,
            ..Default::default()
        });
        assert!(
            block_on(p.device_permission(v01::HostDevicePermissionRequest::Notifications))
                .unwrap()
                .granted
        );
        assert!(
            !block_on(p.remote_permission(v01::RemotePermissionRequest {
                permission: v01::RemotePermission::WebRtc
            }))
            .unwrap()
            .granted
        );
    }

    #[test]
    fn navigation_records_and_can_error() {
        let p = MockPlatform::new();
        block_on(p.navigate_to("a".into())).unwrap();
        block_on(p.navigate_to("b".into())).unwrap();
        assert_eq!(p.navigations(), vec!["a".to_string(), "b".to_string()]);

        let p2 = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                navigate_error: Some("blocked".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(block_on(p2.navigate_to("c".into())).is_err());
    }

    #[test]
    fn notifications_record_order_with_unique_ids() {
        let p = MockPlatform::new();
        let make = |text: &str| v01::HostPushNotificationRequest {
            text: text.to_string(),
            deeplink: None,
            scheduled_at: None,
        };
        let id0 = block_on(p.push_notification(make("one"))).unwrap().id;
        let id1 = block_on(p.push_notification(make("two"))).unwrap().id;
        assert_eq!((id0, id1), (0, 1));
        assert_eq!(p.pushed_notifications().len(), 2);
        block_on(p.cancel_notification(id1)).unwrap();
        assert_eq!(p.cancelled_notifications(), vec![1u32]);
    }

    #[test]
    fn notification_error_injected() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                notification_error: Some("denied".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        let request = v01::HostPushNotificationRequest {
            text: "x".into(),
            deeplink: None,
            scheduled_at: None,
        };
        assert!(block_on(p.push_notification(request)).is_err());
    }

    #[test]
    fn storage_error_injected() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                storage_error: Some("disk".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(block_on(p.read("k".into())).is_err());
        assert!(block_on(p.read_core_storage(CoreStorageKey::AuthSession)).is_err());
    }

    #[test]
    fn confirm_records_kind_and_answers() {
        let p = MockPlatform::new();
        assert!(block_on(p.confirm_user_action(resource_review())).unwrap());
        assert_eq!(p.confirmations(), vec![ConfirmKind::ResourceAllocation]);

        let p2 = MockPlatform::with_config(MockConfig {
            confirm_user_actions: false,
            ..Default::default()
        });
        assert!(!block_on(p2.confirm_user_action(resource_review())).unwrap());
    }

    #[test]
    fn feature_supported_reflects_config() {
        let p = MockPlatform::with_config(MockConfig {
            feature_supported: false,
            ..Default::default()
        });
        let HostFeatureSupportedResponse::V1(response) = block_on(p.feature_supported(
            HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
                genesis_hash: vec![0; 32],
            }),
        ))
        .unwrap();
        assert!(!response.supported);
    }

    #[test]
    fn theme_emits_configured_variant_then_stays_open() {
        let p = MockPlatform::new();
        let mut stream = p.subscribe_theme();
        assert_eq!(
            block_on(stream.next()).unwrap().unwrap(),
            v01::ThemeVariant::Dark
        );
        // A live subscription does not end after the current value.
        assert!(stream.next().now_or_never().is_none());
    }

    #[test]
    fn theme_emits_configured_light() {
        let p = MockPlatform::with_config(MockConfig {
            theme: v01::ThemeVariant::Light,
            ..Default::default()
        });
        assert_eq!(
            block_on(p.subscribe_theme().next()).unwrap().unwrap(),
            v01::ThemeVariant::Light
        );
    }

    #[test]
    fn preimage_submit_then_lookup_round_trips() {
        let p = MockPlatform::new();
        let key = block_on(p.submit_preimage(vec![1, 2, 3])).unwrap();
        let found = block_on(p.lookup_preimage(key).next()).unwrap().unwrap();
        assert_eq!(found, Some(vec![1, 2, 3]));
        // An unknown key misses.
        let miss = block_on(p.lookup_preimage(vec![9, 9, 9, 9, 9, 9, 9, 9]).next())
            .unwrap()
            .unwrap();
        assert_eq!(miss, None);
    }

    #[test]
    fn preimage_submit_error_injected() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                preimage_submit_error: Some("nope".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(block_on(p.submit_preimage(vec![1])).is_err());
    }

    #[test]
    fn chain_silent_records_sends_and_parks() {
        let p = MockPlatform::new();
        let conn = block_on(p.connect(vec![0u8; 32])).unwrap();
        conn.send("req-1".to_string());
        assert_eq!(p.sent_rpc(), vec!["req-1".to_string()]);
        // Silent: the response stream never yields (parks rather than ends).
        assert!(conn.responses().next().now_or_never().is_none());
    }

    #[test]
    fn chain_scripted_replays_frames() {
        let p = MockPlatform::with_config(MockConfig {
            chain: ChainBehavior::Scripted(vec!["frame-1".into(), "frame-2".into()]),
            ..Default::default()
        });
        let conn = block_on(p.connect(vec![0u8; 32])).unwrap();
        let frames: Vec<String> = block_on(conn.responses().collect());
        assert_eq!(frames, vec!["frame-1".to_string(), "frame-2".to_string()]);
    }

    #[test]
    fn chain_closed_ends_stream_immediately() {
        let p = MockPlatform::with_config(MockConfig {
            chain: ChainBehavior::Closed,
            ..Default::default()
        });
        let conn = block_on(p.connect(vec![0u8; 32])).unwrap();
        // Closed ends at once (None), so disconnect paths fail fast instead of
        // parking like Silent.
        assert!(block_on(conn.responses().next()).is_none());
    }

    #[test]
    fn chain_connect_error() {
        let p = MockPlatform::with_config(MockConfig {
            chain: ChainBehavior::ConnectError("offline".into()),
            ..Default::default()
        });
        assert!(block_on(p.connect(vec![0u8; 32])).is_err());
    }

    #[test]
    fn auth_states_record_in_order() {
        let p = MockPlatform::new();
        p.auth_state_changed(AuthState::Disconnected);
        p.auth_state_changed(AuthState::Pairing {
            deeplink: "dl".into(),
        });
        assert_eq!(p.auth_states().len(), 2);
    }

    #[test]
    fn clone_shares_recordings() {
        let p = MockPlatform::new();
        let clone = p.clone();
        block_on(clone.navigate_to("z".into())).unwrap();
        assert_eq!(p.navigations(), vec!["z".to_string()]);
    }
}
