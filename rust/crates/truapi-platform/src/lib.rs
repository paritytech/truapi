//! Platform abstraction traits for TrUAPI host implementations.
//!
//! Each platform (web/WASM, iOS/UniFFI, Android/UniFFI) implements these traits
//! to provide native capabilities. The TrUAPI dispatcher calls into these when
//! handling API requests from the product side.
//!
//! Type aliases here map the truapi v0.1 wire types to short, intent-revealing
//! names used at platform boundaries. They are kept as aliases so call sites
//! still flow through the canonical types defined in the `truapi` crate.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use futures::stream::BoxStream;

use truapi::v01::{
    GenericError, HostAccountCreateProofError, HostAccountGetError, HostGetUserIdError,
    HostLocalStorageReadError, HostNavigateToError, HostPushNotificationRequest,
    HostSignPayloadError, RemoteStatementStoreCreateProofError,
};
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

/// Re-export of `truapi::v01` for platform implementors.
pub use truapi::v01;
/// Re-export of `truapi::versioned` for platform implementors.
pub use truapi::versioned;

/// Key used by the [`Storage`] trait. Strings are namespaced per-product by the
/// platform implementation.
pub type StorageKey = String;

/// Value stored by the [`Storage`] trait.
pub type StorageValue = Vec<u8>;

/// Error returned by [`Storage`] operations.
pub type StorageError = HostLocalStorageReadError;

/// URL navigation error.
pub type NavigateToError = HostNavigateToError;

/// Push-notification payload delivered by [`Notifications`].
pub type PushNotification = HostPushNotificationRequest;

/// SCALE-encoded chain genesis hash, used to pick a JSON-RPC endpoint.
pub type GenesisHash = Vec<u8>;

/// Error returned by account credential lookups (`host_account_get`,
/// `host_account_get_alias`, legacy account enumeration).
pub type RequestCredentialsError = HostAccountGetError;

/// Error returned by ring VRF proof creation.
pub type CreateProofError = HostAccountCreateProofError;

/// Error returned by user-identity disclosure.
pub type UserIdentityError = HostGetUserIdError;

/// Error returned by host signing.
pub type SigningError = HostSignPayloadError;

/// Error returned by statement-store proof creation.
pub type StatementProofError = RemoteStatementStoreCreateProofError;

/// Scoped key-value storage. The platform namespaces keys so different products
/// cannot read each other's data.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Read a value by key.
    async fn read(&self, key: StorageKey) -> Result<Option<StorageValue>, StorageError>;
    /// Write a value to a key.
    async fn write(&self, key: StorageKey, value: StorageValue) -> Result<(), StorageError>;
    /// Clear a value at a key.
    async fn clear(&self, key: StorageKey) -> Result<(), StorageError>;
}

/// Open URLs in the system browser. Input is already trimmed, categorized,
/// and (where needed) normalized by the core, the host implementation is
/// expected to do nothing more than hand the URL to the OS URL handler.
#[async_trait]
pub trait Navigation: Send + Sync {
    /// Open the given URL in the system browser.
    async fn navigate_to(&self, url: String) -> Result<(), NavigateToError>;
}

/// Deliver push notifications.
#[async_trait]
pub trait Notifications: Send + Sync {
    /// Push the given notification to the user.
    async fn push_notification(&self, notification: PushNotification) -> Result<(), GenericError>;
}

/// Permission prompts. The v0.1 wire protocol keeps device permissions
/// (camera, mic, NFC, ...) separate from remote permissions (domain access,
/// chain submit, ...), so the platform surface mirrors that split.
#[async_trait]
pub trait Permissions: Send + Sync {
    /// Prompt the user for a device-level permission.
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, GenericError>;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, GenericError>;
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
#[async_trait]
pub trait ChainProvider: Send + Sync {
    /// Open a JSON-RPC connection for the chain identified by `genesis_hash`.
    /// Drop the returned connection to disconnect.
    async fn connect(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError>;
}

/// A live JSON-RPC connection to a chain.
pub trait JsonRpcConnection: Send + Sync {
    /// Send a JSON-RPC request string.
    fn send(&self, request: String);

    /// Stream of JSON-RPC response strings.
    fn responses(&self) -> BoxStream<'static, String>;
}

/// Account lookup, aliasing, proof generation, and connection status.
#[async_trait]
pub trait Accounts: Send + Sync {
    /// Retrieve a product-scoped account.
    async fn host_account_get(
        &self,
        request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, RequestCredentialsError>;

    /// Retrieve a contextual alias for a product account.
    async fn host_account_get_alias(
        &self,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, RequestCredentialsError>;

    /// Generate a ring VRF proof for a product account.
    async fn host_account_create_proof(
        &self,
        request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CreateProofError>;

    /// List non-product (legacy) accounts the user owns.
    async fn host_get_legacy_accounts(
        &self,
        request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, RequestCredentialsError>;

    /// Subscribe to account connection status changes.
    async fn host_account_connection_status_subscribe(
        &self,
    ) -> BoxStream<'static, HostAccountConnectionStatusSubscribeItem>;

    /// Fetch the user's primary identity.
    async fn host_get_user_id(
        &self,
        request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, UserIdentityError>;
}

/// Signing. Dotli's contract needs sign_payload and sign_raw, the runtime
/// leaves `host_create_transaction*` on its default "unavailable" body until a
/// host surfaces them.
#[async_trait]
pub trait Signing: Send + Sync {
    /// Sign a SCALE-encoded extrinsic payload.
    async fn host_sign_payload(
        &self,
        request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, SigningError>;

    /// Sign a raw payload (bytes or string).
    async fn host_sign_raw(
        &self,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, SigningError>;
}

/// Statement store subscribe, submit, and proof creation.
#[async_trait]
pub trait StatementStore: Send + Sync {
    /// Subscribe to statements matching the given topic filter.
    async fn remote_statement_store_subscribe(
        &self,
        request: RemoteStatementStoreSubscribeRequest,
    ) -> BoxStream<'static, RemoteStatementStoreSubscribeItem>;

    /// Submit a signed statement to the network.
    async fn remote_statement_store_submit(
        &self,
        request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), GenericError>;

    /// Create a cryptographic proof for a statement.
    async fn remote_statement_store_create_proof(
        &self,
        request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<RemoteStatementStoreCreateProofResponse, StatementProofError>;
}

/// Preimage lookup.
#[async_trait]
pub trait Preimage: Send + Sync {
    /// Subscribe to lookups for the given preimage key.
    async fn remote_preimage_lookup_subscribe(
        &self,
        request: RemotePreimageLookupSubscribeRequest,
    ) -> BoxStream<'static, RemotePreimageLookupSubscribeItem>;
}

/// Combined platform interface. A host must provide all of these.
pub trait Platform:
    Navigation
    + Notifications
    + Permissions
    + Features
    + Storage
    + ChainProvider
    + Accounts
    + Signing
    + StatementStore
    + Preimage
{
}

impl<T> Platform for T where
    T: Navigation
        + Notifications
        + Permissions
        + Features
        + Storage
        + ChainProvider
        + Accounts
        + Signing
        + StatementStore
        + Preimage
{
}
