//! Versioned wrappers for [`JsonRpc`](crate::api::JsonRpc) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostJsonrpcMessageSendRequest`].
    pub enum HostJsonrpcMessageSendRequest { V1 => v01::HostJsonrpcMessageSendRequest }
    /// Versioned unit response for JSON-RPC send.
    pub enum HostJsonrpcMessageSendResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`].
    pub enum HostJsonrpcMessageSendError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::HostJsonrpcMessageSubscribeRequest`].
    pub enum HostJsonrpcMessageSubscribeRequest { V1 => v01::HostJsonrpcMessageSubscribeRequest }
    /// Versioned wrapper for [`v01::HostJsonrpcMessageSubscribeItem`].
    pub enum HostJsonrpcMessageSubscribeItem { V1 => v01::HostJsonrpcMessageSubscribeItem }
}
