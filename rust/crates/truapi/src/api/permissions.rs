//! Unified [`Permissions`] trait.

use crate::versioned::permissions::{
    HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Device and remote permission prompts.
#[async_trait::async_trait]
pub trait Permissions: Send + Sync {
    /// Request a device-capability permission from the user.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function requestCameraPermission(truapi: Client) {
    ///   const result = await truapi.permissions.devicePermission({
    ///     tag: "Camera",
    ///     value: undefined,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 8)]
    async fn host_device_permission(
        &self,
        cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>>;

    /// Request one or more remote-operation permissions.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function requestRemotePermission(truapi: Client) {
    ///   const result = await truapi.permissions.permission({
    ///     permissions: [{ tag: "Remote", value: { domains: ["api.example.com"] } }],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 10)]
    async fn remote_permission(
        &self,
        cx: &CallContext,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>>;
}
