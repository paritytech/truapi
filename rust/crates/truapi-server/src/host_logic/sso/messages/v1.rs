//! V1 application messages exchanged on the encrypted SSO channel.
//!
//! Baseline variants are specified in host-spec B.5:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L189-L208>
//! Additional deployed variants are tracked as divergence D-B.5.6:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/divergences.md?plain=1#L26-L35>

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
