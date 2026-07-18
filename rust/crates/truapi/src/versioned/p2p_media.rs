//! Versioned wrappers for [`P2pMedia`](crate::api::P2pMedia) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostP2pStatusRequest { V1 }
    pub enum HostP2pStatusResponse { V1 => v01::HostP2pStatusResponse }
    pub enum HostP2pStatusError { V1 => v01::P2pError }
    pub enum HostP2pRoomCreateRequest { V1 => v01::HostP2pRoomCreateRequest }
    pub enum HostP2pRoomCreateResponse { V1 => v01::HostP2pRoomResponse }
    pub enum HostP2pRoomCreateError { V1 => v01::P2pError }
    pub enum HostP2pRoomJoinRequest { V1 => v01::HostP2pRoomJoinRequest }
    pub enum HostP2pRoomJoinResponse { V1 => v01::HostP2pRoomResponse }
    pub enum HostP2pRoomJoinError { V1 => v01::P2pError }
    pub enum HostP2pRoomLeaveRequest { V1 => v01::HostP2pRoomLeaveRequest }
    pub enum HostP2pRoomLeaveResponse { V1 }
    pub enum HostP2pRoomLeaveError { V1 => v01::P2pError }
    pub enum HostP2pEndpointRefreshRequest { V1 => v01::HostP2pEndpointRefreshRequest }
    pub enum HostP2pEndpointRefreshResponse { V1 => v01::HostP2pEndpointRefreshResponse }
    pub enum HostP2pEndpointRefreshError { V1 => v01::P2pError }
    pub enum HostP2pPublishRequest { V1 => v01::HostP2pPublishRequest }
    pub enum HostP2pPublishResponse { V1 }
    pub enum HostP2pPublishError { V1 => v01::P2pError }
    pub enum HostP2pUnpublishRequest { V1 => v01::HostP2pUnpublishRequest }
    pub enum HostP2pUnpublishResponse { V1 }
    pub enum HostP2pUnpublishError { V1 => v01::P2pError }
    pub enum HostP2pRoomEventsRequest { V1 => v01::HostP2pRoomEventsRequest }
    pub enum HostP2pRoomEventsItem { V1 => v01::HostP2pRoomEvent }
    pub enum HostP2pRoomEventsError { V1 => v01::P2pError }
}
