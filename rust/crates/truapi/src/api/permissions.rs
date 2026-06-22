//! Unified [`Permissions`] trait.

use crate::versioned::permissions::{
    HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Permission request methods.
pub trait Permissions: Send + Sync {
    /// Request a device-capability permission from the user.
    ///
    /// # Permissions
    ///
    /// - **prompt**: device permission grant dialog
    ///
    /// ```ts
    /// const result = await truapi.permissions.requestDevicePermission("Camera");
    /// assert(result.isOk(), "requestDevicePermission failed:", result);
    /// console.log("device permission result:", result.value);
    /// ```
    #[wire(request_id = 8)]
    async fn request_device_permission(
        &self,
        cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>>;

    /// Request a remote-operation permission.
    ///
    /// # Permissions
    ///
    /// - **prompt**: remote permission grant dialog
    ///
    /// ```ts
    /// const result = await truapi.permissions.requestRemotePermission({
    ///   permission: { tag: "Remote", value: { domains: ["api.example.com"] } },
    /// });
    /// assert(result.isOk(), "requestRemotePermission failed:", result);
    /// console.log("remote permission result:", result.value);
    /// ```
    #[wire(request_id = 10)]
    async fn request_remote_permission(
        &self,
        cx: &CallContext,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>>;
}
