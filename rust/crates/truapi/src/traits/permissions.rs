//! Unified [`Permissions`] trait.

use crate::v02::GenericError;
use crate::versioned::permissions::{
    HostDevicePermissionRequest, HostDevicePermissionResponse, RemotePermissionRequest,
    RemotePermissionResponse,
};
use crate::wire;
use crate::CallContext;

/// Device and remote permission prompts. Unified counterpart of
/// [`crate::v02::Permissions`].
#[async_trait::async_trait]
pub trait Permissions: Send + Sync {
    /// Request a device-capability permission from the user.
    #[wire(id = 8)]
    async fn host_device_permission(
        &self,
        cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, GenericError>;

    /// Request one or more remote-operation permissions.
    #[wire(id = 10)]
    async fn remote_permission(
        &self,
        cx: &CallContext,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, GenericError>;
}
