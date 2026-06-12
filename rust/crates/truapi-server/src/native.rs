//! UniFFI-facing native bridge. Exposes [`NativeTrUApiCore`] and the
//! [`HostCallbacks`] callback interface that iOS and Android call into.
//!
//! The native side builds a [`CallbackPlatform`] that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`TrUApiCore::from_platform_with_config`] so the rest
//! of the dispatcher pipeline behaves identically to the WS-bridge and wasm
//! flavors.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::task::SpawnExt;
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::PairingDeeplinkScheme as PlatformPairingDeeplinkScheme;
use truapi_platform::{
    AuthPresenter, ChainProvider, ChatHost, Features, JsonRpcConnection, Navigation, Notifications,
    PaymentHost, Permissions, PreimageHost, RuntimeConfig, RuntimeConfigValidationError,
    SessionStore, Storage, ThemeHost, UserConfirmation,
};

use crate::TrUApiCore;
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

/// Native-friendly payment request error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostPaymentRequestRejection {
    /// User rejected the payment request.
    #[error("payment rejected")]
    Rejected,
    /// User's available balance is not sufficient.
    #[error("insufficient balance")]
    InsufficientBalance,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl From<HostPaymentRequestRejection> for v01::HostPaymentError {
    fn from(err: HostPaymentRequestRejection) -> Self {
        match err {
            HostPaymentRequestRejection::Rejected => v01::HostPaymentError::Rejected,
            HostPaymentRequestRejection::InsufficientBalance => {
                v01::HostPaymentError::InsufficientBalance
            }
            HostPaymentRequestRejection::Unknown { reason } => {
                v01::HostPaymentError::Unknown { reason }
            }
        }
    }
}

/// Native-friendly payment balance subscription error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostPaymentBalanceSubscriptionRejection {
    /// User denied balance disclosure.
    #[error("permission denied")]
    PermissionDenied,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl From<HostPaymentBalanceSubscriptionRejection> for v01::HostPaymentBalanceSubscribeError {
    fn from(err: HostPaymentBalanceSubscriptionRejection) -> Self {
        match err {
            HostPaymentBalanceSubscriptionRejection::PermissionDenied => {
                v01::HostPaymentBalanceSubscribeError::PermissionDenied
            }
            HostPaymentBalanceSubscriptionRejection::Unknown { reason } => {
                v01::HostPaymentBalanceSubscribeError::Unknown { reason }
            }
        }
    }
}

/// Native-friendly payment lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativePaymentStatus {
    /// Payment is being processed.
    Processing,
    /// Payment has completed.
    Completed,
    /// Payment failed.
    Failed {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl From<v01::HostPaymentStatusSubscribeItem> for NativePaymentStatus {
    fn from(status: v01::HostPaymentStatusSubscribeItem) -> Self {
        match status {
            v01::HostPaymentStatusSubscribeItem::Processing => Self::Processing,
            v01::HostPaymentStatusSubscribeItem::Completed => Self::Completed,
            v01::HostPaymentStatusSubscribeItem::Failed { reason } => Self::Failed { reason },
        }
    }
}

impl From<NativePaymentStatus> for v01::HostPaymentStatusSubscribeItem {
    fn from(status: NativePaymentStatus) -> Self {
        match status {
            NativePaymentStatus::Processing => Self::Processing,
            NativePaymentStatus::Completed => Self::Completed,
            NativePaymentStatus::Failed { reason } => Self::Failed { reason },
        }
    }
}

/// Native-friendly payment status subscription error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostPaymentStatusSubscriptionRejection {
    /// Payment id was not found for the current product.
    #[error("payment not found")]
    PaymentNotFound,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl From<HostPaymentStatusSubscriptionRejection> for v01::HostPaymentStatusSubscribeError {
    fn from(err: HostPaymentStatusSubscriptionRejection) -> Self {
        match err {
            HostPaymentStatusSubscriptionRejection::PaymentNotFound => {
                v01::HostPaymentStatusSubscribeError::PaymentNotFound
            }
            HostPaymentStatusSubscriptionRejection::Unknown { reason } => {
                v01::HostPaymentStatusSubscribeError::Unknown { reason }
            }
        }
    }
}

/// Native-friendly payment top-up source.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativePaymentTopUpSource {
    /// Fund from one of the calling product's scoped accounts.
    ProductAccount {
        /// Product account derivation index.
        derivation_index: u32,
    },
    /// Fund from a one-time sr25519 account.
    PrivateKey {
        /// Sr25519 secret key bytes.
        sr25519_secret_key: Vec<u8>,
    },
    /// Fund directly from coin secret keys.
    Coins {
        /// Sr25519 secret keys, one per coin.
        sr25519_secret_keys: Vec<Vec<u8>>,
    },
}

impl From<v01::PaymentTopUpSource> for NativePaymentTopUpSource {
    fn from(source: v01::PaymentTopUpSource) -> Self {
        match source {
            v01::PaymentTopUpSource::ProductAccount { derivation_index } => {
                Self::ProductAccount { derivation_index }
            }
            v01::PaymentTopUpSource::PrivateKey { sr25519_secret_key } => Self::PrivateKey {
                sr25519_secret_key: sr25519_secret_key.to_vec(),
            },
            v01::PaymentTopUpSource::Coins {
                sr25519_secret_keys,
            } => Self::Coins {
                sr25519_secret_keys: sr25519_secret_keys
                    .into_iter()
                    .map(|key| key.to_vec())
                    .collect(),
            },
        }
    }
}

/// Native-friendly payment top-up error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostPaymentTopUpRejection {
    /// The source account does not hold sufficient funds.
    #[error("insufficient funds")]
    InsufficientFunds,
    /// The source account was not found or is invalid.
    #[error("invalid source")]
    InvalidSource,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
    /// Some coins were claimed but the total fell short of the requested amount.
    #[error("partial payment credited {credited}")]
    PartialPayment {
        /// Amount that was successfully credited, encoded as a decimal string.
        credited: String,
    },
}

impl From<HostPaymentTopUpRejection> for v01::HostPaymentTopUpError {
    fn from(err: HostPaymentTopUpRejection) -> Self {
        match err {
            HostPaymentTopUpRejection::InsufficientFunds => {
                v01::HostPaymentTopUpError::InsufficientFunds
            }
            HostPaymentTopUpRejection::InvalidSource => v01::HostPaymentTopUpError::InvalidSource,
            HostPaymentTopUpRejection::Unknown { reason } => {
                v01::HostPaymentTopUpError::Unknown { reason }
            }
            HostPaymentTopUpRejection::PartialPayment { credited } => credited
                .parse()
                .map(|credited| v01::HostPaymentTopUpError::PartialPayment { credited })
                .unwrap_or_else(|err| v01::HostPaymentTopUpError::Unknown {
                    reason: format!("invalid partial payment amount {credited:?}: {err}"),
                }),
        }
    }
}

/// Native-friendly chat room creation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HostChatRoomRegistrationStatus {
    /// The host created a new room.
    New,
    /// The requested room already exists.
    Exists,
}

impl From<HostChatRoomRegistrationStatus> for v01::ChatRoomRegistrationStatus {
    fn from(status: HostChatRoomRegistrationStatus) -> Self {
        match status {
            HostChatRoomRegistrationStatus::New => v01::ChatRoomRegistrationStatus::New,
            HostChatRoomRegistrationStatus::Exists => v01::ChatRoomRegistrationStatus::Exists,
        }
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
    /// Whether a session is currently active. When `false` every other
    /// field is `None`.
    pub connected: bool,
    /// 32-byte sr25519 root public key of the active session.
    pub public_key: Option<Vec<u8>>,
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
            connected: info.connected,
            public_key: info.public_key.map(|key| key.to_vec()),
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

impl From<NativePairingDeeplinkScheme> for PlatformPairingDeeplinkScheme {
    fn from(scheme: NativePairingDeeplinkScheme) -> Self {
        match scheme {
            NativePairingDeeplinkScheme::PolkadotApp => PlatformPairingDeeplinkScheme::PolkadotApp,
            NativePairingDeeplinkScheme::PolkadotAppDev => {
                PlatformPairingDeeplinkScheme::PolkadotAppDev
            }
        }
    }
}

/// Native runtime configuration supplied before product calls are handled.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeRuntimeConfig {
    /// Human-readable dotli label, e.g. `my-app`.
    pub product_label: String,
    /// Canonical product identifier used for account derivation.
    pub product_id: String,
    /// Host deployment/site identifier.
    pub site_id: String,
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
    /// Deeplink scheme used in pairing QR payloads.
    pub pairing_deeplink_scheme: NativePairingDeeplinkScheme,
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
}

impl TryFrom<NativeRuntimeConfig> for RuntimeConfig {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: NativeRuntimeConfig) -> Result<Self, Self::Error> {
        let people_chain_genesis_hash =
            <[u8; 32]>::try_from(config.people_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidPeopleChainGenesisHash {
                    actual: config.people_chain_genesis_hash.len() as u64,
                }
            })?;
        let runtime_config = Self {
            product_label: config.product_label,
            product_id: config.product_id,
            site_id: config.site_id,
            host_name: config.host_name,
            host_icon: config.host_icon,
            host_version: config.host_version,
            platform_type: config.platform_type,
            platform_version: config.platform_version,
            people_chain_genesis_hash,
            pairing_deeplink_scheme: config.pairing_deeplink_scheme.into(),
        };
        runtime_config.validate()?;
        Ok(runtime_config)
    }
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
    fn from(err: RuntimeConfigValidationError) -> Self {
        match err {
            RuntimeConfigValidationError::EmptyField { field } => Self::EmptyField {
                field: field.to_string(),
            },
            RuntimeConfigValidationError::InvalidHostIcon { reason } => {
                Self::InvalidHostIcon { reason }
            }
            RuntimeConfigValidationError::InsecureHostIcon { scheme } => {
                Self::InsecureHostIcon { scheme }
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

/// Callback surface that iOS and Android implement.
///
/// Threading contract: every callback is invoked on a background thread
/// owned by the Rust core, never the host's main/UI thread. UI-decision
/// callbacks (`navigate_to`, `device_permission`, `remote_permission`,
/// `payment_balance_subscribe`, `payment_request`, `payment_top_up`,
/// `payment_status_subscribe`, the `confirm_*` family) plus the potentially
/// slow `submit_preimage` run on the tokio blocking pool, so an implementation
/// may block its calling thread until the user decides without stalling
/// concurrent dispatches.
/// All other callbacks run inline on the dispatcher thread and must return
/// promptly; in particular `auth_state_changed` should only hand the state
/// to the host UI thread, never wait for the user. As the one exception to
/// the background-thread rule, `auth_state_changed` can also arrive
/// synchronously on whichever thread calls `NativeTrUApiCore::cancel_login`.
#[uniffi::export(callback_interface)]
pub trait HostCallbacks: Send + Sync {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    fn on_core_log(&self, marker: String, detail: String);

    /// Open a URL in the system browser.
    fn navigate_to(&self, url: String) -> Result<(), HostNavigateRejection>;

    /// Deliver a push notification and return the host-assigned id.
    fn push_notification(
        &self,
        text: String,
        deeplink: Option<String>,
        scheduled_at_ms: Option<u64>,
    ) -> Result<u32, HostRejection>;

    /// Cancel a notification by id.
    fn cancel_notification(&self, id: u32) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...).
    fn device_permission(&self, capability: String) -> Result<bool, HostRejection>;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    fn remote_permission(
        &self,
        permission: String,
        domains: Vec<String>,
    ) -> Result<bool, HostRejection>;

    /// Observe an auth state change. Emitted only when the state actually
    /// changes, in transition order: render `Pairing` as the pairing QR UI,
    /// `Connected`/`Disconnected` as the account badge, `LoginFailed` as a
    /// retryable error. User cancellation is reported through
    /// `NativeTrUApiCore.cancel_login()`.
    fn auth_state_changed(&self, state: AuthState);

    /// Read the opaque core-owned SSO session blob from host-global storage.
    fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Persist the opaque core-owned SSO session blob in host-global storage.
    fn write_session(&self, value: Vec<u8>) -> Result<(), HostRejection>;

    /// Clear the persisted core-owned SSO session blob.
    fn clear_session(&self) -> Result<(), HostRejection>;

    /// Open a JSON-RPC connection for a chain. Return a host-assigned
    /// connection id, or `None` when unsupported.
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection>;

    /// Send one JSON-RPC request over a previously opened chain connection.
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection>;

    /// Close a previously opened chain connection.
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection>;

    /// Confirm a sign-payload request. `review` is a SCALE-encoded review
    /// payload owned by the Rust core.
    fn confirm_sign_payload(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a sign-raw request. `review` is a SCALE-encoded review payload
    /// owned by the Rust core.
    fn confirm_sign_raw(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a create-transaction request. `review` is a SCALE-encoded
    /// review payload owned by the Rust core.
    fn confirm_create_transaction(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a cross-domain account-alias request. `review` is a
    /// SCALE-encoded review payload owned by the Rust core.
    fn confirm_account_alias(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a resource-allocation request. `review` is a SCALE-encoded
    /// review payload owned by the Rust core.
    fn confirm_resource_allocation(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm preimage submission before the host stores it.
    fn confirm_preimage_submit(&self, size: u64) -> Result<(), HostRejection>;

    /// Submit the preimage through the host backend and return its key.
    fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection>;

    /// Look up one preimage value by key. The native shim emits this as the
    /// current item in its subscription stream.
    fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Current host theme. The native shim emits this as the current item in
    /// its subscription stream.
    fn current_theme(&self) -> Result<HostTheme, HostRejection>;

    /// Answer whether the host supports the chain identified by genesis hash.
    fn feature_supported_chain(&self, genesis_hash: Vec<u8>) -> Result<bool, HostRejection>;

    /// Create or join a native chat room and return its registration status.
    fn chat_create_room(
        &self,
        room_id: String,
        name: String,
        icon: String,
    ) -> Result<HostChatRoomRegistrationStatus, HostRejection>;

    /// Post a text message into the native chat system and return the
    /// host-assigned message id.
    fn chat_post_text_message(
        &self,
        room_id: String,
        text: String,
    ) -> Result<String, HostRejection>;

    /// Post a custom message into the native chat system and return the
    /// host-assigned message id.
    fn chat_post_custom_message(
        &self,
        room_id: String,
        message_type: String,
        payload: Vec<u8>,
    ) -> Result<String, HostRejection>;

    /// Start forwarding payment balance updates and return the first balance.
    fn payment_balance_subscribe(&self) -> Result<String, HostPaymentBalanceSubscriptionRejection>;

    /// Initiate a native payment and return the host-assigned payment id.
    fn payment_request(
        &self,
        amount_in_planks: String,
        destination: Vec<u8>,
    ) -> Result<String, HostPaymentRequestRejection>;

    /// Top up the product payment balance.
    fn payment_top_up(
        &self,
        amount_in_planks: String,
        source: NativePaymentTopUpSource,
    ) -> Result<(), HostPaymentTopUpRejection>;

    /// Start forwarding updates for a payment and return its current status.
    fn payment_status_subscribe(
        &self,
        payment_id: String,
    ) -> Result<NativePaymentStatus, HostPaymentStatusSubscriptionRejection>;

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
    core: Arc<TrUApiCore>,
    events: Arc<NativeEventBus>,
    #[cfg(feature = "ws-bridge")]
    callbacks: Arc<dyn HostCallbacks>,
    #[cfg(feature = "ws-bridge")]
    bridge: std::sync::Mutex<Option<WsBridge>>,
}

#[uniffi::export]
impl NativeTrUApiCore {
    /// Construct the core with explicit product and pairing runtime config.
    #[uniffi::constructor]
    pub fn with_runtime_config(
        callbacks: Box<dyn HostCallbacks>,
        runtime_config: NativeRuntimeConfig,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        Ok(native_core_from_platform_config(
            callbacks,
            runtime_config.try_into()?,
        ))
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    pub fn disconnect(&self) {
        self.core.disconnect();
    }

    /// Notify this core that host-global session storage changed outside a
    /// direct core write/clear. Native hosts call this after cross-process or
    /// platform storage notifications so the core re-reads `SessionStore`.
    pub fn notify_session_store_changed(&self) {
        self.events.notify_session_store_changed();
    }

    /// Cancel any in-flight `request_login` pairing (e.g. the user dismissed
    /// the pairing UI). The host receives a `Disconnected` auth state
    /// immediately and the pending login resolves to `Rejected`. A no-op
    /// when no login is in progress.
    pub fn cancel_login(&self) {
        self.core.cancel_login();
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

    /// Push a native payment balance update into active `payment.balanceSubscribe()` streams.
    pub fn notify_payment_balance_changed(&self, amount_in_planks: String) {
        if let Ok(balance) = amount_in_planks.parse() {
            self.events.notify_payment_balance_changed(balance);
        }
    }

    /// Push a native payment status update into active `payment.statusSubscribe()` streams.
    pub fn notify_payment_status_changed(&self, payment_id: String, status: NativePaymentStatus) {
        self.events
            .notify_payment_status_changed(&payment_id, status.into());
    }

    /// Publish a host-originated plain-text chat message to product
    /// `chat.actionSubscribe()` streams.
    pub fn notify_chat_message_posted(&self, room_id: String, peer: String, text: String) {
        self.core.notify_chat_message_posted(room_id, peer, text);
    }

    /// Publish a host-originated custom chat message to product
    /// `chat.actionSubscribe()` streams.
    pub fn notify_chat_custom_message_posted(
        &self,
        room_id: String,
        peer: String,
        message_type: String,
        payload: Vec<u8>,
    ) {
        self.core
            .notify_chat_custom_message_posted(room_id, peer, message_type, payload);
    }

    /// Publish a host-originated chat action trigger to product
    /// `chat.actionSubscribe()` streams.
    pub fn notify_chat_action_triggered(
        &self,
        room_id: String,
        peer: String,
        message_id: String,
        action_id: String,
        payload: Option<Vec<u8>>,
    ) {
        self.core
            .notify_chat_action_triggered(room_id, peer, message_id, action_id, payload);
    }
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
    callbacks: Box<dyn HostCallbacks>,
    runtime_config: RuntimeConfig,
) -> Arc<NativeTrUApiCore> {
    crate::logging::init();
    let callbacks: Arc<dyn HostCallbacks> = callbacks.into();
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
    Arc::new(NativeTrUApiCore {
        core: Arc::new(TrUApiCore::from_platform_with_config(
            platform,
            runtime_config,
            spawner,
        )),
        events,
        #[cfg(feature = "ws-bridge")]
        callbacks,
        #[cfg(feature = "ws-bridge")]
        bridge: std::sync::Mutex::new(None),
    })
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
        let (bridge, endpoint) = WsBridge::start(bind_port, self.core.clone(), logger)?;
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

/// Run a host callback that may block awaiting a user decision.
///
/// UI-decision callbacks are allowed to block their calling thread until the
/// user decides. Running them inline would stall the single-threaded
/// WS-bridge dispatcher (and deadlock if the decision UI itself issues a
/// TrUAPI call), so inside a tokio runtime the callback is moved to the
/// blocking pool. Outside a tokio context the callback runs inline.
async fn run_blocking_callback<T, F>(callback: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    #[cfg(feature = "ws-bridge")]
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return handle
            .spawn_blocking(callback)
            .await
            .expect("blocking host callback panicked");
    }
    callback()
}

#[derive(Default)]
struct NativeEventBus {
    session_store_ticks: Mutex<Vec<mpsc::UnboundedSender<Result<(), v01::GenericError>>>>,
    theme_changes: Mutex<Vec<mpsc::UnboundedSender<Result<v01::ThemeVariant, v01::GenericError>>>>,
    preimage_changes: Mutex<Vec<PreimageSubscription>>,
    chain_responses: Mutex<HashMap<u32, mpsc::UnboundedSender<String>>>,
    payment_balances: Mutex<Vec<mpsc::UnboundedSender<v01::Balance>>>,
    payment_statuses:
        Mutex<HashMap<String, Vec<mpsc::UnboundedSender<v01::HostPaymentStatusSubscribeItem>>>>,
}

struct PreimageSubscription {
    key: Vec<u8>,
    tx: mpsc::UnboundedSender<Result<Option<Vec<u8>>, v01::GenericError>>,
}

impl NativeEventBus {
    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.session_store_ticks
            .lock()
            .expect("native session store subscribers mutex poisoned")
            .push(tx);
        stream::once(async { Ok(()) }).chain(rx).boxed()
    }

    fn notify_session_store_changed(&self) {
        self.session_store_ticks
            .lock()
            .expect("native session store subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(())).is_ok());
    }

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

    fn subscribe_preimage(
        &self,
        key: Vec<u8>,
        current: Result<Option<Vec<u8>>, v01::GenericError>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .push(PreimageSubscription { key, tx });
        stream::once(async move { current }).chain(rx).boxed()
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

    fn subscribe_payment_balance(&self, current: v01::Balance) -> BoxStream<'static, v01::Balance> {
        let (tx, rx) = mpsc::unbounded();
        self.payment_balances
            .lock()
            .expect("native payment balance subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_payment_balance_changed(&self, balance: v01::Balance) {
        self.payment_balances
            .lock()
            .expect("native payment balance subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(balance).is_ok());
    }

    fn subscribe_payment_status(
        &self,
        payment_id: String,
        current: v01::HostPaymentStatusSubscribeItem,
    ) -> BoxStream<'static, v01::HostPaymentStatusSubscribeItem> {
        let (tx, rx) = mpsc::unbounded();
        self.payment_statuses
            .lock()
            .expect("native payment status subscribers mutex poisoned")
            .entry(payment_id)
            .or_default()
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_payment_status_changed(
        &self,
        payment_id: &str,
        status: v01::HostPaymentStatusSubscribeItem,
    ) {
        let mut subscriptions = self
            .payment_statuses
            .lock()
            .expect("native payment status subscribers mutex poisoned");
        let Some(senders) = subscriptions.get_mut(payment_id) else {
            return;
        };
        senders.retain(|tx| tx.unbounded_send(status.clone()).is_ok());
        if senders.is_empty() {
            subscriptions.remove(payment_id);
        }
    }
}

impl Navigation for CallbackPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.navigate_to".to_string(),
            url.clone(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.navigate_to(url))
            .await
            .map_err(Into::into)
    }
}

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
            .push_notification(
                notification.text,
                notification.deeplink,
                notification.scheduled_at,
            )
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

fn remote_permission_name(permission: &v01::RemotePermission) -> &'static str {
    match permission {
        v01::RemotePermission::Remote { .. } => "Remote",
        v01::RemotePermission::WebRtc => "WebRTC",
        v01::RemotePermission::ChainSubmit => "ChainSubmit",
        v01::RemotePermission::PreimageSubmit => "PreimageSubmit",
        v01::RemotePermission::StatementSubmit => "StatementSubmit",
        v01::RemotePermission::UserId => "UserId",
    }
}

fn remote_permission_domains(permission: &v01::RemotePermission) -> Vec<String> {
    match permission {
        v01::RemotePermission::Remote { domains } => domains.clone(),
        v01::RemotePermission::WebRtc
        | v01::RemotePermission::ChainSubmit
        | v01::RemotePermission::PreimageSubmit
        | v01::RemotePermission::StatementSubmit
        | v01::RemotePermission::UserId => Vec::new(),
    }
}

impl Permissions for CallbackPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission".to_string(),
            format!("{request}"),
        );
        let callbacks = self.callbacks.clone();
        let capability = format!("{request}");
        let granted = run_blocking_callback(move || callbacks.device_permission(capability))
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
        let callbacks = self.callbacks.clone();
        let permission = remote_permission_name(&request.permission).to_string();
        let domains = remote_permission_domains(&request.permission);
        let granted =
            run_blocking_callback(move || callbacks.remote_permission(permission, domains))
                .await
                .map_err(v01::GenericError::from)?;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

impl Features for CallbackPlatform {
    async fn feature_supported(
        &self,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
        let HostFeatureSupportedRequest::V1(request) = request;
        let supported = match request {
            v01::HostFeatureSupportedRequest::Chain { genesis_hash } => self
                .callbacks
                .feature_supported_chain(genesis_hash)
                .map_err(v01::GenericError::from)?,
        };
        Ok(HostFeatureSupportedResponse::V1(
            v01::HostFeatureSupportedResponse { supported },
        ))
    }
}

impl Storage for CallbackPlatform {
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

impl ChatHost for CallbackPlatform {
    async fn create_chat_room(
        &self,
        room_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatRoomRegistrationStatus, v01::HostChatCreateRoomError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.chat_create_room".to_string(),
            room_id.clone(),
        );

        self.callbacks
            .chat_create_room(room_id, name, icon)
            .map(Into::into)
            .map_err(chat_create_room_rejection_to_error)
    }

    async fn post_chat_message(
        &self,
        room_id: String,
        payload: v01::ChatMessageContent,
    ) -> Result<String, v01::HostChatPostMessageError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.chat_post_message".to_string(),
            room_id.clone(),
        );

        match payload {
            v01::ChatMessageContent::Text { text } => self
                .callbacks
                .chat_post_text_message(room_id, text)
                .map_err(chat_post_rejection_to_error),
            v01::ChatMessageContent::Custom(custom) => self
                .callbacks
                .chat_post_custom_message(room_id, custom.message_type, custom.payload)
                .map_err(chat_post_rejection_to_error),
            _ => Err(v01::HostChatPostMessageError::Unknown {
                reason: "chat message type is not available through the native TrUAPI bridge yet"
                    .to_string(),
            }),
        }
    }
}

impl PaymentHost for CallbackPlatform {
    async fn subscribe_payment_balance(
        &self,
    ) -> Result<BoxStream<'static, v01::Balance>, v01::HostPaymentBalanceSubscribeError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.payment_balance_subscribe".to_string(),
            String::new(),
        );
        let callbacks = self.callbacks.clone();
        let balance: v01::Balance =
            run_blocking_callback(move || callbacks.payment_balance_subscribe())
                .await
                .map_err(v01::HostPaymentBalanceSubscribeError::from)?
                .parse()
                .map_err(|err| v01::HostPaymentBalanceSubscribeError::Unknown {
                    reason: format!("invalid payment balance amount: {err}"),
                })?;
        Ok(self.events.subscribe_payment_balance(balance))
    }

    async fn request_payment(
        &self,
        amount: v01::Balance,
        destination: [u8; 32],
    ) -> Result<String, v01::HostPaymentError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.payment_request".to_string(),
            amount.to_string(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || {
            callbacks.payment_request(amount.to_string(), destination.to_vec())
        })
        .await
        .map_err(Into::into)
    }

    async fn top_up_payment(
        &self,
        amount: v01::Balance,
        source: v01::PaymentTopUpSource,
    ) -> Result<(), v01::HostPaymentTopUpError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.payment_top_up".to_string(),
            amount.to_string(),
        );
        let callbacks = self.callbacks.clone();
        let source = NativePaymentTopUpSource::from(source);
        run_blocking_callback(move || callbacks.payment_top_up(amount.to_string(), source))
            .await
            .map_err(Into::into)
    }

    async fn subscribe_payment_status(
        &self,
        payment_id: String,
    ) -> Result<
        BoxStream<'static, v01::HostPaymentStatusSubscribeItem>,
        v01::HostPaymentStatusSubscribeError,
    > {
        self.callbacks.on_core_log(
            "truapi.native.callback.payment_status_subscribe".to_string(),
            payment_id.clone(),
        );
        let callbacks = self.callbacks.clone();
        let current = run_blocking_callback({
            let payment_id = payment_id.clone();
            move || callbacks.payment_status_subscribe(payment_id)
        })
        .await
        .map_err(v01::HostPaymentStatusSubscribeError::from)?;
        Ok(self
            .events
            .subscribe_payment_status(payment_id, current.into()))
    }
}

fn chat_create_room_rejection_to_error(err: HostRejection) -> v01::HostChatCreateRoomError {
    v01::HostChatCreateRoomError::Unknown {
        reason: err.to_string(),
    }
}

fn chat_post_rejection_to_error(err: HostRejection) -> v01::HostChatPostMessageError {
    v01::HostChatPostMessageError::Unknown {
        reason: err.to_string(),
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
}

impl Drop for NativeJsonRpcConnection {
    fn drop(&mut self) {
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

impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let Some(connection_id) = self
            .callbacks
            .chain_connect(genesis_hash)
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

impl SessionStore for CallbackPlatform {
    async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.callbacks
            .read_session()
            .map_err(v01::GenericError::from)
    }

    async fn write_session(&self, value: Vec<u8>) -> Result<(), v01::GenericError> {
        self.callbacks
            .write_session(value)
            .map_err(v01::GenericError::from)?;
        self.events.notify_session_store_changed();
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), v01::GenericError> {
        self.callbacks
            .clear_session()
            .map_err(v01::GenericError::from)?;
        self.events.notify_session_store_changed();
        Ok(())
    }

    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        self.events.subscribe_session_store()
    }
}

impl UserConfirmation for CallbackPlatform {
    async fn confirm_sign_payload(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_sign_payload".to_string(),
            String::new(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.confirm_sign_payload(review))
            .await
            .map_err(v01::GenericError::from)
    }

    async fn confirm_sign_raw(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_sign_raw".to_string(),
            String::new(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.confirm_sign_raw(review))
            .await
            .map_err(v01::GenericError::from)
    }

    async fn confirm_create_transaction(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_create_transaction".to_string(),
            String::new(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.confirm_create_transaction(review))
            .await
            .map_err(v01::GenericError::from)
    }

    async fn confirm_account_alias(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_account_alias".to_string(),
            String::new(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.confirm_account_alias(review))
            .await
            .map_err(v01::GenericError::from)
    }

    async fn confirm_resource_allocation(
        &self,
        review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_resource_allocation".to_string(),
            String::new(),
        );
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.confirm_resource_allocation(review))
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
    async fn confirm_preimage_submit(&self, size: u64) -> Result<(), v01::PreimageSubmitError> {
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.confirm_preimage_submit(size))
            .await
            .map_err(|err| v01::PreimageSubmitError::Unknown {
                reason: err.to_string(),
            })
    }

    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        let callbacks = self.callbacks.clone();
        run_blocking_callback(move || callbacks.submit_preimage(value))
            .await
            .map_err(|err| v01::PreimageSubmitError::Unknown {
                reason: err.to_string(),
            })
    }

    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let current = self
            .callbacks
            .lookup_preimage(key.clone())
            .map_err(v01::GenericError::from);
        self.events.subscribe_preimage(key, current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type PreimageFixtureEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;
    type PushNotificationFixtureEntries = Vec<(String, Option<String>, Option<u64>)>;

    struct EventCallbacks {
        theme: Mutex<HostTheme>,
        preimages: Mutex<PreimageFixtureEntries>,
        auth_states: Mutex<Vec<AuthState>>,
        chain_id: Mutex<Option<u32>>,
        chain_connects: Mutex<Vec<Vec<u8>>>,
        chain_sends: Mutex<Vec<(u32, String)>>,
        chain_closes: Mutex<Vec<u32>>,
        push_notifications: Mutex<PushNotificationFixtureEntries>,
        device_permissions: Mutex<Vec<String>>,
        remote_permissions: Mutex<Vec<(String, Vec<String>)>>,
        feature_supported_chains: Mutex<Vec<Vec<u8>>>,
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
                push_notifications: Mutex::new(Vec::new()),
                device_permissions: Mutex::new(Vec::new()),
                remote_permissions: Mutex::new(Vec::new()),
                feature_supported_chains: Mutex::new(Vec::new()),
            }
        }
    }

    impl HostCallbacks for EventCallbacks {
        fn on_core_log(&self, _marker: String, _detail: String) {}
        fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
            Ok(())
        }
        fn push_notification(
            &self,
            text: String,
            deeplink: Option<String>,
            scheduled_at_ms: Option<u64>,
        ) -> Result<u32, HostRejection> {
            self.push_notifications
                .lock()
                .expect("push notifications mutex poisoned")
                .push((text, deeplink, scheduled_at_ms));
            Ok(7)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        fn device_permission(&self, capability: String) -> Result<bool, HostRejection> {
            self.device_permissions
                .lock()
                .expect("device permissions mutex poisoned")
                .push(capability);
            Ok(true)
        }
        fn remote_permission(
            &self,
            permission: String,
            domains: Vec<String>,
        ) -> Result<bool, HostRejection> {
            self.remote_permissions
                .lock()
                .expect("remote permissions mutex poisoned")
                .push((permission, domains));
            Ok(true)
        }
        fn auth_state_changed(&self, state: AuthState) {
            self.auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .push(state);
        }
        fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn write_session(&self, _value: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn clear_session(&self) -> Result<(), HostRejection> {
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
        fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_create_transaction(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_resource_allocation(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_preimage_submit(&self, _size: u64) -> Result<(), HostRejection> {
            Ok(())
        }
        fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection> {
            Ok(value)
        }
        fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
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
        fn feature_supported_chain(&self, genesis_hash: Vec<u8>) -> Result<bool, HostRejection> {
            self.feature_supported_chains
                .lock()
                .expect("feature chains mutex poisoned")
                .push(genesis_hash);
            Ok(true)
        }
        fn chat_create_room(
            &self,
            _room_id: String,
            _name: String,
            _icon: String,
        ) -> Result<HostChatRoomRegistrationStatus, HostRejection> {
            Ok(HostChatRoomRegistrationStatus::New)
        }
        fn chat_post_text_message(
            &self,
            _room_id: String,
            _text: String,
        ) -> Result<String, HostRejection> {
            Ok("message-1".to_string())
        }
        fn chat_post_custom_message(
            &self,
            _room_id: String,
            _message_type: String,
            _payload: Vec<u8>,
        ) -> Result<String, HostRejection> {
            Ok("message-1".to_string())
        }
        fn payment_balance_subscribe(
            &self,
        ) -> Result<String, HostPaymentBalanceSubscriptionRejection> {
            Ok("0".to_string())
        }
        fn payment_request(
            &self,
            _amount_in_planks: String,
            _destination: Vec<u8>,
        ) -> Result<String, HostPaymentRequestRejection> {
            Ok("payment-1".to_string())
        }
        fn payment_top_up(
            &self,
            _amount_in_planks: String,
            _source: NativePaymentTopUpSource,
        ) -> Result<(), HostPaymentTopUpRejection> {
            Ok(())
        }
        fn payment_status_subscribe(
            &self,
            _payment_id: String,
        ) -> Result<NativePaymentStatus, HostPaymentStatusSubscriptionRejection> {
            Ok(NativePaymentStatus::Completed)
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

    #[test]
    fn native_auth_presenter_forwards_states_across_the_ffi_mirror() {
        let (callbacks, _events, platform) = event_platform();

        platform.auth_state_changed(truapi_platform::AuthState::Pairing {
            deeplink: "polkadotapp://pair?handshake=00".to_string(),
        });
        platform.auth_state_changed(truapi_platform::AuthState::Connected(
            truapi_platform::SessionUiInfo {
                connected: true,
                public_key: Some([7; 32]),
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
                        connected: true,
                        public_key: Some(vec![7; 32]),
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
    fn native_session_store_subscription_emits_current_then_notified_ticks() {
        let (_callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_session_store();

        let first = futures::executor::block_on(stream.next()).unwrap();
        events.notify_session_store_changed();
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert!(first.is_ok());
        assert!(second.is_ok());
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
        let genesis = vec![9; 32];

        let connection =
            futures::executor::block_on(ChainProvider::connect(&platform, genesis.clone()))
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
            &[genesis]
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
    fn native_callbacks_receive_typed_protocol_values() {
        let (callbacks, _events, platform) = event_platform();
        let push_response = futures::executor::block_on(Notifications::push_notification(
            &platform,
            v01::HostPushNotificationRequest {
                text: "hello".to_string(),
                deeplink: Some("polkadot://chat".to_string()),
                scheduled_at: Some(42),
            },
        ))
        .expect("push notification callback succeeds");
        let device_response = futures::executor::block_on(Permissions::device_permission(
            &platform,
            v01::HostDevicePermissionRequest::Camera,
        ))
        .expect("device permission callback succeeds");
        let remote_response = futures::executor::block_on(Permissions::remote_permission(
            &platform,
            v01::RemotePermissionRequest {
                permission: v01::RemotePermission::Remote {
                    domains: vec!["example.com".to_string()],
                },
            },
        ))
        .expect("remote permission callback succeeds");
        let genesis_hash = vec![9; 32];
        let feature_response = futures::executor::block_on(Features::feature_supported(
            &platform,
            HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
                genesis_hash: genesis_hash.clone(),
            }),
        ))
        .expect("feature support callback succeeds");

        assert_eq!(push_response.id, 7);
        assert!(device_response.granted);
        assert!(remote_response.granted);
        assert_eq!(
            feature_response,
            HostFeatureSupportedResponse::V1(v01::HostFeatureSupportedResponse { supported: true })
        );
        assert_eq!(
            callbacks
                .push_notifications
                .lock()
                .expect("push notifications mutex poisoned")
                .as_slice(),
            &[(
                "hello".to_string(),
                Some("polkadot://chat".to_string()),
                Some(42)
            )]
        );
        assert_eq!(
            callbacks
                .device_permissions
                .lock()
                .expect("device permissions mutex poisoned")
                .as_slice(),
            &["camera".to_string()]
        );
        assert_eq!(
            callbacks
                .remote_permissions
                .lock()
                .expect("remote permissions mutex poisoned")
                .as_slice(),
            &[("Remote".to_string(), vec!["example.com".to_string()])]
        );
        assert_eq!(
            callbacks
                .feature_supported_chains
                .lock()
                .expect("feature chains mutex poisoned")
                .as_slice(),
            &[genesis_hash]
        );
    }

    #[test]
    fn runtime_config_rejects_wrong_size_genesis_hash() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: "app.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://example.invalid/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0; 31],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidPeopleChainGenesisHash { actual: 31 }
        ));
    }

    #[test]
    fn runtime_config_rejects_empty_required_fields() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: " ".to_string(),
            site_id: "dot.li".to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://example.invalid/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0; 32],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::EmptyField { field } if field == "product_id"
        ));
    }

    #[test]
    fn runtime_config_rejects_relative_host_icon() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: "app.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0; 32],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidHostIcon { .. }
        ));
    }

    #[test]
    fn runtime_config_rejects_non_https_host_icon() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: "app.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("http://localhost:3000/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0; 32],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
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
        impl HostCallbacks for Noop {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            fn push_notification(
                &self,
                _text: String,
                _deeplink: Option<String>,
                _scheduled_at_ms: Option<u64>,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            fn device_permission(&self, _capability: String) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn remote_permission(
                &self,
                _permission: String,
                _domains: Vec<String>,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn write_session(&self, _value: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn clear_session(&self) -> Result<(), HostRejection> {
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
            fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_create_transaction(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_resource_allocation(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_preimage_submit(&self, _size: u64) -> Result<(), HostRejection> {
                Ok(())
            }
            fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection> {
                Ok(value)
            }
            fn lookup_preimage(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<HostTheme, HostRejection> {
                Ok(HostTheme::Light)
            }
            fn feature_supported_chain(
                &self,
                _genesis_hash: Vec<u8>,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn chat_create_room(
                &self,
                _room_id: String,
                _name: String,
                _icon: String,
            ) -> Result<HostChatRoomRegistrationStatus, HostRejection> {
                Ok(HostChatRoomRegistrationStatus::New)
            }
            fn chat_post_text_message(
                &self,
                _room_id: String,
                _text: String,
            ) -> Result<String, HostRejection> {
                Ok("message-1".to_string())
            }
            fn chat_post_custom_message(
                &self,
                _room_id: String,
                _message_type: String,
                _payload: Vec<u8>,
            ) -> Result<String, HostRejection> {
                Ok("message-1".to_string())
            }
            fn payment_balance_subscribe(
                &self,
            ) -> Result<String, HostPaymentBalanceSubscriptionRejection> {
                Ok("0".to_string())
            }
            fn payment_request(
                &self,
                _amount_in_planks: String,
                _destination: Vec<u8>,
            ) -> Result<String, HostPaymentRequestRejection> {
                Ok("payment-1".to_string())
            }
            fn payment_top_up(
                &self,
                _amount_in_planks: String,
                _source: NativePaymentTopUpSource,
            ) -> Result<(), HostPaymentTopUpRejection> {
                Ok(())
            }
            fn payment_status_subscribe(
                &self,
                _payment_id: String,
            ) -> Result<NativePaymentStatus, HostPaymentStatusSubscriptionRejection> {
                Ok(NativePaymentStatus::Completed)
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
            Box::new(Noop),
            NativeRuntimeConfig {
                product_label: "dotli".to_string(),
                product_id: "dotli.dot".to_string(),
                site_id: "dot.li".to_string(),
                host_name: "Polkadot Web".to_string(),
                host_icon: Some("https://dot.li/dotli.png".to_string()),
                host_version: None,
                platform_type: None,
                platform_version: None,
                people_chain_genesis_hash: [0xa2; 32].to_vec(),
                pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
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

    /// A permission callback that blocks awaiting the user's decision runs on
    /// the blocking pool, so an unrelated request on the same connection
    /// still round-trips while the callback is blocked.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn blocked_permission_callback_does_not_stall_bridge() {
        use std::sync::atomic::{AtomicBool, Ordering};

        use futures::SinkExt;
        use parity_scale_codec::{Decode, Encode};
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        use truapi::versioned::permissions::HostDevicePermissionRequest;
        use truapi_platform::PairingDeeplinkScheme;

        use crate::frame::{Payload, ProtocolMessage, request_ids};

        /// `device_permission` blocks until the test sends on `release`;
        /// every other callback is a trivial success.
        struct GatedPermissionCallbacks {
            permission_entered: Arc<AtomicBool>,
            release: Mutex<std::sync::mpsc::Receiver<()>>,
        }

        impl HostCallbacks for GatedPermissionCallbacks {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            fn push_notification(
                &self,
                _text: String,
                _deeplink: Option<String>,
                _scheduled_at_ms: Option<u64>,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            fn device_permission(&self, _capability: String) -> Result<bool, HostRejection> {
                self.permission_entered.store(true, Ordering::SeqCst);
                self.release
                    .lock()
                    .expect("release receiver mutex poisoned")
                    .recv()
                    .expect("release signal");
                Ok(true)
            }
            fn remote_permission(
                &self,
                _permission: String,
                _domains: Vec<String>,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn write_session(&self, _value: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn clear_session(&self) -> Result<(), HostRejection> {
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
            fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_create_transaction(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_resource_allocation(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_preimage_submit(&self, _size: u64) -> Result<(), HostRejection> {
                Ok(())
            }
            fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection> {
                Ok(value)
            }
            fn lookup_preimage(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<HostTheme, HostRejection> {
                Ok(HostTheme::Light)
            }
            fn feature_supported_chain(
                &self,
                _genesis_hash: Vec<u8>,
            ) -> Result<bool, HostRejection> {
                Ok(true)
            }
            fn chat_create_room(
                &self,
                _room_id: String,
                _name: String,
                _icon: String,
            ) -> Result<HostChatRoomRegistrationStatus, HostRejection> {
                Ok(HostChatRoomRegistrationStatus::New)
            }
            fn chat_post_text_message(
                &self,
                _room_id: String,
                _text: String,
            ) -> Result<String, HostRejection> {
                Ok("message-1".to_string())
            }
            fn chat_post_custom_message(
                &self,
                _room_id: String,
                _message_type: String,
                _payload: Vec<u8>,
            ) -> Result<String, HostRejection> {
                Ok("message-1".to_string())
            }
            fn payment_balance_subscribe(
                &self,
            ) -> Result<String, HostPaymentBalanceSubscriptionRejection> {
                Ok("0".to_string())
            }
            fn payment_request(
                &self,
                _amount_in_planks: String,
                _destination: Vec<u8>,
            ) -> Result<String, HostPaymentRequestRejection> {
                Ok("payment-1".to_string())
            }
            fn payment_top_up(
                &self,
                _amount_in_planks: String,
                _source: NativePaymentTopUpSource,
            ) -> Result<(), HostPaymentTopUpRejection> {
                Ok(())
            }
            fn payment_status_subscribe(
                &self,
                _payment_id: String,
            ) -> Result<NativePaymentStatus, HostPaymentStatusSubscriptionRejection> {
                Ok(NativePaymentStatus::Completed)
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

        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let permission_entered = Arc::new(AtomicBool::new(false));
        let callbacks: Arc<dyn HostCallbacks> = Arc::new(GatedPermissionCallbacks {
            permission_entered: permission_entered.clone(),
            release: Mutex::new(release_rx),
        });
        let events = Arc::new(NativeEventBus::default());
        let platform = Arc::new(CallbackPlatform { callbacks, events });
        let core = Arc::new(TrUApiCore::from_platform_with_config(
            platform,
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
            },
            crate::subscription::thread_per_subscription_spawner(),
        ));
        let logger: BridgeLogger = Arc::new(|_, _| {});
        let (mut bridge, endpoint) = WsBridge::start(0, core, logger).expect("start bridge");
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
                .expect("feature_supported must answer while the permission is blocked");

            release_tx.send(()).expect("release permission callback");
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

        bridge.stop();
    }
}
