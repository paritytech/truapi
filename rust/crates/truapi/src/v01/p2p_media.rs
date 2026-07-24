use parity_scale_codec::{Decode, Encode};

/// Opaque p2p room handle, host-minted, scoped to the issuing product instance.
pub type P2pRoomId = u64;

/// Which media directions a room wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct RtDirections {
    /// Publish camera video into the room.
    pub publish_video: bool,
    /// Publish microphone audio into the room.
    pub publish_audio: bool,
    /// Receive remote peers' video.
    pub receive_video: bool,
    /// Receive remote peers' audio.
    pub receive_audio: bool,
}

/// The loopback endpoint a product dials with `@moq/*` / raw WebTransport
/// (`RtRelayConfig`-shaped, loopback edition). The token rides both URLs as
/// `?jwt=…` and scopes the session to the room's root: publish under `self/`
/// only, subscribe everything (bridged peers appear at `room/<peer-id>/<name>`).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pEndpoint {
    /// `https://127.0.0.1:<port>/?jwt=<token>` - WebTransport.
    pub wt_url: String,
    /// sha-256 (hex) of the relay's self-signed cert, for
    /// `serverCertificateHashes`.
    pub cert_sha256: String,
    /// `ws://127.0.0.1:<port>/?jwt=<token>` - WebSocket fallback.
    pub ws_url: String,
    /// Token expiry, unix millis. Refresh via `endpoint_refresh`.
    pub expires_at_ms: u64,
}

/// Shared error union for every [`P2pMedia`](crate::api::P2pMedia) method.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum P2pError {
    /// The user declined `MediaP2p` or a required device permission.
    PermissionDenied,
    /// The invite ticket failed to parse or has expired.
    InvalidTicket,
    /// All bootstrap peers offline / gossip join failed.
    JoinFailed {
        /// Human-readable failure diagnostic.
        reason: String,
    },
    /// The room handle does not name a live room of this product instance.
    RoomNotFound,
    /// A named broadcast never appeared in the loopback relay.
    BroadcastMissing {
        /// The broadcast name that never appeared.
        name: String,
    },
    /// The host's per-product concurrent room cap was reached.
    TooManyRooms,
    /// Caller modality may not perform this operation (e.g. capture
    /// directions from a widget, or any room from a headless worker).
    NotAllowedForModality,
    /// This host/platform cannot run a p2p node.
    Unsupported,
    /// Catch-all.
    Unknown {
        /// Human-readable failure diagnostic.
        reason: String,
    },
}

/// `status` response: host p2p capability + node info.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pStatusResponse {
    /// Whether this host can run p2p media rooms.
    pub available: bool,
    /// The node's stable iroh endpoint id (hex), when running.
    pub endpoint_id: Option<String>,
    /// Number of live rooms held by the calling product.
    pub num_rooms: u32,
}

/// `room_create` request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pRoomCreateRequest {
    /// Requested media directions; publishing folds a camera/mic prompt.
    pub directions: RtDirections,
    /// Short human-readable purpose, shown in the permission prompt.
    pub purpose: String,
    /// Per-room presence display name.
    pub display_name: Option<String>,
}

/// Shared response for `room_create` / `room_join`: the opaque handle, its
/// invite ticket, and the loopback endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pRoomResponse {
    /// Opaque room handle - passed back to the other `P2pMedia` calls.
    pub room: P2pRoomId,
    /// The invite: a self-describing compact ticket string.
    pub ticket: String,
    /// The loopback endpoint the product dials.
    pub endpoint: HostP2pEndpoint,
}

/// `room_join` request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pRoomJoinRequest {
    /// The invite ticket string received out-of-band.
    pub ticket: String,
    /// Requested media directions; publishing folds a camera/mic prompt.
    pub directions: RtDirections,
    /// Short human-readable purpose, shown in the permission prompt.
    pub purpose: String,
    /// Per-room presence display name.
    pub display_name: Option<String>,
}

/// `room_leave` request (unit response).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pRoomLeaveRequest {
    /// The room to leave.
    pub room: P2pRoomId,
}

/// `endpoint_refresh` request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pEndpointRefreshRequest {
    /// The room whose loopback endpoint to re-issue.
    pub room: P2pRoomId,
}

/// `endpoint_refresh` response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pEndpointRefreshResponse {
    /// The fresh loopback endpoint (rotated token/cert).
    pub endpoint: HostP2pEndpoint,
}

/// `publish` request (unit response).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pPublishRequest {
    /// The room to offer the broadcasts to.
    pub room: P2pRoomId,
    /// Broadcast names, relative to the product's `self/` scope.
    pub names: Vec<String>,
}

/// `unpublish` request (unit response).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pUnpublishRequest {
    /// The room to withdraw the broadcasts from.
    pub room: P2pRoomId,
    /// Broadcast names, relative to the product's `self/` scope.
    pub names: Vec<String>,
}

/// `room_events` subscription request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostP2pRoomEventsRequest {
    /// The room whose events to stream.
    pub room: P2pRoomId,
}

/// `room_events` subscription item: roster + broadcast lifecycle +
/// rt-session lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostP2pRoomEvent {
    /// Emitted once on subscribe; grants are live.
    Active,
    /// A peer joined the room.
    PeerJoined {
        /// The peer's endpoint id (hex).
        peer: String,
        /// The peer's presence display name, if it announced one.
        display_name: Option<String>,
    },
    /// A peer left the room.
    PeerLeft {
        /// The peer's endpoint id (hex).
        peer: String,
    },
    /// The peer's broadcast is now served by the local loopback relay at
    /// `room/<peer>/<name>` under the product's scope root.
    BroadcastAdded {
        /// The publishing peer's endpoint id (hex).
        peer: String,
        /// The broadcast name.
        name: String,
    },
    /// The peer's broadcast disappeared from the loopback relay.
    BroadcastRemoved {
        /// The publishing peer's endpoint id (hex).
        peer: String,
        /// The broadcast name.
        name: String,
    },
    /// Loopback endpoint rotated (token/cert) - re-dial with the new config.
    EndpointChanged {
        /// The fresh loopback endpoint.
        endpoint: HostP2pEndpoint,
    },
    /// Platform is about to freeze the product.
    Suspending {
        /// Grace window before the freeze, in milliseconds.
        grace_ms: u32,
    },
    /// The product returned to the foreground.
    Resumed,
    /// Room ended by the host; default posture restored.
    Revoked {
        /// Human-readable revocation reason.
        reason: String,
    },
}
