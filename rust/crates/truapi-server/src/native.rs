//! UniFFI-facing native bridge. Exposes [`NativeTrUApiCore`] and the
//! [`HostCallbacks`] callback interface that iOS and Android call into.
//!
//! The native side builds a `CallbackPlatform` that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`SigningHostRuntime`] so the rest of the
//! dispatcher pipeline behaves identically to the WS-bridge and wasm flavors.
//! A native host therefore owns the signer: there is no pairing flow here, and
//! the pairing-host-only entry points are inert.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::task::SpawnExt;
use parity_scale_codec::Encode;
use truapi::v01;
use truapi_platform::{
    AuthPresenter, ChainProvider, CoreStorage, CoreStorageKey, Features, HostInfo,
    JsonRpcConnection, Navigation, Notifications, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, Permissions, PlatformInfo, PreimageHost, ProductContext,
    ProductStorage, RuntimeConfigValidationError, SigningHostConfig, ThemeHost, UserConfirmation,
    UserConfirmationReview, async_trait, normalize_product_identifier,
};

pub mod reviews;

pub use reviews::{
    NativeUserConfirmationReview, ProductAccountId as NativeProductAccountId,
    RingLocation as NativeRingLocation,
};

use crate::SigningHostRuntime;
use crate::host_logic::dotns;
use crate::subscription::Spawner;
#[cfg(feature = "ws-bridge")]
use crate::ws_bridge::{BridgeLogger, WsBridge, WsBridgeEndpoint, WsBridgeStartError};

/// Native-friendly storage error. Mirrors the v0.1 wire shape so the
/// callback surface stays SCALE-free.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostStorageError {
    /// Quota exhausted.
    #[error("storage quota exhausted")]
    Full,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl From<HostStorageError> for v01::HostLocalStorageReadError {
    fn from(err: HostStorageError) -> Self {
        match err {
            HostStorageError::Full => v01::HostLocalStorageReadError::Full,
            HostStorageError::Unknown { reason } => {
                v01::HostLocalStorageReadError::Unknown { reason }
            }
        }
    }
}

/// Native-friendly rejection error returned by callback methods that map
/// onto [`truapi::v01::GenericError`].
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostRejection {
    /// Caller rejected the operation.
    #[error("{reason}")]
    Rejected {
        /// Human-readable rejection reason.
        reason: String,
    },
}

impl From<HostRejection> for v01::GenericError {
    fn from(err: HostRejection) -> Self {
        let HostRejection::Rejected { reason } = err;
        v01::GenericError { reason }
    }
}

impl From<v01::GenericError> for HostRejection {
    fn from(err: v01::GenericError) -> Self {
        HostRejection::Rejected { reason: err.reason }
    }
}

/// Native-friendly navigation error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostNavigateRejection {
    /// User declined the navigation.
    #[error("navigation denied by user")]
    PermissionDenied,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Native-friendly theme enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HostTheme {
    /// Light host theme.
    Light,
    /// Dark host theme.
    Dark,
}

impl From<HostTheme> for v01::ThemeVariant {
    fn from(theme: HostTheme) -> Self {
        match theme {
            HostTheme::Light => v01::ThemeVariant::Light,
            HostTheme::Dark => v01::ThemeVariant::Dark,
        }
    }
}

/// Native-friendly mirror of [`truapi_platform::SessionUiInfo`]: decoded
/// session fields for host account UI, with byte arrays widened to `Vec<u8>`
/// for the FFI surface.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SessionUiInfo {
    /// 32-byte sr25519 root public key of the active session.
    pub public_key: Vec<u8>,
    /// Wallet identity account id used for People-chain username lookup.
    pub identity_account_id: Option<Vec<u8>>,
    /// Short username from the People-chain identity record.
    pub lite_username: Option<String>,
    /// Fully qualified username from the People-chain identity record.
    pub full_username: Option<String>,
}

impl From<truapi_platform::SessionUiInfo> for SessionUiInfo {
    fn from(info: truapi_platform::SessionUiInfo) -> Self {
        Self {
            public_key: info.public_key.to_vec(),
            identity_account_id: info.identity_account_id.map(|id| id.to_vec()),
            lite_username: info.lite_username,
            full_username: info.full_username,
        }
    }
}

/// Native-friendly mirror of [`truapi_platform::AuthState`]. The core emits
/// these in transition order through `HostCallbacks::auth_state_changed`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AuthState {
    /// No active session and no login in progress.
    Disconnected,
    /// A login is in progress: present the pairing deeplink/QR.
    Pairing {
        /// Wallet pairing deeplink to render as a QR code or open directly.
        deeplink: String,
    },
    /// A session is active.
    Connected {
        /// Decoded session fields for host account UI.
        info: SessionUiInfo,
    },
    /// The last login attempt failed; show the reason and offer a retry.
    LoginFailed {
        /// Human-readable failure reason.
        reason: String,
    },
    /// The wallet accepted pairing and the core is resolving the session.
    Authenticating,
}

impl From<truapi_platform::AuthState> for AuthState {
    fn from(state: truapi_platform::AuthState) -> Self {
        match state {
            truapi_platform::AuthState::Disconnected => AuthState::Disconnected,
            truapi_platform::AuthState::Pairing { deeplink } => AuthState::Pairing { deeplink },
            truapi_platform::AuthState::Connected(info) => {
                AuthState::Connected { info: info.into() }
            }
            truapi_platform::AuthState::LoginFailed { reason } => AuthState::LoginFailed { reason },
            truapi_platform::AuthState::Authenticating => AuthState::Authenticating,
        }
    }
}

/// Native-friendly SSO deeplink scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativePairingDeeplinkScheme {
    /// Production Polkadot app.
    PolkadotApp,
    /// Development Polkadot app.
    PolkadotAppDev,
}

/// Native-friendly mirror of [`PermissionAuthorizationStatus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativePermissionAuthorizationStatus {
    /// No persisted authorization exists.
    NotDetermined,
    /// Access is denied.
    Denied,
    /// Access is authorized.
    Authorized,
}

impl From<PermissionAuthorizationStatus> for NativePermissionAuthorizationStatus {
    fn from(status: PermissionAuthorizationStatus) -> Self {
        match status {
            PermissionAuthorizationStatus::NotDetermined => Self::NotDetermined,
            PermissionAuthorizationStatus::Denied => Self::Denied,
            PermissionAuthorizationStatus::Authorized => Self::Authorized,
        }
    }
}

impl From<NativePermissionAuthorizationStatus> for PermissionAuthorizationStatus {
    fn from(status: NativePermissionAuthorizationStatus) -> Self {
        match status {
            NativePermissionAuthorizationStatus::NotDetermined => Self::NotDetermined,
            NativePermissionAuthorizationStatus::Denied => Self::Denied,
            NativePermissionAuthorizationStatus::Authorized => Self::Authorized,
        }
    }
}

/// Native-friendly mirror of [`v01::HostDevicePermissionRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeDevicePermission {
    /// Showing system notifications.
    Notifications,
    /// Camera capture access.
    Camera,
    /// Microphone capture access.
    Microphone,
    /// Bluetooth device access.
    Bluetooth,
    /// NFC reader access.
    Nfc,
    /// Geolocation access.
    Location,
    /// Clipboard access.
    Clipboard,
    /// Opening URLs outside the host.
    OpenUrl,
    /// Biometric authentication.
    Biometrics,
}

impl From<v01::HostDevicePermissionRequest> for NativeDevicePermission {
    fn from(request: v01::HostDevicePermissionRequest) -> Self {
        match request {
            v01::HostDevicePermissionRequest::Notifications => Self::Notifications,
            v01::HostDevicePermissionRequest::Camera => Self::Camera,
            v01::HostDevicePermissionRequest::Microphone => Self::Microphone,
            v01::HostDevicePermissionRequest::Bluetooth => Self::Bluetooth,
            v01::HostDevicePermissionRequest::NFC => Self::Nfc,
            v01::HostDevicePermissionRequest::Location => Self::Location,
            v01::HostDevicePermissionRequest::Clipboard => Self::Clipboard,
            v01::HostDevicePermissionRequest::OpenUrl => Self::OpenUrl,
            v01::HostDevicePermissionRequest::Biometrics => Self::Biometrics,
        }
    }
}

impl From<NativeDevicePermission> for v01::HostDevicePermissionRequest {
    fn from(request: NativeDevicePermission) -> Self {
        match request {
            NativeDevicePermission::Notifications => Self::Notifications,
            NativeDevicePermission::Camera => Self::Camera,
            NativeDevicePermission::Microphone => Self::Microphone,
            NativeDevicePermission::Bluetooth => Self::Bluetooth,
            NativeDevicePermission::Nfc => Self::NFC,
            NativeDevicePermission::Location => Self::Location,
            NativeDevicePermission::Clipboard => Self::Clipboard,
            NativeDevicePermission::OpenUrl => Self::OpenUrl,
            NativeDevicePermission::Biometrics => Self::Biometrics,
        }
    }
}

/// Native-friendly mirror of [`v01::RemotePermission`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeRemotePermission {
    /// Outbound HTTP/WebSocket access to a set of domains.
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    /// WebRTC media access.
    WebRtc,
    /// Submitting chain transactions on behalf of the user.
    ChainSubmit,
    /// Submitting preimages on behalf of the user.
    PreimageSubmit,
    /// Submitting statements on behalf of the user.
    StatementSubmit,
}

impl From<v01::RemotePermission> for NativeRemotePermission {
    fn from(permission: v01::RemotePermission) -> Self {
        match permission {
            v01::RemotePermission::Remote { domains } => Self::Remote { domains },
            v01::RemotePermission::WebRtc => Self::WebRtc,
            v01::RemotePermission::ChainSubmit => Self::ChainSubmit,
            v01::RemotePermission::PreimageSubmit => Self::PreimageSubmit,
            v01::RemotePermission::StatementSubmit => Self::StatementSubmit,
        }
    }
}

impl From<NativeRemotePermission> for v01::RemotePermission {
    fn from(permission: NativeRemotePermission) -> Self {
        match permission {
            NativeRemotePermission::Remote { domains } => Self::Remote { domains },
            NativeRemotePermission::WebRtc => Self::WebRtc,
            NativeRemotePermission::ChainSubmit => Self::ChainSubmit,
            NativeRemotePermission::PreimageSubmit => Self::PreimageSubmit,
            NativeRemotePermission::StatementSubmit => Self::StatementSubmit,
        }
    }
}

/// Native-friendly mirror of [`PermissionAuthorizationRequest`]. Flattens the
/// one-field `RemotePermissionRequest` wrapper into the `Remote` payload.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativePermissionAuthorizationRequest {
    /// Device-level permission such as camera, microphone, or location.
    Device(NativeDevicePermission),
    /// Remote/product-scoped permission such as chain submit or HTTP access.
    Remote(NativeRemotePermission),
    /// Product-scoped permission to disclose the user's primary identity.
    IdentityDisclosure,
    /// Product-scoped permission to access another product's account context.
    AccountAccess {
        /// Product whose account context may be accessed.
        target_product_id: String,
    },
}

impl From<PermissionAuthorizationRequest> for NativePermissionAuthorizationRequest {
    fn from(request: PermissionAuthorizationRequest) -> Self {
        match request {
            PermissionAuthorizationRequest::Device(device) => Self::Device(device.into()),
            PermissionAuthorizationRequest::Remote(remote) => {
                Self::Remote(remote.permission.into())
            }
            PermissionAuthorizationRequest::IdentityDisclosure => Self::IdentityDisclosure,
            PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
                Self::AccountAccess { target_product_id }
            }
        }
    }
}

impl From<NativePermissionAuthorizationRequest> for PermissionAuthorizationRequest {
    fn from(request: NativePermissionAuthorizationRequest) -> Self {
        match request {
            NativePermissionAuthorizationRequest::Device(device) => Self::Device(device.into()),
            NativePermissionAuthorizationRequest::Remote(permission) => {
                Self::Remote(v01::RemotePermissionRequest {
                    permission: permission.into(),
                })
            }
            NativePermissionAuthorizationRequest::IdentityDisclosure => Self::IdentityDisclosure,
            NativePermissionAuthorizationRequest::AccountAccess { target_product_id } => {
                Self::AccountAccess { target_product_id }
            }
        }
    }
}

/// Native-friendly mirror of [`v01::HostPushNotificationRequest`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
    /// Optional Unix timestamp in milliseconds (UTC) at which the
    /// notification should fire. `None` fires immediately.
    pub scheduled_at: Option<u64>,
}

impl From<v01::HostPushNotificationRequest> for PushNotificationRequest {
    fn from(request: v01::HostPushNotificationRequest) -> Self {
        let v01::HostPushNotificationRequest {
            text,
            deeplink,
            scheduled_at,
        } = request;
        Self {
            text,
            deeplink,
            scheduled_at,
        }
    }
}

/// Native-friendly mirror of [`v01::HostFeatureSupportedRequest`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FeatureSupportedRequest {
    /// Ask whether the host can interact with the chain identified by genesis hash.
    Chain {
        /// Chain genesis hash.
        genesis_hash: Vec<u8>,
    },
}

impl From<v01::HostFeatureSupportedRequest> for FeatureSupportedRequest {
    fn from(request: v01::HostFeatureSupportedRequest) -> Self {
        match request {
            v01::HostFeatureSupportedRequest::Chain { genesis_hash } => {
                Self::Chain { genesis_hash }
            }
        }
    }
}

/// Native runtime configuration supplied before product calls are handled.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeRuntimeConfig {
    /// Canonical product identifier used for account derivation.
    pub product_id: String,
    /// Host name shown by the wallet during SSO pairing.
    pub host_name: String,
    /// Optional host icon URL shown by the wallet during SSO pairing.
    pub host_icon: Option<String>,
    /// Optional host version shown by the wallet during SSO pairing.
    pub host_version: Option<String>,
    /// Optional platform/browser name shown by the wallet during SSO pairing.
    pub platform_type: Option<String>,
    /// Optional platform/browser version shown by the wallet during SSO pairing.
    pub platform_version: Option<String>,
    /// People-chain genesis hash. Must be exactly 32 bytes.
    pub people_chain_genesis_hash: Vec<u8>,
    /// Bulletin-chain genesis hash. Must be exactly 32 bytes.
    pub bulletin_chain_genesis_hash: Vec<u8>,
    /// Optional local signing-host secret material (raw BIP-39 entropy).
    pub local_session_secret: Option<Vec<u8>>,
    /// Optional lite username attached to the local signing-host session.
    pub local_session_lite_username: Option<String>,
    /// Deeplink scheme used in pairing QR payloads.
    pub pairing_deeplink_scheme: NativePairingDeeplinkScheme,
}

#[derive(Debug)]
struct NativeResolvedRuntimeConfig {
    signing: SigningHostConfig,
    product: ProductContext,
    local_session_secret: Option<Vec<u8>>,
    local_session_lite_username: Option<String>,
}

/// Native runtime config validation error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeRuntimeConfigError {
    /// Required string field was empty or whitespace-only.
    #[error("{field} must not be empty")]
    EmptyField {
        /// Field name.
        field: String,
    },
    /// People-chain genesis hash was not exactly 32 bytes.
    #[error("people_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidPeopleChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Bulletin-chain genesis hash was not exactly 32 bytes.
    #[error("bulletin_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidBulletinChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Host icon URL could not be parsed.
    #[error("host_icon must be an absolute HTTPS URL: {reason}")]
    InvalidHostIcon {
        /// Parse failure reason.
        reason: String,
    },
    /// Host icon URL used a non-HTTPS scheme.
    #[error("host_icon must use https scheme, got {scheme:?}")]
    InsecureHostIcon {
        /// Actual URL scheme.
        scheme: String,
    },
    /// Pairing deeplink scheme included a URL separator.
    #[error("pairing_deeplink_scheme must not include ://, got {scheme:?}")]
    InvalidDeeplinkScheme {
        /// Actual deeplink scheme value.
        scheme: String,
    },
    /// Product id was not a valid host-spec product identifier.
    #[error("invalid product_id: {product_id}")]
    InvalidProductId {
        /// Actual product id value.
        product_id: String,
    },
    /// Local signing-host session activation failed.
    #[error("failed to activate local signing session: {reason}")]
    LocalSessionActivation {
        /// Activation failure reason.
        reason: String,
    },
}

impl TryFrom<NativeRuntimeConfig> for NativeResolvedRuntimeConfig {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: NativeRuntimeConfig) -> Result<Self, Self::Error> {
        let people_chain_genesis_hash =
            <[u8; 32]>::try_from(config.people_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidPeopleChainGenesisHash {
                    actual: config.people_chain_genesis_hash.len() as u64,
                }
            })?;
        let bulletin_chain_genesis_hash =
            <[u8; 32]>::try_from(config.bulletin_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidBulletinChainGenesisHash {
                    actual: config.bulletin_chain_genesis_hash.len() as u64,
                }
            })?;
        let product =
            ProductContext::new(config.product_id).map_err(NativeRuntimeConfigError::from)?;
        let signing = SigningHostConfig::new(
            HostInfo {
                name: config.host_name,
                icon: config.host_icon,
                version: config.host_version,
            },
            PlatformInfo {
                kind: config.platform_type,
                version: config.platform_version,
            },
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
        )?;
        Ok(Self {
            signing,
            product,
            local_session_secret: config.local_session_secret,
            local_session_lite_username: config.local_session_lite_username,
        })
    }
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
    fn from(err: RuntimeConfigValidationError) -> Self {
        match err {
            RuntimeConfigValidationError::EmptyField { field } => Self::EmptyField {
                field: field.to_string(),
            },
            // `url::ParseError` cannot cross the UniFFI boundary, so the native
            // error keeps a rendered string.
            RuntimeConfigValidationError::InvalidHostIcon { source } => Self::InvalidHostIcon {
                reason: source.to_string(),
            },
            RuntimeConfigValidationError::InsecureHostIcon { scheme } => {
                Self::InsecureHostIcon { scheme }
            }
            RuntimeConfigValidationError::InvalidDeeplinkScheme { scheme } => {
                Self::InvalidDeeplinkScheme { scheme }
            }
            RuntimeConfigValidationError::InvalidProductId { product_id } => {
                Self::InvalidProductId { product_id }
            }
        }
    }
}

impl From<HostNavigateRejection> for v01::HostNavigateToError {
    fn from(err: HostNavigateRejection) -> Self {
        match err {
            HostNavigateRejection::PermissionDenied => v01::HostNavigateToError::PermissionDenied,
            HostNavigateRejection::Unknown { reason } => {
                v01::HostNavigateToError::Unknown { reason }
            }
        }
    }
}

/// Native-friendly mirror of [`dotns::NavigateDecision`], so WebView hosts
/// classify navigations with the core's dotns logic instead of reimplementing
/// it. The open variants carry the ready-to-load canonical URL; `identifier`
/// stays lower-cased/NFC-normalized so hosts can compare it against the
/// current page's identifier for same-domain checks.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NavigateDecision {
    /// A `.dot` identifier plus path/query/hash suffix (no leading `/`).
    DotName {
        /// Lower-cased `.dot` host (e.g. `mytestapp.dot`).
        identifier: String,
        /// Path/query/hash suffix without a leading `/`.
        path: String,
        /// Loadable `https://` URL for this decision.
        canonical_url: String,
    },
    /// A `localhost[:port]` URL plus path/query/hash suffix (no leading `/`).
    Localhost {
        /// `localhost` with optional `:port` suffix.
        host: String,
        /// Path/query/hash suffix without a leading `/`.
        path: String,
        /// Loadable `http://` URL for this decision.
        canonical_url: String,
    },
    /// An absolute external URL with an `http(s):` scheme prepended if missing.
    External {
        /// Canonical URL string.
        url: String,
    },
    /// Input that fails every branch; must not be loaded.
    Reject {
        /// Human-readable reason for the rejection.
        reason: String,
    },
}

impl From<dotns::NavigateDecision> for NavigateDecision {
    /// Total mapping: an open decision that yields no canonical URL becomes
    /// `Reject` rather than panicking, so no unwrap can cross the FFI
    /// boundary and crash the host app.
    fn from(decision: dotns::NavigateDecision) -> Self {
        let canonical_url = decision.canonical_url();
        match (decision, canonical_url) {
            (dotns::NavigateDecision::DotName { identifier, path }, Some(canonical_url)) => {
                Self::DotName {
                    identifier,
                    path,
                    canonical_url,
                }
            }
            (dotns::NavigateDecision::Localhost { host, path }, Some(canonical_url)) => {
                Self::Localhost {
                    host,
                    path,
                    canonical_url,
                }
            }
            (dotns::NavigateDecision::External { url }, _) => Self::External { url },
            (dotns::NavigateDecision::Reject { reason }, _) => Self::Reject { reason },
            (open, None) => Self::Reject {
                reason: format!("{open:?} produced no canonical URL"),
            },
        }
    }
}

/// Classify a navigation input exactly like the core's internal navigate host
/// call: `.dot` first, then `localhost`, then normalized external, with
/// everything else rejected. Pure and stateless; hosts call it on every
/// webview-internal navigation.
#[uniffi::export]
pub fn parse_navigate(input: String) -> NavigateDecision {
    dotns::parse_navigate(&input).into()
}

/// Callback surface that iOS and Android implement.
///
/// Threading contract: every callback executes on the shared bridge
/// executor's worker threads, and blocking one of those threads can stall
/// the entire bridge — not just the request being served. Async callbacks
/// (`navigate_to`, `push_notification`, `device_permission`,
/// `remote_permission`, `feature_supported`, `confirm_user_action`,
/// `lookup_preimage`) are awaited by the core — implementations hop to the
/// main thread for any UI and may keep the future pending arbitrarily long,
/// but must suspend rather than block the polling thread (foreign
/// implementations bridged through UniFFI suspend naturally; the rule
/// chiefly binds Rust implementations). Dropping the returned future
/// cancels the foreign task. The remaining sync callbacks run inline on the
/// dispatcher thread and must return promptly without blocking; in
/// particular `auth_state_changed` should only hand the state to the host
/// UI thread, never wait for the user.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait HostCallbacks: Send + Sync {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    fn on_core_log(&self, marker: String, detail: String);

    /// Open a URL in the system browser.
    async fn navigate_to(&self, url: String) -> Result<(), HostNavigateRejection>;

    /// Deliver a push notification.
    async fn push_notification(
        &self,
        request: PushNotificationRequest,
    ) -> Result<u32, HostRejection>;

    /// Cancel a notification by id.
    fn cancel_notification(&self, id: u32) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...);
    /// the host returns whether the permission was granted.
    async fn device_permission(
        &self,
        request: NativeDevicePermission,
    ) -> Result<bool, HostRejection>;

    /// Prompt the user for a remote (product-scoped) permission.
    async fn remote_permission(
        &self,
        request: NativeRemotePermission,
    ) -> Result<bool, HostRejection>;

    /// Observe an auth state change. Emitted only when the state actually
    /// changes, in transition order: render `Pairing` as the pairing QR UI,
    /// `Connected`/`Disconnected` as the account badge, `LoginFailed` as a
    /// retryable error. User cancellation is reported through
    /// `NativeTrUApiCore.cancel_login()`.
    fn auth_state_changed(&self, state: AuthState);

    /// Read a core-owned host-private storage slot. `key` is a SCALE-encoded
    /// [`CoreStorageKey`].
    fn core_storage_read(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Persist a core-owned host-private storage slot. `key` is a
    /// SCALE-encoded [`CoreStorageKey`].
    fn core_storage_write(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), HostRejection>;

    /// Clear a core-owned host-private storage slot. `key` is a SCALE-encoded
    /// [`CoreStorageKey`].
    fn core_storage_clear(&self, key: Vec<u8>) -> Result<(), HostRejection>;

    /// Open a JSON-RPC connection for a chain. Return a host-assigned
    /// connection id, or `None` when unsupported.
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection>;

    /// Send one JSON-RPC request over a previously opened chain connection.
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection>;

    /// Close a previously opened chain connection.
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection>;

    /// Confirm one user-reviewed core action.
    async fn confirm_user_action(
        &self,
        review: NativeUserConfirmationReview,
    ) -> Result<bool, HostRejection>;

    /// Look up one preimage value by key. The native shim emits this as the
    /// current item in its subscription stream.
    async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Current host theme. The native shim emits this as the current item in
    /// its subscription stream.
    fn current_theme(&self) -> Result<HostTheme, HostRejection>;

    /// Answer a feature-support query.
    async fn feature_supported(
        &self,
        request: FeatureSupportedRequest,
    ) -> Result<bool, HostRejection>;

    /// Read a value from the host's scoped key-value store.
    fn local_storage_read(&self, key: String) -> Result<Option<Vec<u8>>, HostStorageError>;
    /// Write a value to the host's scoped key-value store.
    fn local_storage_write(&self, key: String, value: Vec<u8>) -> Result<(), HostStorageError>;
    /// Clear a value from the host's scoped key-value store.
    fn local_storage_clear(&self, key: String) -> Result<(), HostStorageError>;
}

/// UniFFI object exposing the TrUAPI core to native hosts.
#[derive(uniffi::Object)]
pub struct NativeTrUApiCore {
    runtime: Arc<SigningHostRuntime>,
    product: ProductContext,
    events: Arc<NativeEventBus>,
    #[cfg(feature = "ws-bridge")]
    callbacks: Arc<dyn HostCallbacks>,
    #[cfg(feature = "ws-bridge")]
    bridge: std::sync::Mutex<Option<WsBridge>>,
}

#[uniffi::export]
impl NativeTrUApiCore {
    /// Construct the core with explicit product and pairing runtime config.
    ///
    /// When `runtime_config` carries `local_session_secret`, the session is
    /// activated before this returns, so construction blocks the calling thread
    /// on the same key derivation as [`Self::activate_local_session`]. Prefer
    /// constructing off the host's main/UI thread.
    #[uniffi::constructor]
    pub fn with_runtime_config(
        callbacks: Arc<dyn HostCallbacks>,
        runtime_config: NativeRuntimeConfig,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        native_core_from_platform_config(callbacks, runtime_config.try_into()?)
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    ///
    /// Blocks the calling thread until the disconnect completes, so call it off
    /// the host's main/UI thread.
    pub fn disconnect(&self) {
        futures::executor::block_on(self.runtime.disconnect_session());
    }

    /// Notify this core that host-global session storage changed outside a
    /// direct core write/clear.
    ///
    /// **Inert on a native host.** A signing host owns the active session in
    /// memory, so there is no session-store sync loop to wake. Retained so
    /// hosts written against the pairing-host surface still link.
    pub fn notify_session_store_changed(&self) {
        // Signing hosts own the active local session in memory. There is no
        // pairing-host session-store sync loop to notify.
    }

    /// Cancel an in-flight pairing login.
    ///
    /// **Inert on a native host.** The native bridge runs a signing host, which
    /// has no pairing flow to cancel: `request_login` resolves against the
    /// locally activated session instead. Calling this emits no auth state and
    /// changes nothing. Retained so hosts written against the pairing-host
    /// surface still link.
    pub fn cancel_login(&self) {
        // Signing hosts do not perform SSO pairing when products call
        // request_login; a locally activated session returns AlreadyConnected.
    }

    /// Read a stored permission authorization status without prompting.
    ///
    /// Blocks the calling thread on the storage read, so call it off the host's
    /// main/UI thread.
    pub fn permission_authorization_status(
        &self,
        request: NativePermissionAuthorizationRequest,
    ) -> Result<NativePermissionAuthorizationStatus, HostRejection> {
        let admin = self.runtime.product_admin(self.product.clone());
        let status =
            futures::executor::block_on(admin.permission_authorization_status(request.into()))?;
        Ok(status.into())
    }

    /// Update a stored permission authorization status. Passing
    /// `.notDetermined` clears the stored value so the next product request
    /// prompts again.
    ///
    /// Blocks the calling thread on the storage write, so call it off the host's
    /// main/UI thread.
    pub fn set_permission_authorization_status(
        &self,
        request: NativePermissionAuthorizationRequest,
        status: NativePermissionAuthorizationStatus,
    ) -> Result<(), HostRejection> {
        let admin = self.runtime.product_admin(self.product.clone());
        futures::executor::block_on(
            admin.set_permission_authorization_status(request.into(), status.into()),
        )?;
        Ok(())
    }

    /// Activate or replace the local signing-host session from host-held
    /// secret material (raw BIP-39 entropy).
    ///
    /// Blocks the calling thread while the session is derived (PBKDF2, 2048
    /// rounds), so call it off the host's main/UI thread.
    pub fn activate_local_session(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), HostRejection> {
        futures::executor::block_on(
            self.runtime
                .activate_local_session_with_identity(secret, lite_username),
        )
        .map_err(Into::into)
    }

    /// List registered providers for a ring so host UI can present the RFC-0024
    /// personhood-provider setting.
    pub fn ring_vrf_providers(
        &self,
        ring: NativeRingLocation,
    ) -> Result<Vec<NativeProductAccountId>, HostRejection> {
        let ring = ring.try_into().map_err(native_ring_vrf_input_error)?;
        futures::executor::block_on(self.runtime.ring_vrf_providers(&ring))
            .map(|providers| providers.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }

    /// Return the currently selected provider for a ring.
    pub fn selected_ring_vrf_provider(
        &self,
        ring: NativeRingLocation,
    ) -> Result<Option<NativeProductAccountId>, HostRejection> {
        let ring = ring.try_into().map_err(native_ring_vrf_input_error)?;
        futures::executor::block_on(self.runtime.selected_ring_vrf_provider(&ring))
            .map(|provider| provider.map(Into::into))
            .map_err(Into::into)
    }

    /// Persist a user-selected provider after checking that the handle is
    /// registered for the exact ring.
    pub fn select_ring_vrf_provider(
        &self,
        ring: NativeRingLocation,
        handle: NativeProductAccountId,
    ) -> Result<(), HostRejection> {
        let ring = ring.try_into().map_err(native_ring_vrf_input_error)?;
        let mut handle: v01::ProductAccountId =
            handle.try_into().map_err(native_ring_vrf_input_error)?;
        handle.dot_ns_identifier = normalize_product_identifier(&handle.dot_ns_identifier)
            .map_err(|error| native_ring_vrf_input_error(error.to_string()))?;
        futures::executor::block_on(self.runtime.select_ring_vrf_provider(ring, handle))
            .map_err(Into::into)
    }

    /// Push a host theme update to active TrUAPI theme subscriptions.
    pub fn notify_theme_changed(&self, theme: HostTheme) {
        self.events.notify_theme_changed(theme.into());
    }

    /// Push a preimage lookup update to active subscriptions for `key`.
    ///
    /// `value == None` represents a known miss; `Some(bytes)` represents the
    /// current preimage value.
    pub fn notify_preimage_changed(&self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.events.notify_preimage_changed(&key, value);
    }

    /// Push a JSON-RPC response from a native chain connection into the core.
    pub fn notify_chain_response(&self, connection_id: u32, json: String) {
        self.events.notify_chain_response(connection_id, json);
    }

    /// Notify the core that a native chain connection closed externally.
    pub fn notify_chain_closed(&self, connection_id: u32) {
        self.events.notify_chain_closed(connection_id);
    }
}

fn native_ring_vrf_input_error(reason: String) -> HostRejection {
    HostRejection::Rejected { reason }
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`) for
/// the `tracing` output, which on native routes to stderr (system logs on
/// iOS/Android). Most native diagnostics flow through `on_core_log` instead;
/// this controls the cross-platform `tracing` events shared with wasm.
#[uniffi::export]
pub fn set_log_level(level: String) {
    crate::logging::set_level_from_str(&level);
}

fn native_core_from_platform_config(
    callbacks: Arc<dyn HostCallbacks>,
    runtime_config: NativeResolvedRuntimeConfig,
) -> Result<Arc<NativeTrUApiCore>, NativeRuntimeConfigError> {
    crate::logging::init();
    callbacks.on_core_log(
        "truapi.native.core.boot".to_string(),
        "core ready".to_string(),
    );

    let events = Arc::new(NativeEventBus::default());
    let platform = Arc::new(CallbackPlatform {
        callbacks: callbacks.clone(),
        events: events.clone(),
    });
    let spawner = native_thread_pool_spawner(&callbacks);
    let runtime = Arc::new(SigningHostRuntime::new(
        platform,
        runtime_config.signing,
        spawner,
    ));

    if let Some(secret) = runtime_config.local_session_secret {
        futures::executor::block_on(runtime.activate_local_session_with_identity(
            secret,
            runtime_config.local_session_lite_username,
        ))
        .map_err(|err| NativeRuntimeConfigError::LocalSessionActivation { reason: err.reason })?;
    }

    Ok(Arc::new(NativeTrUApiCore {
        runtime,
        product: runtime_config.product,
        events,
        #[cfg(feature = "ws-bridge")]
        callbacks,
        #[cfg(feature = "ws-bridge")]
        bridge: std::sync::Mutex::new(None),
    }))
}

#[cfg(feature = "ws-bridge")]
#[uniffi::export]
impl NativeTrUApiCore {
    /// Start the localhost WebSocket bridge. Returns the descriptor the
    /// host hands to the product so it can dial back in.
    pub fn start_ws_bridge(&self, bind_port: u16) -> Result<WsBridgeEndpoint, WsBridgeStartError> {
        let mut guard = self.bridge.lock().unwrap();
        if guard.is_some() {
            return Err(WsBridgeStartError::AlreadyRunning);
        }
        let logger: BridgeLogger = {
            let callbacks = self.callbacks.clone();
            Arc::new(move |marker: &str, detail: &str| {
                callbacks.on_core_log(marker.to_string(), detail.to_string());
            })
        };
        let runtime = self.runtime.clone();
        let product = self.product.clone();
        let runtime_factory = Arc::new(move |sink| runtime.product_runtime(product.clone(), sink));
        let (bridge, endpoint) = WsBridge::start(bind_port, runtime_factory, logger)?;
        *guard = Some(bridge);
        Ok(endpoint)
    }

    /// Stop the localhost WebSocket bridge (if running).
    pub fn stop_ws_bridge(&self) {
        if let Some(mut bridge) = self.bridge.lock().unwrap().take() {
            bridge.stop();
        }
    }
}

/// Build a [`Spawner`] backed by a shared `futures::executor::ThreadPool`.
/// The pool is sized at the default (one worker per logical CPU). Falls
/// back to a thread-per-subscription spawner if the pool fails to build,
/// which only ever happens if the host has no available threads at all.
fn native_thread_pool_spawner(callbacks: &Arc<dyn HostCallbacks>) -> Spawner {
    match ThreadPool::new() {
        Ok(pool) => {
            let callbacks = callbacks.clone();
            Arc::new(move |fut: BoxFuture<'static, ()>| {
                if let Err(err) = pool.spawn(fut) {
                    callbacks.on_core_log(
                        "truapi.native.core.subscription.spawn_failed".to_string(),
                        format!("{err}"),
                    );
                }
            })
        }
        Err(err) => {
            callbacks.on_core_log(
                "truapi.native.core.subscription.pool_unavailable".to_string(),
                format!("{err}; falling back to thread-per-subscription"),
            );
            crate::subscription::thread_per_subscription_spawner()
        }
    }
}

struct CallbackPlatform {
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
}

#[derive(Default)]
struct NativeEventBus {
    theme_changes: Mutex<Vec<mpsc::UnboundedSender<Result<v01::ThemeVariant, v01::GenericError>>>>,
    preimage_changes: Mutex<Vec<PreimageSubscription>>,
    chain_responses: Mutex<HashMap<u32, mpsc::UnboundedSender<String>>>,
}

struct PreimageSubscription {
    key: Vec<u8>,
    tx: mpsc::UnboundedSender<Result<Option<Vec<u8>>, v01::GenericError>>,
}

impl NativeEventBus {
    fn subscribe_theme(
        &self,
        current: Result<v01::ThemeVariant, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_theme_changed(&self, theme: v01::ThemeVariant) {
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(theme)).is_ok());
    }

    fn subscribe_preimage_changes(
        &self,
        key: Vec<u8>,
    ) -> mpsc::UnboundedReceiver<Result<Option<Vec<u8>>, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .push(PreimageSubscription { key, tx });
        rx
    }

    fn notify_preimage_changed(&self, key: &[u8], value: Option<Vec<u8>>) {
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .retain(|sub| {
                if sub.key != key {
                    return true;
                }
                sub.tx.unbounded_send(Ok(value.clone())).is_ok()
            });
    }

    fn register_chain(&self, connection_id: u32) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded();
        self.chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned")
            .insert(connection_id, tx);
        rx
    }

    fn notify_chain_response(&self, connection_id: u32, json: String) {
        let mut responses = self
            .chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned");
        let Some(tx) = responses.get(&connection_id) else {
            return;
        };
        if tx.unbounded_send(json).is_err() {
            responses.remove(&connection_id);
        }
    }

    fn notify_chain_closed(&self, connection_id: u32) {
        self.chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned")
            .remove(&connection_id);
    }
}

#[async_trait]
impl Navigation for CallbackPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.navigate_to".to_string(),
            url.clone(),
        );
        self.callbacks.navigate_to(url).await.map_err(Into::into)
    }
}

#[async_trait]
impl Notifications for CallbackPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.push_notification".to_string(),
            notification.text.clone(),
        );

        let id = self
            .callbacks
            .push_notification(notification.into())
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.cancel_notification".to_string(),
            id.to_string(),
        );
        self.callbacks
            .cancel_notification(id)
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl Permissions for CallbackPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission".to_string(),
            format!("{request}"),
        );

        let granted = self
            .callbacks
            .device_permission(request.into())
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostDevicePermissionResponse { granted })
    }

    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.remote_permission".to_string(),
            format!("{request}"),
        );

        let granted = self
            .callbacks
            .remote_permission(request.permission.into())
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

#[async_trait]
impl Features for CallbackPlatform {
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.feature_supported".to_string(),
            format!("{request:?}"),
        );

        let supported = self
            .callbacks
            .feature_supported(request.into())
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostFeatureSupportedResponse { supported })
    }
}

#[async_trait]
impl ProductStorage for CallbackPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_read(key).map_err(Into::into)
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks
            .local_storage_write(key, value)
            .map_err(Into::into)
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_clear(key).map_err(Into::into)
    }
}

#[async_trait]
impl CoreStorage for CallbackPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.callbacks
            .core_storage_read(key.encode())
            .map_err(v01::GenericError::from)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_write(key.encode(), value)
            .map_err(v01::GenericError::from)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_clear(key.encode())
            .map_err(v01::GenericError::from)
    }
}

struct NativeJsonRpcConnection {
    id: u32,
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
    response_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for NativeJsonRpcConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        if let Err(err) = self.callbacks.chain_send(self.id, request) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_send_failed".to_string(),
                err.to_string(),
            );
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => {
                self.callbacks.on_core_log(
                    "truapi.native.chain.responses_reused".to_string(),
                    "responses() called more than once".to_string(),
                );
                stream::empty().boxed()
            }
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        self.events.notify_chain_closed(self.id);
        if let Err(err) = self.callbacks.chain_close(self.id) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_close_failed".to_string(),
                err.to_string(),
            );
        }
    }
}

impl Drop for NativeJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[async_trait]
impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let Some(connection_id) = self
            .callbacks
            .chain_connect(genesis_hash.to_vec())
            .map_err(v01::GenericError::from)?
        else {
            return Err(v01::GenericError {
                reason: "chain provider unavailable".to_string(),
            });
        };
        let response_rx = self.events.register_chain(connection_id);
        Ok(Box::new(NativeJsonRpcConnection {
            id: connection_id,
            callbacks: self.callbacks.clone(),
            events: self.events.clone(),
            response_rx: Mutex::new(Some(response_rx)),
            closed: AtomicBool::new(false),
        }))
    }
}

impl AuthPresenter for CallbackPlatform {
    fn auth_state_changed(&self, state: truapi_platform::AuthState) {
        self.callbacks.on_core_log(
            "truapi.native.callback.auth_state_changed".to_string(),
            String::new(),
        );
        self.callbacks.auth_state_changed(state.into());
    }
}

#[async_trait]
impl UserConfirmation for CallbackPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_user_action".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_user_action(review.into())
            .await
            .map_err(v01::GenericError::from)
    }
}

impl ThemeHost for CallbackPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        let current = self
            .callbacks
            .current_theme()
            .map(v01::ThemeVariant::from)
            .map_err(v01::GenericError::from);
        self.events.subscribe_theme(current)
    }
}

impl PreimageHost for CallbackPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        // Register the change receiver first so no event between the lookup
        // and the subscription is lost, then await the current value lazily.
        let rx = self.events.subscribe_preimage_changes(key.clone());
        let callbacks = self.callbacks.clone();
        let current = async move {
            callbacks
                .lookup_preimage(key)
                .await
                .map_err(v01::GenericError::from)
        };
        stream::once(current).chain(rx).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type PreimageFixtureEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

    struct EventCallbacks {
        theme: Mutex<HostTheme>,
        preimages: Mutex<PreimageFixtureEntries>,
        auth_states: Mutex<Vec<AuthState>>,
        chain_id: Mutex<Option<u32>>,
        chain_connects: Mutex<Vec<Vec<u8>>>,
        chain_sends: Mutex<Vec<(u32, String)>>,
        chain_closes: Mutex<Vec<u32>>,
    }

    impl EventCallbacks {
        fn new() -> Self {
            Self {
                theme: Mutex::new(HostTheme::Light),
                preimages: Mutex::new(Vec::new()),
                auth_states: Mutex::new(Vec::new()),
                chain_id: Mutex::new(None),
                chain_connects: Mutex::new(Vec::new()),
                chain_sends: Mutex::new(Vec::new()),
                chain_closes: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl HostCallbacks for EventCallbacks {
        fn on_core_log(&self, _marker: String, _detail: String) {}
        async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
            Ok(())
        }
        async fn push_notification(
            &self,
            _request: PushNotificationRequest,
        ) -> Result<u32, HostRejection> {
            Ok(0)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn device_permission(
            &self,
            _request: NativeDevicePermission,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn remote_permission(
            &self,
            _request: NativeRemotePermission,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn auth_state_changed(&self, state: AuthState) {
            self.auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .push(state);
        }
        fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn core_storage_write(&self, _key: Vec<u8>, _value: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
            self.chain_connects
                .lock()
                .expect("chain connects mutex poisoned")
                .push(genesis_hash);
            Ok(*self.chain_id.lock().expect("chain id mutex poisoned"))
        }
        fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection> {
            self.chain_sends
                .lock()
                .expect("chain sends mutex poisoned")
                .push((connection_id, request));
            Ok(())
        }
        fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection> {
            self.chain_closes
                .lock()
                .expect("chain closes mutex poisoned")
                .push(connection_id);
            Ok(())
        }
        async fn confirm_user_action(
            &self,
            _review: NativeUserConfirmationReview,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(self
                .preimages
                .lock()
                .expect("preimage map mutex poisoned")
                .iter()
                .find(|(stored_key, _)| stored_key == &key)
                .and_then(|(_, value)| value.clone()))
        }
        fn current_theme(&self) -> Result<HostTheme, HostRejection> {
            Ok(*self.theme.lock().expect("theme mutex poisoned"))
        }
        async fn feature_supported(
            &self,
            _request: FeatureSupportedRequest,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn local_storage_read(&self, _key: String) -> Result<Option<Vec<u8>>, HostStorageError> {
            Ok(None)
        }
        fn local_storage_write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), HostStorageError> {
            Ok(())
        }
        fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
            Ok(())
        }
    }

    fn event_platform() -> (Arc<EventCallbacks>, Arc<NativeEventBus>, CallbackPlatform) {
        let callbacks = Arc::new(EventCallbacks::new());
        let events = Arc::new(NativeEventBus::default());
        let platform = CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
        };
        (callbacks, events, platform)
    }

    fn native_runtime_config(product_id: &str) -> NativeRuntimeConfig {
        NativeRuntimeConfig {
            product_id: product_id.to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://example.invalid/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0xa2; 32],
            bulletin_chain_genesis_hash: vec![0xbb; 32],
            local_session_secret: None,
            local_session_lite_username: None,
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        }
    }

    #[test]
    fn permission_authorization_request_mirror_round_trips() {
        let device_cases = [
            v01::HostDevicePermissionRequest::Notifications,
            v01::HostDevicePermissionRequest::Camera,
            v01::HostDevicePermissionRequest::Microphone,
            v01::HostDevicePermissionRequest::Bluetooth,
            v01::HostDevicePermissionRequest::NFC,
            v01::HostDevicePermissionRequest::Location,
            v01::HostDevicePermissionRequest::Clipboard,
            v01::HostDevicePermissionRequest::OpenUrl,
            v01::HostDevicePermissionRequest::Biometrics,
        ];
        let remote_cases = [
            v01::RemotePermission::Remote {
                domains: vec!["a.dot".to_string(), "b.dot".to_string()],
            },
            v01::RemotePermission::WebRtc,
            v01::RemotePermission::ChainSubmit,
            v01::RemotePermission::PreimageSubmit,
            v01::RemotePermission::StatementSubmit,
        ];

        let mut cases: Vec<PermissionAuthorizationRequest> = Vec::new();
        cases.extend(
            device_cases
                .into_iter()
                .map(PermissionAuthorizationRequest::Device),
        );
        cases.extend(remote_cases.into_iter().map(|permission| {
            PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest { permission })
        }));
        cases.push(PermissionAuthorizationRequest::IdentityDisclosure);
        cases.push(PermissionAuthorizationRequest::AccountAccess {
            target_product_id: "other.dot".to_string(),
        });

        for case in cases {
            let native = NativePermissionAuthorizationRequest::from(case.clone());
            assert_eq!(PermissionAuthorizationRequest::from(native), case);
        }
    }

    #[test]
    fn native_auth_presenter_forwards_states_across_the_ffi_mirror() {
        let (callbacks, _events, platform) = event_platform();

        platform.auth_state_changed(truapi_platform::AuthState::Pairing {
            deeplink: "polkadotapp://pair?handshake=00".to_string(),
        });
        platform.auth_state_changed(truapi_platform::AuthState::Connected(
            truapi_platform::SessionUiInfo {
                public_key: [7; 32],
                identity_account_id: None,
                lite_username: Some("alice".to_string()),
                full_username: None,
            },
        ));
        platform.auth_state_changed(truapi_platform::AuthState::Disconnected);

        assert_eq!(
            callbacks
                .auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .as_slice(),
            &[
                AuthState::Pairing {
                    deeplink: "polkadotapp://pair?handshake=00".to_string(),
                },
                AuthState::Connected {
                    info: SessionUiInfo {
                        public_key: vec![7; 32],
                        identity_account_id: None,
                        lite_username: Some("alice".to_string()),
                        full_username: None,
                    },
                },
                AuthState::Disconnected,
            ]
        );
    }

    #[test]
    fn native_theme_subscription_emits_current_then_notified_changes() {
        let (callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_theme();

        let first = futures::executor::block_on(stream.next()).unwrap();
        *callbacks.theme.lock().expect("theme mutex poisoned") = HostTheme::Dark;
        events.notify_theme_changed(v01::ThemeVariant::Dark);
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(first.unwrap(), v01::ThemeVariant::Light);
        assert_eq!(second.unwrap(), v01::ThemeVariant::Dark);
    }

    #[test]
    fn native_preimage_subscription_emits_current_then_notified_value() {
        let (callbacks, events, platform) = event_platform();
        let key = vec![7; 32];
        callbacks
            .preimages
            .lock()
            .expect("preimage map mutex poisoned")
            .push((key.clone(), Some(vec![1, 2, 3])));
        let mut stream = platform.lookup_preimage(key.clone());

        let first = futures::executor::block_on(stream.next()).unwrap();
        events.notify_preimage_changed(&key, Some(vec![4, 5, 6]));
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(first.unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(second.unwrap(), Some(vec![4, 5, 6]));
    }

    #[test]
    fn native_chain_provider_forwards_send_response_and_close() {
        let (callbacks, events, platform) = event_platform();
        *callbacks.chain_id.lock().expect("chain id mutex poisoned") = Some(42);
        let genesis = [9; 32];

        let connection = futures::executor::block_on(ChainProvider::connect(&platform, genesis))
            .expect("chain connection should open");
        connection.send(r#"{"jsonrpc":"2.0","id":1}"#.to_string());
        let mut responses = connection.responses();
        events.notify_chain_response(42, r#"{"jsonrpc":"2.0","id":1,"result":true}"#.to_string());
        let response = futures::executor::block_on(responses.next()).unwrap();
        drop(responses);
        drop(connection);

        assert_eq!(
            callbacks
                .chain_connects
                .lock()
                .expect("chain connects mutex poisoned")
                .as_slice(),
            &[genesis.to_vec()]
        );
        assert_eq!(
            callbacks
                .chain_sends
                .lock()
                .expect("chain sends mutex poisoned")
                .as_slice(),
            &[(42, r#"{"jsonrpc":"2.0","id":1}"#.to_string())]
        );
        assert_eq!(response, r#"{"jsonrpc":"2.0","id":1,"result":true}"#);
        assert_eq!(
            callbacks
                .chain_closes
                .lock()
                .expect("chain closes mutex poisoned")
                .as_slice(),
            &[42]
        );
    }

    #[test]
    fn runtime_config_rejects_wrong_size_genesis_hash() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            people_chain_genesis_hash: vec![0; 31],
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidPeopleChainGenesisHash { actual: 31 }
        ));
    }

    #[test]
    fn runtime_config_rejects_empty_required_fields() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            product_id: " ".to_string(),
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::EmptyField { field } if field == "product_id"
        ));
    }

    #[test]
    fn runtime_config_rejects_relative_host_icon() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            host_icon: Some("/dotli.png".to_string()),
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidHostIcon { .. }
        ));
    }

    #[test]
    fn runtime_config_rejects_non_https_host_icon() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            host_icon: Some("http://localhost:3000/dotli.png".to_string()),
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InsecureHostIcon { scheme } if scheme == "http"
        ));
    }

    /// Calling `start_ws_bridge` twice on the same `NativeTrUApiCore`
    /// without an intervening `stop_ws_bridge` is a hard error. The bridge
    /// is single-instance per core, so the second start must surface
    /// `AlreadyRunning` rather than silently leaking a worker thread.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn start_ws_bridge_twice_returns_already_running() {
        struct Noop;
        #[async_trait::async_trait]
        impl HostCallbacks for Noop {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            async fn push_notification(
                &self,
                _request: PushNotificationRequest,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn device_permission(
                &self,
                _request: NativeDevicePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn remote_permission(
                &self,
                _request: NativeRemotePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn core_storage_write(
                &self,
                _key: Vec<u8>,
                _value: Vec<u8>,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_send(
                &self,
                _connection_id: u32,
                _request: String,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn confirm_user_action(
                &self,
                _review: NativeUserConfirmationReview,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn lookup_preimage(
                &self,
                _key: Vec<u8>,
            ) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<HostTheme, HostRejection> {
                Ok(HostTheme::Light)
            }
            async fn feature_supported(
                &self,
                _request: FeatureSupportedRequest,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn local_storage_read(
                &self,
                _key: String,
            ) -> Result<Option<Vec<u8>>, HostStorageError> {
                Ok(None)
            }
            fn local_storage_write(
                &self,
                _key: String,
                _value: Vec<u8>,
            ) -> Result<(), HostStorageError> {
                Ok(())
            }
            fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
                Ok(())
            }
        }

        let core = NativeTrUApiCore::with_runtime_config(
            Arc::new(Noop),
            NativeRuntimeConfig {
                host_icon: Some("https://dot.li/dotli.png".to_string()),
                ..native_runtime_config("dotli.dot")
            },
        )
        .expect("runtime config should be valid");
        let _first = core.start_ws_bridge(0).expect("first start must succeed");
        let err = core
            .start_ws_bridge(0)
            .expect_err("second start must error");
        assert!(matches!(err, WsBridgeStartError::AlreadyRunning));
        core.stop_ws_bridge();
    }

    /// A permission callback suspends while awaiting the user's decision and
    /// holds no executor worker, so an unrelated request on the same
    /// connection still round-trips while the decision is pending.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn pending_permission_decision_does_not_stall_bridge() {
        use std::sync::atomic::{AtomicBool, Ordering};

        use futures::SinkExt;
        use parity_scale_codec::Decode;
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        use truapi::versioned::permissions::HostDevicePermissionRequest;
        use truapi::versioned::system::HostFeatureSupportedRequest;

        use crate::frame::{Payload, ProtocolMessage, request_ids};

        /// `device_permission` stays pending until the test sends on
        /// `release`; every other callback is a trivial success.
        struct GatedPermissionCallbacks {
            permission_entered: Arc<AtomicBool>,
            release: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>,
        }

        #[async_trait::async_trait]
        impl HostCallbacks for GatedPermissionCallbacks {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            async fn push_notification(
                &self,
                _request: PushNotificationRequest,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn device_permission(
                &self,
                _request: NativeDevicePermission,
            ) -> Result<bool, HostRejection> {
                self.permission_entered.store(true, Ordering::SeqCst);
                self.release
                    .lock()
                    .await
                    .recv()
                    .await
                    .expect("release signal");
                Ok(true)
            }
            async fn remote_permission(
                &self,
                _request: NativeRemotePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn core_storage_write(
                &self,
                _key: Vec<u8>,
                _value: Vec<u8>,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_send(
                &self,
                _connection_id: u32,
                _request: String,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn confirm_user_action(
                &self,
                _review: NativeUserConfirmationReview,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn lookup_preimage(
                &self,
                _key: Vec<u8>,
            ) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<HostTheme, HostRejection> {
                Ok(HostTheme::Light)
            }
            async fn feature_supported(
                &self,
                _request: FeatureSupportedRequest,
            ) -> Result<bool, HostRejection> {
                Ok(true)
            }
            fn local_storage_read(
                &self,
                _key: String,
            ) -> Result<Option<Vec<u8>>, HostStorageError> {
                Ok(None)
            }
            fn local_storage_write(
                &self,
                _key: String,
                _value: Vec<u8>,
            ) -> Result<(), HostStorageError> {
                Ok(())
            }
            fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
                Ok(())
            }
        }

        let (release_tx, release_rx) = tokio::sync::mpsc::channel::<()>(1);
        let permission_entered = Arc::new(AtomicBool::new(false));
        let core = NativeTrUApiCore::with_runtime_config(
            Arc::new(GatedPermissionCallbacks {
                permission_entered: permission_entered.clone(),
                release: tokio::sync::Mutex::new(release_rx),
            }),
            NativeRuntimeConfig {
                host_icon: Some("https://dot.li/dotli.png".to_string()),
                ..native_runtime_config("dotli.dot")
            },
        )
        .expect("runtime config should be valid");
        let endpoint = core.start_ws_bridge(0).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let permission_ids =
            request_ids("permissions_request_device_permission").expect("known request method");
        let feature_ids = request_ids("system_feature_supported").expect("known request method");
        let (feature_response, permission_response) = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

            let permission_frame = ProtocolMessage {
                request_id: "p:permission".into(),
                payload: Payload {
                    id: permission_ids.request_id,
                    value: HostDevicePermissionRequest::V1(
                        v01::HostDevicePermissionRequest::Camera,
                    )
                    .encode(),
                },
            };
            ws.send(WsMessage::Binary(permission_frame.encode()))
                .await
                .expect("send device permission");

            // Wait until the permission callback is blocked on the decision.
            for _ in 0..1000 {
                if permission_entered.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                permission_entered.load(Ordering::SeqCst),
                "permission callback was not invoked"
            );

            let feature_frame = ProtocolMessage {
                request_id: "p:feature".into(),
                payload: Payload {
                    id: feature_ids.request_id,
                    value: HostFeatureSupportedRequest::V1(
                        v01::HostFeatureSupportedRequest::Chain {
                            genesis_hash: vec![0u8; 32],
                        },
                    )
                    .encode(),
                },
            };
            ws.send(WsMessage::Binary(feature_frame.encode()))
                .await
                .expect("send feature_supported");

            let feature_response =
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        match ws.next().await {
                            Some(Ok(WsMessage::Binary(bytes))) => {
                                break ProtocolMessage::decode(&mut &bytes[..])
                                    .expect("decode response");
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => panic!("ws error: {err}"),
                            None => panic!("connection closed before response"),
                        }
                    }
                })
                .await
                .expect("feature_supported must answer while the permission decision is pending");

            release_tx
                .send(())
                .await
                .expect("release permission callback");
            let permission_response =
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        match ws.next().await {
                            Some(Ok(WsMessage::Binary(bytes))) => {
                                break ProtocolMessage::decode(&mut &bytes[..])
                                    .expect("decode response");
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => panic!("ws error: {err}"),
                            None => panic!("connection closed before response"),
                        }
                    }
                })
                .await
                .expect("released permission must answer");

            (feature_response, permission_response)
        });

        assert_eq!(feature_response.request_id, "p:feature");
        assert_eq!(feature_response.payload.id, feature_ids.response_id);

        assert_eq!(permission_response.request_id, "p:permission");
        assert_eq!(permission_response.payload.id, permission_ids.response_id);
        // [Ok 0x00][V1 0x00][granted=1]
        assert_eq!(permission_response.payload.value, vec![0x00, 0x00, 0x01]);

        core.stop_ws_bridge();
    }

    #[test]
    fn exported_parse_navigate_maps_every_variant() {
        assert_eq!(
            parse_navigate("mytestapp.dot/some/path?q=1".to_string()),
            NavigateDecision::DotName {
                identifier: "mytestapp.dot".to_string(),
                path: "some/path?q=1".to_string(),
                canonical_url: "https://mytestapp.dot/some/path?q=1".to_string(),
            }
        );
        assert_eq!(
            parse_navigate("Example.DOT".to_string()),
            NavigateDecision::DotName {
                identifier: "example.dot".to_string(),
                path: String::new(),
                canonical_url: "https://example.dot".to_string(),
            }
        );
        assert_eq!(
            parse_navigate("localhost:3000/path#h".to_string()),
            NavigateDecision::Localhost {
                host: "localhost:3000".to_string(),
                path: "path#h".to_string(),
                canonical_url: "http://localhost:3000/path#h".to_string(),
            }
        );
        assert_eq!(
            parse_navigate("google.com".to_string()),
            NavigateDecision::External {
                url: "https://google.com/".to_string(),
            }
        );
        assert!(matches!(
            parse_navigate("javascript:alert(1)".to_string()),
            NavigateDecision::Reject { .. }
        ));
    }

    /// The FFI mirror's canonical URL must stay byte-identical to what the
    /// runtime's internal navigate path computes from the same input.
    #[test]
    fn exported_canonical_url_matches_host_logic() {
        let inputs = [
            "mytestapp.dot",
            "mytestapp.dot/some/path?q=1#frag",
            "localhost",
            "localhost:3000/path",
            "https://example.com/page",
        ];
        for input in inputs {
            let expected = crate::host_logic::dotns::parse_navigate(input)
                .canonical_url()
                .expect("open decision has a canonical URL");
            let actual = match parse_navigate(input.to_string()) {
                NavigateDecision::DotName { canonical_url, .. }
                | NavigateDecision::Localhost { canonical_url, .. } => canonical_url,
                NavigateDecision::External { url } => url,
                NavigateDecision::Reject { reason } => {
                    panic!("{input}: unexpected rejection: {reason}")
                }
            };
            assert_eq!(actual, expected, "{input}");
        }
    }
}
