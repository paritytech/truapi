//! Versioned wrappers for [`JsonRpc`](crate::api::JsonRpc) methods.

use crate::v01;

versioned_type! {
    pub enum HostJsonrpcMessageSendRequest { V1 => v01::HostJsonrpcMessageSendRequest }
    pub enum HostJsonrpcMessageSendResponse { V1 }
    pub enum HostJsonrpcMessageSendError { V1 => v01::GenericError }
    pub enum HostJsonrpcMessageSubscribeRequest { V1 => v01::HostJsonrpcMessageSubscribeRequest }
    pub enum HostJsonrpcMessageSubscribeItem { V1 => v01::HostJsonrpcMessageSubscribeItem }
}
