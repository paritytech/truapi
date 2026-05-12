use parity_scale_codec::{Decode, Encode};

/// Request to send a JSON-RPC message to a chain identified by its genesis hash.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSendRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// JSON-RPC message body.
    pub message: String,
}

/// Request to subscribe to inbound JSON-RPC messages for a chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// An inbound JSON-RPC message from the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeItem {
    /// JSON-RPC message body.
    pub message: String,
}
