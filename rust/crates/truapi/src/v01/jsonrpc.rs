use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSendRequest {
    pub genesis_hash: Vec<u8>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeRequest {
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeItem {
    pub message: String,
}
