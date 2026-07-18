//! Unified [`P2pMedia`] trait.

use crate::versioned::p2p_media::{
    HostP2pEndpointRefreshError, HostP2pEndpointRefreshRequest, HostP2pEndpointRefreshResponse,
    HostP2pPublishError, HostP2pPublishRequest, HostP2pPublishResponse, HostP2pRoomCreateError,
    HostP2pRoomCreateRequest, HostP2pRoomCreateResponse, HostP2pRoomEventsError,
    HostP2pRoomEventsItem, HostP2pRoomEventsRequest, HostP2pRoomJoinError, HostP2pRoomJoinRequest,
    HostP2pRoomJoinResponse, HostP2pRoomLeaveError, HostP2pRoomLeaveRequest,
    HostP2pRoomLeaveResponse, HostP2pStatusError, HostP2pStatusRequest, HostP2pStatusResponse,
    HostP2pUnpublishError, HostP2pUnpublishRequest, HostP2pUnpublishResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Peer-to-peer media rooms (MoQ-over-iroh).
///
/// The host embeds a headless iroh/MoQ node and exposes rooms - behind
/// the RFC 0002 grant model. Products reach their room through a loopback moq
/// relay the host serves on `127.0.0.1`: publishes under `self/…` fan out to
/// peers, remote peers' broadcasts appear under `room/<peer-id>/…`.
pub trait P2pMedia: Send + Sync {
    /// Probe the host's p2p capability + node info. Never prompts.
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.status();
    /// assert(result.isOk(), "status failed:", result);
    /// console.log("p2p available:", result.value.available);
    /// ```
    #[wire(request_id = 164)]
    async fn status(
        &self,
        _cx: &CallContext,
        _request: HostP2pStatusRequest,
    ) -> Result<HostP2pStatusResponse, CallError<HostP2pStatusError>> {
        Err(CallError::unavailable())
    }

    /// Create a room (host side). THE prompting method: resolves
    /// `RemotePermission::MediaP2p` - even for receive-only rooms (peers learn
    /// the user's network address) - plus `DevicePermission::{Camera,
    /// Microphone}` per the requested directions, in ONE prompt, persisted per
    /// RFC 0002. Grants (loopback connect allowance, inline playback/autoplay,
    /// lifecycle) apply for the room's lifetime.
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.roomCreate({
    ///   directions: {
    ///     publishVideo: true,
    ///     publishAudio: true,
    ///     receiveVideo: true,
    ///     receiveAudio: true,
    ///   },
    ///   purpose: "Video room",
    ///   displayName: undefined,
    /// });
    /// assert(result.isOk(), "roomCreate failed:", result);
    /// console.log("room ticket:", result.value.ticket);
    /// ```
    #[wire(request_id = 166)]
    async fn room_create(
        &self,
        _cx: &CallContext,
        _request: HostP2pRoomCreateRequest,
    ) -> Result<HostP2pRoomCreateResponse, CallError<HostP2pRoomCreateError>> {
        Err(CallError::unavailable())
    }

    /// Join a room via its invite ticket string. Same gating and response
    /// shape as `room_create`.
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.roomJoin({
    ///   ticket: "room-invite-ticket",
    ///   directions: {
    ///     publishVideo: false,
    ///     publishAudio: false,
    ///     receiveVideo: true,
    ///     receiveAudio: true,
    ///   },
    ///   purpose: "Video room",
    ///   displayName: undefined,
    /// });
    /// assert(result.isOk(), "roomJoin failed:", result);
    /// console.log("joined room:", result.value.room);
    /// ```
    #[wire(request_id = 168)]
    async fn room_join(
        &self,
        _cx: &CallContext,
        _request: HostP2pRoomJoinRequest,
    ) -> Result<HostP2pRoomJoinResponse, CallError<HostP2pRoomJoinError>> {
        Err(CallError::unavailable())
    }

    /// Leave a room and restore the default sandbox posture. De-escalation
    /// only - no prompt (mirrors the rt-session `session_close`).
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.roomLeave({ room: 1n });
    /// assert(result.isOk(), "roomLeave failed:", result);
    /// ```
    #[wire(request_id = 170)]
    async fn room_leave(
        &self,
        _cx: &CallContext,
        _request: HostP2pRoomLeaveRequest,
    ) -> Result<HostP2pRoomLeaveResponse, CallError<HostP2pRoomLeaveError>> {
        Err(CallError::unavailable())
    }

    /// Re-issue the loopback endpoint (token/cert rotation). Valid room
    /// required, no prompt - mirrors the rt-session `relay_token`.
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.endpointRefresh({ room: 1n });
    /// assert(result.isOk(), "endpointRefresh failed:", result);
    /// console.log("fresh endpoint:", result.value.endpoint.wtUrl);
    /// ```
    #[wire(request_id = 172)]
    async fn endpoint_refresh(
        &self,
        _cx: &CallContext,
        _request: HostP2pEndpointRefreshRequest,
    ) -> Result<HostP2pEndpointRefreshResponse, CallError<HostP2pEndpointRefreshError>> {
        Err(CallError::unavailable())
    }

    /// Offer broadcast names (relative to the product's `self/` scope) to the
    /// room. Valid room required; the product must be publishing them into the
    /// loopback relay (or start within the host's patience).
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.publish({
    ///   room: 1n,
    ///   names: ["camera"],
    /// });
    /// assert(result.isOk(), "publish failed:", result);
    /// ```
    #[wire(request_id = 174)]
    async fn publish(
        &self,
        _cx: &CallContext,
        _request: HostP2pPublishRequest,
    ) -> Result<HostP2pPublishResponse, CallError<HostP2pPublishError>> {
        Err(CallError::unavailable())
    }

    /// Withdraw broadcast names from the room.
    ///
    /// ```ts
    /// const result = await truapi.p2PMedia.unpublish({
    ///   room: 1n,
    ///   names: ["camera"],
    /// });
    /// assert(result.isOk(), "unpublish failed:", result);
    /// ```
    #[wire(request_id = 176)]
    async fn unpublish(
        &self,
        _cx: &CallContext,
        _request: HostP2pUnpublishRequest,
    ) -> Result<HostP2pUnpublishResponse, CallError<HostP2pUnpublishError>> {
        Err(CallError::unavailable())
    }

    /// Room membership + broadcast lifecycle + rt-session lifecycle events.
    /// Holding this subscription is the KEEP-ALIVE signal:
    /// it keeps the node running through backgrounding grace
    /// windows, so a live media session must keep it open.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const event = await firstValueFrom(
    ///   from(truapi.p2PMedia.roomEvents({ room: 1n })),
    /// );
    /// console.log("room event:", event);
    /// ```
    #[wire(start_id = 178)]
    async fn room_events(
        &self,
        _cx: &CallContext,
        _request: HostP2pRoomEventsRequest,
    ) -> Result<Subscription<HostP2pRoomEventsItem>, CallError<HostP2pRoomEventsError>> {
        Err(CallError::unavailable())
    }
}
