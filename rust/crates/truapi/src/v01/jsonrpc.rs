use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSendRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// JSON-RPC message body.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeItem {
    /// JSON-RPC message body.
    pub message: String,
}
