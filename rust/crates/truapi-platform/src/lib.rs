//! Capability traits a TrUAPI host must implement.
//!
//! Each trait covers a single OS-primitive surface the Rust core cannot reach
//! from its own process (key-value persistence, URL launching, push
//! notifications, permission UI, chain RPC, host-selected preimage backends).
//! Account management, signing, and statement-store protocol flows live in the
//! Rust core itself and are not part of this trait set.
//!
//! Host implementations may use `async fn` in trait bodies directly. The
//! consumers (`truapi-server::runtime::PlatformRuntimeHost<P>`) are generic
//! over `P: Platform`, so `dyn Trait` object safety is not required.

#![forbid(unsafe_code)]

use futures::stream::BoxStream;

use truapi::v01::{
    GenericError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    HostLocalStorageReadError, HostNavigateToError, HostPushNotificationRequest,
    HostPushNotificationResponse, NotificationId, PreimageSubmitError, RemotePermissionRequest,
    RemotePermissionResponse, ThemeVariant,
};
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use url::Url;

/// Re-export of `truapi::v01` for host implementations.
pub use truapi::v01;
/// Re-export of `truapi::versioned` for host implementations.
pub use truapi::versioned;

/// Static runtime configuration supplied by the embedding host before the
/// core handles product-scoped calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Canonical product identifier used for account derivation.
    pub product_id: String,
    /// Host name shown by the wallet during SSO pairing.
    pub host_name: String,
    /// Optional host icon URL/CID shown by the wallet during SSO pairing.
    pub host_icon: Option<String>,
    /// Optional host version shown by the wallet during SSO pairing.
    pub host_version: Option<String>,
    /// Optional platform/browser name shown by the wallet during SSO pairing.
    pub platform_type: Option<String>,
    /// Optional platform/browser version shown by the wallet during SSO pairing.
    pub platform_version: Option<String>,
    /// People-chain genesis hash used for statement-store SSO.
    pub people_chain_genesis_hash: [u8; 32],
    /// Deeplink scheme used in pairing QR payloads.
    pub pairing_deeplink_scheme: PairingDeeplinkScheme,
}

impl RuntimeConfig {
    /// Build a runtime config, validating fields whose representation cannot
    /// be made invalid by Rust types alone.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        product_id: String,
        host_name: String,
        host_icon: Option<String>,
        host_version: Option<String>,
        platform_type: Option<String>,
        platform_version: Option<String>,
        people_chain_genesis_hash: [u8; 32],
        pairing_deeplink_scheme: PairingDeeplinkScheme,
    ) -> Result<Self, RuntimeConfigValidationError> {
        let config = Self {
            product_id,
            host_name,
            host_icon,
            host_version,
            platform_type,
            platform_version,
            people_chain_genesis_hash,
            pairing_deeplink_scheme,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), RuntimeConfigValidationError> {
        require_non_empty("product_id", &self.product_id)?;
        require_non_empty("host_name", &self.host_name)?;
        if let Some(icon) = &self.host_icon {
            let parsed =
                Url::parse(icon).map_err(|err| RuntimeConfigValidationError::InvalidHostIcon {
                    reason: err.to_string(),
                })?;
            if parsed.scheme() != "https" {
                return Err(RuntimeConfigValidationError::InsecureHostIcon {
                    scheme: parsed.scheme().to_string(),
                });
            }
        }
        Ok(())
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), RuntimeConfigValidationError> {
    if value.trim().is_empty() {
        return Err(RuntimeConfigValidationError::EmptyField { field });
    }
    Ok(())
}

/// Runtime config validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeConfigValidationError {
    /// Required string field was empty or whitespace-only.
    EmptyField {
        /// Field name.
        field: &'static str,
    },
    /// Host icon URL could not be parsed as an absolute URL.
    InvalidHostIcon {
        /// Parse failure reason.
        reason: String,
    },
    /// Host icon URL used a non-HTTPS scheme.
    InsecureHostIcon {
        /// Actual URL scheme.
        scheme: String,
    },
}

impl std::fmt::Display for RuntimeConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeConfigValidationError::EmptyField { field } => {
                write!(f, "{field} must not be empty")
            }
            RuntimeConfigValidationError::InvalidHostIcon { reason } => {
                write!(f, "host_icon must be an absolute HTTPS URL: {reason}")
            }
            RuntimeConfigValidationError::InsecureHostIcon { scheme } => {
                write!(f, "host_icon must use https scheme, got {scheme:?}")
            }
        }
    }
}

impl std::error::Error for RuntimeConfigValidationError {}

/// SSO wallet deeplink scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingDeeplinkScheme {
    /// Production Polkadot app.
    PolkadotApp,
    /// Development Polkadot app.
    PolkadotAppDev,
}

/// Scoped key-value storage. The platform namespaces keys so different products
/// cannot read each other's data.
pub trait Storage: Send + Sync {
    /// Read a value by key.
    fn read(
        &self,
        key: String,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, HostLocalStorageReadError>> + Send;

    /// Write a value to a key.
    fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> impl Future<Output = Result<(), HostLocalStorageReadError>> + Send;

    /// Clear a value at a key.
    fn clear(
        &self,
        key: String,
    ) -> impl Future<Output = Result<(), HostLocalStorageReadError>> + Send;
}

/// Open URLs in the system browser. Input is already trimmed, categorized,
/// and (where needed) normalized by the core; the host implementation only
/// needs to hand the URL to the OS URL handler.
pub trait Navigation: Send + Sync {
    /// Open the given URL in the system browser.
    fn navigate_to(
        &self,
        url: String,
    ) -> impl Future<Output = Result<(), HostNavigateToError>> + Send;
}

/// Deliver push notifications.
pub trait Notifications: Send + Sync {
    /// Schedule or immediately display the given notification and return the
    /// host-assigned id.
    fn push_notification(
        &self,
        notification: HostPushNotificationRequest,
    ) -> impl Future<Output = Result<HostPushNotificationResponse, GenericError>> + Send;

    /// Cancel a notification by id. Idempotent: cancelling an already-fired or
    /// unknown id still returns `Ok(())`.
    fn cancel_notification(
        &self,
        id: NotificationId,
    ) -> impl Future<Output = Result<(), GenericError>> + Send;
}

/// Permission prompts. v0.1 keeps device permissions (camera, mic, NFC, ...)
/// separate from remote permissions (domain access, chain submit, ...), so the
/// platform surface mirrors that split.
pub trait Permissions: Send + Sync {
    /// Prompt the user for a device-level permission.
    fn device_permission(
        &self,
        request: HostDevicePermissionRequest,
    ) -> impl Future<Output = Result<HostDevicePermissionResponse, GenericError>> + Send;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    fn remote_permission(
        &self,
        request: RemotePermissionRequest,
    ) -> impl Future<Output = Result<RemotePermissionResponse, GenericError>> + Send;
}

/// Feature-support probing. The host answers whether it can service a given
/// capability (currently scoped to per-chain support).
pub trait Features: Send + Sync {
    /// Report whether the requested feature is supported.
    fn feature_supported(
        &self,
        request: HostFeatureSupportedRequest,
    ) -> impl Future<Output = Result<HostFeatureSupportedResponse, GenericError>> + Send;
}

/// JSON-RPC provider factory for chain access.
///
/// The platform provides a way to get a JSON-RPC connection for a given chain.
/// The server runtime manages the chainHead v1 state machine on top of this.
pub trait ChainProvider: Send + Sync {
    /// Open a JSON-RPC connection for the chain identified by `genesis_hash`.
    /// Drop the returned connection to disconnect.
    fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> impl Future<Output = Result<Box<dyn JsonRpcConnection>, GenericError>> + Send;
}

/// A live JSON-RPC connection to a chain.
pub trait JsonRpcConnection: Send + Sync {
    /// Send a JSON-RPC request string.
    fn send(&self, request: String);

    /// Stream of JSON-RPC response strings.
    fn responses(&self) -> BoxStream<'static, String>;
}

/// Decoded session fields a host shell needs to render account UI without
/// parsing the opaque session blob the core persists through [`SessionStore`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionUiInfo {
    /// 32-byte sr25519 root public key of the active session.
    pub public_key: [u8; 32],
    /// Wallet identity account id used for People-chain username lookup.
    pub identity_account_id: Option<[u8; 32]>,
    /// Short username from the People-chain identity record.
    pub lite_username: Option<String>,
    /// Fully qualified username from the People-chain identity record.
    pub full_username: Option<String>,
}

/// Auth/session lifecycle state the core projects for host UI. The core owns
/// every transition and emits states in order; hosts render the current state
/// and never derive auth UI from any other signal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AuthState {
    /// No active session and no login in progress.
    #[default]
    Disconnected,
    /// A login is in progress: present the pairing deeplink/QR. Leave this
    /// state only on a subsequent emission (connected, failed, or
    /// disconnected after cancellation).
    Pairing {
        /// Wallet pairing deeplink to render as a QR code or open directly.
        deeplink: String,
    },
    /// A session is active.
    Connected(SessionUiInfo),
    /// The last login attempt failed; show the reason and offer a retry.
    LoginFailed {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Host auth UI driven by core-owned [`AuthState`] transitions.
pub trait AuthPresenter: Send + Sync {
    /// Observe an auth state change. Emitted only when the state actually
    /// changes, in transition order. Default is a no-op for hosts that
    /// render no auth UI.
    fn auth_state_changed(&self, state: AuthState) {
        let _ = state;
    }
}

/// Host-global opaque session persistence for core-owned SSO state.
pub trait SessionStore: Send + Sync {
    /// Read the currently persisted core session blob.
    fn read_session(&self) -> impl Future<Output = Result<Option<Vec<u8>>, GenericError>> + Send;

    /// Persist the core session blob.
    fn write_session(
        &self,
        value: Vec<u8>,
    ) -> impl Future<Output = Result<(), GenericError>> + Send;

    /// Clear the persisted core session blob.
    fn clear_session(&self) -> impl Future<Output = Result<(), GenericError>> + Send;

    /// Emit once immediately, then on future local/cross-runtime changes.
    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), GenericError>>;
}

/// Local user confirmation UI for session-channel operations.
pub trait UserConfirmation: Send + Sync {
    /// Confirm a sign-payload request before the core asks the SSO peer.
    fn confirm_sign_payload(
        &self,
        review: Vec<u8>,
    ) -> impl Future<Output = Result<bool, GenericError>> + Send;

    /// Confirm a sign-raw request before the core asks the SSO peer.
    fn confirm_sign_raw(
        &self,
        review: Vec<u8>,
    ) -> impl Future<Output = Result<bool, GenericError>> + Send;

    /// Confirm a create-transaction request before the core asks the SSO peer.
    fn confirm_create_transaction(
        &self,
        review: Vec<u8>,
    ) -> impl Future<Output = Result<bool, GenericError>> + Send;

    /// Confirm a cross-domain account-alias request before the core asks the
    /// SSO peer.
    fn confirm_account_alias(
        &self,
        review: Vec<u8>,
    ) -> impl Future<Output = Result<bool, GenericError>> + Send;

    /// Confirm resource allocation before the core asks the SSO peer.
    fn confirm_resource_allocation(
        &self,
        review: Vec<u8>,
    ) -> impl Future<Output = Result<bool, GenericError>> + Send;
}

/// Host theme source.
pub trait ThemeHost: Send + Sync {
    /// Emits current theme immediately, then future changes.
    fn subscribe_theme(&self) -> BoxStream<'static, Result<ThemeVariant, GenericError>>;
}

/// Host preimage backend. The core owns wire mapping and subscription
/// lifecycle; the host owns the selected backend.
pub trait PreimageHost: Send + Sync {
    /// Prompt before submitting a preimage.
    fn confirm_preimage_submit(
        &self,
        size: u64,
    ) -> impl Future<Output = Result<(), PreimageSubmitError>> + Send;

    /// Submit the preimage and return its key.
    fn submit_preimage(
        &self,
        value: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, PreimageSubmitError>> + Send;

    /// Emits current value/miss immediately, then future updates.
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, GenericError>>;
}

/// Combined platform interface. A host must provide all capability traits.
pub trait Platform:
    Navigation
    + Notifications
    + Permissions
    + Features
    + Storage
    + ChainProvider
    + AuthPresenter
    + SessionStore
    + UserConfirmation
    + ThemeHost
    + PreimageHost
{
}

impl<T> Platform for T where
    T: Navigation
        + Notifications
        + Permissions
        + Features
        + Storage
        + ChainProvider
        + AuthPresenter
        + SessionStore
        + UserConfirmation
        + ThemeHost
        + PreimageHost
{
}
