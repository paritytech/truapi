use parity_scale_codec::{Decode, Encode};

use super::{
    CreateTransactionLegacyRequest, CreateTransactionRequest, CreateTransactionResponse,
    ResourceAllocationRequest, ResourceAllocationResponse, RingVrfAliasRequest,
    RingVrfAliasResponse, SignRawLegacyRequest, SignRawLegacyResponse, SigningRequest,
    SigningResponse,
};

/// v1 messages exchanged with the paired signing host over the encrypted SSO channel.
///
/// The variant order is part of the SCALE wire protocol used inside
/// statement-store session statements.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessage {
    Disconnected,
    SignRequest(Box<SigningRequest>),
    SignResponse(SigningResponse),
    RingVrfAliasRequest(RingVrfAliasRequest),
    RingVrfAliasResponse(RingVrfAliasResponse),
    ResourceAllocationRequest(ResourceAllocationRequest),
    ResourceAllocationResponse(ResourceAllocationResponse),
    CreateTransactionRequest(CreateTransactionRequest),
    CreateTransactionResponse(CreateTransactionResponse),
    CreateTransactionLegacyRequest(CreateTransactionLegacyRequest),
    SignRawLegacyRequest(SignRawLegacyRequest),
    SignRawLegacyResponse(SignRawLegacyResponse),
}
