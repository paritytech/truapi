//! Unified [`System`] trait.

use crate::versioned::system::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
    HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationError,
    HostPushNotificationRequest, HostPushNotificationResponse,
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse, HostThemeSubscribeItem, RemotePermissionError,
    RemotePermissionRequest, RemotePermissionResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// General-purpose TrUAPI methods for feature detection, navigation,
/// notifications, and host-managed capabilities.
///
/// # Wire id reservations
///
/// The discriminants below are listed in [`super::RESERVED_WIRE_IDS`] so
/// codegen rejects any `#[wire(...)]` annotation that collides with them.
/// Slots are held back for upstream `triangle-js-sdks` methods that TrUAPI
/// does not implement, but whose ids must remain free to keep our wire-table
/// positionally aligned with the canonical host `MessagePayload` enum. If we
/// ever need one, annotate the trait method with the matching id and remove
/// it from `RESERVED_WIRE_IDS`.
///
#[async_trait::async_trait]
pub trait System: Send + Sync {
    /// Negotiates the wire codec version with the product.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function handshake(truapi: Client): Promise<void> {
    ///   const result = await truapi.system.handshake();
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 0)]
    async fn host_handshake(
        &self,
        _cx: &CallContext,
        request: HostHandshakeRequest,
    ) -> Result<HostHandshakeResponse, CallError<HostHandshakeError>> {
        let HostHandshakeRequest::V1(version) = request;
        if version.codec_version == 1 {
            Ok(HostHandshakeResponse::V1)
        } else {
            Err(CallError::Domain(HostHandshakeError::V1(
                crate::v01::HostHandshakeError::UnsupportedProtocolVersion,
            )))
        }
    }

    /// Queries whether the host supports a specific feature.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function supportsChain(truapi: Client): Promise<boolean> {
    ///   const result = await truapi.system.featureSupported({
    ///     tag: "Chain",
    ///     value: {
    ///       genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.supported;
    /// }
    /// ```
    #[wire(request_id = 2)]
    async fn host_feature_supported(
        &self,
        cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>>;

    /// Sends a push notification to the user.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function pushNotification(truapi: Client): Promise<void> {
    ///   const result = await truapi.system.pushNotification({
    ///     text: "Hello!",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 4)]
    async fn host_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    /// Requests the host to open a URL.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function navigateToDocs(truapi: Client): Promise<void> {
    ///   const result = await truapi.system.navigateTo({
    ///     url: "https://example.com",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 6)]
    async fn host_navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>>;

    /// Request a device-capability permission from the user.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostDevicePermissionResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestCameraPermission(
    ///   truapi: Client,
    /// ): Promise<HostDevicePermissionResponse> {
    ///   const result = await truapi.system.devicePermission("Camera");
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
    /// import {
    ///   type Client,
    ///   type RemotePermissionResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestRemotePermission(
    ///   truapi: Client,
    /// ): Promise<RemotePermissionResponse> {
    ///   const result = await truapi.system.permission({
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

    /// Subscribe to host theme changes (light/dark).
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostThemeSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchTheme(truapi: Client): Subscription {
    ///   return truapi.system.themeSubscribe().subscribe({
    ///     next: (theme: HostThemeSubscribeItem) => console.log(theme),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 104)]
    async fn host_theme_subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        Subscription::empty()
    }

    /// Derive 32 bytes of entropy from the user's root BIP-39 entropy for the
    /// given key.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostDeriveEntropyResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function deriveEntropy(
    ///   truapi: Client,
    /// ): Promise<HostDeriveEntropyResponse> {
    ///   const result = await truapi.system.deriveEntropy({
    ///     context: "0x70726f647563742d6b6579",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 108)]
    async fn host_derive_entropy(
        &self,
        _cx: &CallContext,
        _request: HostDeriveEntropyRequest,
    ) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
        Err(CallError::unavailable())
    }

    /// Request the host to pre-allocate one or more resources (statement store
    /// allowance, bulletin allowance, smart contract allowance, auto-signing).
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostRequestResourceAllocationResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestAllocation(
    ///   truapi: Client,
    /// ): Promise<HostRequestResourceAllocationResponse> {
    ///   const result =
    ///     await truapi.system.requestResourceAllocation({
    ///       resources: [
    ///         { tag: "StatementStoreAllowance" },
    ///         { tag: "AutoSigning" },
    ///       ],
    ///     });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 130)]
    async fn host_request_resource_allocation(
        &self,
        _cx: &CallContext,
        _request: HostRequestResourceAllocationRequest,
    ) -> Result<
        HostRequestResourceAllocationResponse,
        CallError<HostRequestResourceAllocationError>,
    > {
        Err(CallError::unavailable())
    }
}
