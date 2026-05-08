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
    /// ```truapi-playground-request
    /// { "tag": "Camera" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function requestCameraPermission(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.permissions.devicePermission({
    ///     tag: "Camera",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 8)]
    async fn host_device_permission(
        &self,
        cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>>;

    /// Request one or more remote-operation permissions.
    ///
    /// ```truapi-playground-request
    /// { "permissions": [{ "tag": "Remote", "value": { "domains": ["api.example.com"] } }] }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function requestRemotePermission(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.permissions.permission({
    ///     permissions: [
    ///       { tag: "Remote", value: { domains: ["api.example.com"] } },
    ///     ],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 10)]
    async fn remote_permission(
        &self,
        cx: &CallContext,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>>;
}
