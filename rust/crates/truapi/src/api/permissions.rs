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
    /// ```ts
    /// import {
    ///   type Client,
    ///   type HostDevicePermissionResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestCameraPermission(
    ///   truapi: Client,
    /// ): Promise<HostDevicePermissionResponse> {
    ///   const result = await truapi.permissions.requestDevicePermission("Camera");
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 8)]
    async fn request_device_permission(
        &self,
        cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>>;

    /// Request one or more remote-operation permissions.
    ///
    /// ```ts
    /// import {
    ///   type Client,
    ///   type RemotePermissionResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestRemotePermission(
    ///   truapi: Client,
    /// ): Promise<RemotePermissionResponse> {
    ///   const result = await truapi.permissions.requestRemotePermission({
    ///     permissions: [{ tag: "Remote", value: { domains: ["api.example.com"] } }],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 10)]
    async fn request_remote_permission(
        &self,
        cx: &CallContext,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>>;
}
