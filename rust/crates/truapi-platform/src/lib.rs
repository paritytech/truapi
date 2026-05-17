//! Capability traits a TrUAPI host must implement.
//!
//! Each trait covers a single OS-primitive surface the Rust core cannot reach
//! from its own process (key-value persistence, URL launching, push
//! notifications, permission UI, chain RPC). Account management, signing,
//! statement-store and preimage flows live in the Rust core itself and are not
//! part of this trait set.
//!
//! Host implementations may use `async fn` in trait bodies directly. The
//! consumers (`truapi-server::runtime::PlatformRuntimeHost<P>`) are generic
//! over `P: Platform`, so `dyn Trait` object safety is not required.

#![forbid(unsafe_code)]

use futures::stream::BoxStream;

use truapi::v01::{
    GenericError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    HostLocalStorageReadError, HostNavigateToError, HostPushNotificationRequest,
    RemotePermissionRequest, RemotePermissionResponse,
};
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};

/// Re-export of `truapi::v01` for host implementations.
pub use truapi::v01;
/// Re-export of `truapi::versioned` for host implementations.
pub use truapi::versioned;

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
    /// Push the given notification to the user.
    fn push_notification(
        &self,
        notification: HostPushNotificationRequest,
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

/// Combined platform interface. A host must provide all capability traits.
pub trait Platform:
    Navigation + Notifications + Permissions + Features + Storage + ChainProvider
{
}

impl<T> Platform for T where
    T: Navigation + Notifications + Permissions + Features + Storage + ChainProvider
{
}
