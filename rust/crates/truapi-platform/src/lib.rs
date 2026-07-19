//! Capability traits a TrUAPI host must implement.
//!
//! Each trait covers a single OS-primitive surface the Rust core cannot reach
//! from its own process (key-value persistence, URL launching, push
//! notifications, permission UI, chain RPC, host-selected preimage backends).
//! Account management, signing, and statement-store protocol flows live in the
//! Rust core itself and are not part of this trait set.
//!
//! Async capability traits use `async_trait` so the combined [`Platform`]
//! surface can be used as a trait object by the runtime.

use futures::stream::BoxStream;
use parity_scale_codec::{Decode, Encode};
use unicode_normalization::UnicodeNormalization;

pub use async_trait::async_trait;

use truapi::latest::{
    GenericError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    HostFeatureSupportedRequest, HostFeatureSupportedResponse, HostLocalStorageReadError,
    HostNavigateToError, HostPushNotificationRequest, HostPushNotificationResponse,
    HostRequestResourceAllocationRequest, HostSignPayloadRequest,
    HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
    HostSignRawWithLegacyAccountRequest, LegacyAccountTxPayload, NotificationId,
    ProductAccountTxPayload, ProductProofContext, RemotePermissionRequest,
    RemotePermissionResponse, RingLocation, ThemeVariant,
};
use url::Url;

/// Role-neutral runtime configuration supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRuntimeConfig {
    /// Host metadata.
    pub host_info: HostInfo,
    /// Platform metadata.
    pub platform_info: PlatformInfo,
}

/// Pairing-host runtime configuration supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingHostConfig {
    /// Host identity shown to the signing host during pairing.
    ///
    /// Host-spec B.1.3 defines the host metadata consumed by the signing host:
    /// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L48-L60>
    pub host: HostRuntimeConfig,
    /// People-chain genesis hash used for statement-store SSO.
    pub people_chain_genesis_hash: [u8; 32],
    /// Bulletin-chain genesis hash used for in-core preimage submission.
    pub bulletin_chain_genesis_hash: [u8; 32],
    /// Deeplink URI scheme used in pairing QR payloads, without `://`.
    ///
    /// Host-spec L.2-L.3 define the `polkadotapp://pair` route and construction
    /// rules:
    /// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/L-url-schemes.md?plain=1#L17-L33>
    pub pairing_deeplink_scheme: String,
}

/// Signing-host runtime configuration supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningHostConfig {
    /// Host identity. Not read by the local-signing paths yet; retained for
    /// parity with [`PairingHostConfig`] and for the future signer-side SSO
    /// responder, which advertises host identity in handshake responses.
    pub host: HostRuntimeConfig,
    /// People-chain genesis hash used for statement-store product calls.
    pub people_chain_genesis_hash: [u8; 32],
    /// Bulletin-chain genesis hash used for in-core preimage submission.
    pub bulletin_chain_genesis_hash: [u8; 32],
}

/// Product identity attached to one product-facing TrUAPI connection.
///
/// A host may create multiple product runtimes from the same long-lived host
/// runtime, each with its own product context.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductContext {
    /// Product identifier used for account derivation and product-scoped
    /// storage/permission namespaces.
    ///
    /// Host-spec C.7 defines accepted product id forms:
    /// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L109-L128>
    pub product_id: String,
}

/// Host metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    /// Host name.
    pub name: String,
    /// Optional absolute HTTPS host icon URL.
    pub icon: Option<String>,
    /// Optional host version.
    pub version: Option<String>,
}

/// Platform metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformInfo {
    /// Optional platform/browser name.
    pub kind: Option<String>,
    /// Optional platform/browser version.
    pub version: Option<String>,
}

impl HostRuntimeConfig {
    /// Build a role-neutral host runtime config, validating fields whose
    /// representation cannot be made invalid by Rust types alone.
    pub fn new(
        host_info: HostInfo,
        platform_info: PlatformInfo,
    ) -> Result<Self, RuntimeConfigValidationError> {
        require_non_empty("host_info.name", &host_info.name)?;
        if let Some(icon) = &host_info.icon {
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
        Ok(Self {
            host_info,
            platform_info,
        })
    }
}

impl PairingHostConfig {
    /// Build a pairing-host runtime config, validating fields whose
    /// representation cannot be made invalid by Rust types alone.
    pub fn new(
        host_info: HostInfo,
        platform_info: PlatformInfo,
        people_chain_genesis_hash: [u8; 32],
        bulletin_chain_genesis_hash: [u8; 32],
        pairing_deeplink_scheme: String,
    ) -> Result<Self, RuntimeConfigValidationError> {
        require_non_empty("pairing_deeplink_scheme", &pairing_deeplink_scheme)?;
        if pairing_deeplink_scheme.contains("://") {
            return Err(RuntimeConfigValidationError::InvalidDeeplinkScheme {
                scheme: pairing_deeplink_scheme,
            });
        }
        let config = Self {
            host: HostRuntimeConfig::new(host_info, platform_info)?,
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
            pairing_deeplink_scheme,
        };
        Ok(config)
    }
}

impl SigningHostConfig {
    /// Build a signing-host runtime config, validating fields whose
    /// representation cannot be made invalid by Rust types alone.
    pub fn new(
        host_info: HostInfo,
        platform_info: PlatformInfo,
        people_chain_genesis_hash: [u8; 32],
        bulletin_chain_genesis_hash: [u8; 32],
    ) -> Result<Self, RuntimeConfigValidationError> {
        Ok(Self {
            host: HostRuntimeConfig::new(host_info, platform_info)?,
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
        })
    }
}

impl ProductContext {
    /// Build a product context, validating fields whose representation cannot
    /// be made invalid by Rust types alone.
    pub fn new(product_id: String) -> Result<Self, RuntimeConfigValidationError> {
        Ok(Self {
            product_id: normalize_product_identifier(&product_id)?,
        })
    }
}

/// Whether `identifier` is a product scope the core is allowed to derive for.
pub fn is_product_identifier(identifier: &str) -> bool {
    normalize_product_identifier(identifier).is_ok()
}

/// Normalize product identifiers before derivation and policy checks.
pub fn normalize_product_identifier(
    product_id: &str,
) -> Result<String, RuntimeConfigValidationError> {
    let trimmed = product_id.trim();
    require_non_empty("product_id", trimmed)?;
    let normalized = trimmed.nfc().collect::<String>().to_lowercase();
    if normalized.ends_with(".dot")
        || normalized == "localhost"
        || normalized.starts_with("localhost:")
    {
        Ok(normalized)
    } else {
        Err(RuntimeConfigValidationError::InvalidProductId {
            product_id: product_id.to_string(),
        })
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), RuntimeConfigValidationError> {
    if value.trim().is_empty() {
        return Err(RuntimeConfigValidationError::EmptyField { field });
    }
    Ok(())
}

/// Runtime config validation error.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum RuntimeConfigValidationError {
    /// Required string field was empty or whitespace-only.
    #[display("{field} must not be empty")]
    EmptyField {
        /// Field name.
        field: &'static str,
    },
    /// Host icon URL could not be parsed as an absolute HTTPS URL.
    #[display("host_info.icon must be an absolute HTTPS URL: {reason}")]
    InvalidHostIcon {
        /// Parse failure reason.
        reason: String,
    },
    /// Host icon URL used a non-HTTPS scheme.
    #[display("host_info.icon must use https scheme, got {scheme:?}")]
    InsecureHostIcon {
        /// Actual URL scheme.
        scheme: String,
    },
    /// Pairing deeplink scheme included a URL separator.
    #[display("pairing_deeplink_scheme must not include ://, got {scheme:?}")]
    InvalidDeeplinkScheme {
        /// Actual deeplink scheme value.
        scheme: String,
    },
    /// Product id was not a `.dot` or localhost product identifier.
    #[display("product_id must be a .dot or localhost product identifier, got {product_id:?}")]
    InvalidProductId {
        /// Actual product id value.
        product_id: String,
    },
}

/// Product-scoped key-value storage.
///
/// The core namespaces product keys before calling this trait. Host
/// implementations should treat `key` as an opaque OS-style storage key.
#[async_trait]
pub trait ProductStorage: Send + Sync {
    /// Read a value by key.
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, HostLocalStorageReadError>;

    /// Write a value to a key.
    async fn write(&self, key: String, value: Vec<u8>) -> Result<(), HostLocalStorageReadError>;

    /// Clear a value at a key.
    async fn clear(&self, key: String) -> Result<(), HostLocalStorageReadError>;
}

/// Open URLs in the system browser. Input is already trimmed, categorized,
/// and (where needed) normalized by the core; the host implementation only
/// needs to hand the URL to the OS URL handler.
#[async_trait]
pub trait Navigation: Send + Sync {
    /// Open the given URL in the system browser.
    async fn navigate_to(&self, url: String) -> Result<(), HostNavigateToError>;
}

/// Deliver push notifications.
#[async_trait]
pub trait Notifications: Send + Sync {
    /// Schedule or immediately display the given notification and return the
    /// host-assigned id.
    async fn push_notification(
        &self,
        notification: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, GenericError>;

    /// Cancel a notification by id. Idempotent: cancelling an already-fired or
    /// unknown id still returns `Ok(())`.
    async fn cancel_notification(&self, id: NotificationId) -> Result<(), GenericError> {
        let _ = id;
        Ok(())
    }
}

/// Permission prompts. Device permissions (camera, mic, NFC, ...) are separate
/// from remote permissions (domain access, chain submit, ...), so the platform
/// surface mirrors that split.
#[async_trait]
pub trait Permissions: Send + Sync {
    /// Prompt the user for a device-level permission.
    async fn device_permission(
        &self,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, GenericError>;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    async fn remote_permission(
        &self,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, GenericError>;
}

/// Permission request whose authorization status can be inspected or updated
/// by host administration UI.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PermissionAuthorizationRequest {
    /// Device-level permission such as camera, microphone, or location.
    Device(HostDevicePermissionRequest),
    /// Remote/product-scoped permission such as chain submit or HTTP access.
    Remote(RemotePermissionRequest),
    /// Product-scoped permission to disclose the user's primary identity.
    IdentityDisclosure,
}

/// Authorization status for a permission request.
///
/// `NotDetermined` means the core has no persisted answer and will prompt the
/// host the next time the product requests this permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PermissionAuthorizationStatus {
    /// No persisted authorization exists.
    NotDetermined,
    /// Access is denied.
    Denied,
    /// Access is authorized.
    Authorized,
}

/// Core-owned administration API exposed to host UI.
///
/// Hosts call this surface to drive global runtime actions or inspect/update
/// core-owned state without going through a product-scoped TrUAPI request.
#[async_trait]
pub trait CoreAdmin: Send + Sync {
    /// Best-effort logout/disconnect. Clears the active session and emits the
    /// resulting auth state transition.
    async fn disconnect_session(&self) -> Result<(), GenericError>;

    /// Read a stored permission authorization status without prompting.
    async fn get_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, GenericError>;

    /// Read stored permission authorization statuses without prompting.
    ///
    /// Results are returned in the same order as `requests`.
    async fn get_permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, GenericError>;

    /// Update a stored permission authorization status. `NotDetermined` clears
    /// the stored value so the next product request prompts again.
    async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), GenericError>;
}

/// Pairing-host-only administration API exposed to host UI.
#[async_trait]
pub trait PairingHostAdmin: Send + Sync {
    /// Cancel any in-flight pairing request.
    fn cancel_pairing(&self);

    /// Notify the core that the persisted auth-session blob may have changed.
    ///
    /// The host owns persistence and change detection. The pairing core owns
    /// decoding that blob into live `SessionState` / `AuthState`.
    fn notify_session_store_changed(&self);
}

/// Feature-support probing. The host answers whether it can service a given
/// capability (currently scoped to per-chain support).
#[async_trait]
pub trait Features: Send + Sync {
    /// Report whether the requested feature is supported.
    async fn feature_supported(
        &self,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, GenericError>;
}

/// JSON-RPC provider factory for chain access.
///
/// The platform provides a way to get a JSON-RPC connection for a given chain.
/// The server runtime manages the chainHead v1 state machine on top of this.
/// Host-spec N.6 requires products to access chains through host-mediated
/// providers:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/N-shared-infrastructure.md?plain=1#L91-L102>
#[async_trait]
pub trait ChainProvider: Send + Sync {
    /// Open a JSON-RPC connection for the chain identified by `genesis_hash`.
    /// Drop the returned connection to disconnect.
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError>;
}

/// A live JSON-RPC connection to a chain.
pub trait JsonRpcConnection: Send + Sync {
    /// Send a JSON-RPC request string.
    fn send(&self, request: String);

    /// Stream of JSON-RPC response strings.
    fn responses(&self) -> BoxStream<'static, String>;

    /// Close the connection lease.
    ///
    /// Hosts may keep a shared underlying transport alive, but this handle
    /// must stop receiving responses and release any per-caller resources.
    fn close(&self);
}

/// Core-owned host-private storage slots. Products never address these slots;
/// the host chooses the backing store for each slot.
///
/// Storage is host-local; `storage.md` records the current status quo:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/storage.md?plain=1#L1-L7>
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoreStorageKey {
    /// Opaque SSO/auth session blob.
    AuthSession,
    /// Pairing device identity used during SSO flows.
    PairingDeviceIdentity,
    /// Persisted authorization for one product-scoped permission request.
    PermissionAuthorization {
        /// Product whose permission decision is being stored.
        product_id: String,
        /// Permission request whose authorization is being stored.
        request: PermissionAuthorizationRequest,
    },
    /// Persisted allowance-slot keys for one paired SSO session.
    AllowanceKeys {
        /// Stable host-derived SSO session id.
        session_id: String,
    },
    /// Last processed SSO pairing response statement for the pairing device.
    LastProcessedPairingStatement,
}

/// Host-private persistence for core-owned state.
#[async_trait]
pub trait CoreStorage: Send + Sync {
    /// Read a core-owned value by typed slot.
    async fn read_core_storage(&self, key: CoreStorageKey)
    -> Result<Option<Vec<u8>>, GenericError>;

    /// Write a core-owned value by typed slot.
    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError>;

    /// Clear a core-owned value by typed slot.
    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError>;
}

/// Decoded session fields a host shell needs to render account UI without
/// parsing the opaque session blob the core persists through [`CoreStorage`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
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
    /// The wallet accepted the pairing request and the core is resolving and
    /// persisting the session. Hosts should replace the pairing QR with an
    /// in-progress presentation until a terminal state is emitted.
    Authenticating,
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

/// Review shown before a sign-payload request is sent to the paired wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SignPayloadReview {
    /// Product-account signing request.
    Product(HostSignPayloadRequest),
    /// Legacy-account signing request.
    LegacyAccount(HostSignPayloadWithLegacyAccountRequest),
}

/// Review shown before a sign-raw request is sent to the paired wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SignRawReview {
    /// Product-account raw signing request.
    Product(HostSignRawRequest),
    /// Legacy-account raw signing request.
    LegacyAccount(HostSignRawWithLegacyAccountRequest),
}

/// Review shown before a transaction-creation request is sent to the paired wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionReview {
    /// Product-account transaction request.
    Product(ProductAccountTxPayload),
    /// Legacy-account transaction request.
    LegacyAccount(LegacyAccountTxPayload),
}

/// Review shown before a product derives a contextual alias (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AccountAliasReview {
    /// Product requesting the alias.
    pub calling_product_id: String,
    /// Product-scoped context the alias is bound to.
    pub context: ProductProofContext,
    /// Ring the alias is derived against.
    pub ring_location: RingLocation,
}

/// Review shown before a product creates a ring-VRF proof (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateProofReview {
    /// Product requesting the proof.
    pub calling_product_id: String,
    /// Product-scoped context the proof's alias is bound to.
    pub context: ProductProofContext,
    /// Ring the proof is generated against.
    pub ring_location: RingLocation,
    /// Opaque message bound into the proof.
    pub message: Vec<u8>,
}

/// Review shown before a product asks to access another product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AccountAccessReview {
    /// Product currently handling the request.
    pub requesting_product_id: String,
    /// Product whose account is being requested.
    pub target_product_id: String,
}

/// Review shown before a product learns the user's primary identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct IdentityDisclosureReview {
    /// Product currently handling the request.
    pub product_id: String,
}

/// Review shown before a preimage is submitted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PreimageSubmitReview {
    /// Size of the preimage in bytes.
    pub size: u64,
}

/// Review shown before a user-confirmed core action continues.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum UserConfirmationReview {
    /// Sign a SCALE payload with a product or legacy account.
    SignPayload(SignPayloadReview),
    /// Sign raw bytes with a product or legacy account.
    SignRaw(SignRawReview),
    /// Create a transaction with a product or legacy account.
    CreateTransaction(CreateTransactionReview),
    /// Allow a product to derive a contextual alias for a ring.
    AccountAlias(AccountAliasReview),
    /// Allow a product to create a ring-VRF proof for a ring.
    CreateProof(CreateProofReview),
    /// Allow a product to learn the user's primary identity.
    IdentityDisclosure(IdentityDisclosureReview),
    /// Allocate resources for the requesting product.
    ResourceAllocation(HostRequestResourceAllocationRequest),
    /// Submit a preimage to the host-selected backend.
    PreimageSubmit(PreimageSubmitReview),
    /// Allow a product to access another product account.
    AccountAccess(AccountAccessReview),
}

/// Local user confirmation UI for sensitive core-owned operations.
#[async_trait]
pub trait UserConfirmation: Send + Sync {
    /// Confirm a reviewed action before the core continues.
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, GenericError>;
}

/// Host theme source.
pub trait ThemeHost: Send + Sync {
    /// Emits current theme immediately, then future changes.
    fn subscribe_theme(&self) -> BoxStream<'static, Result<ThemeVariant, GenericError>>;
}

/// Host preimage backend. The core builds, signs, and submits the Bulletin
/// `TransactionStorage.store` transaction itself; the host only owns preimage
/// content retrieval (P2P/IPFS lookup).
#[async_trait]
pub trait PreimageHost: Send + Sync {
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
    + ProductStorage
    + CoreStorage
    + ChainProvider
    + AuthPresenter
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
        + ProductStorage
        + CoreStorage
        + ChainProvider
        + AuthPresenter
        + UserConfirmation
        + ThemeHost
        + PreimageHost
{
}
