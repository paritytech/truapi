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
    RingVrfAliasResponse, RingVrfProofRequest, RingVrfProofResponse, SignRawLegacyRequest,
    SignRawLegacyResponse, SigningRequest, SigningResponse, StatementStoreProductSignRequest,
    StatementStoreProductSignResponse,
};

/// v1 messages exchanged with the paired signing host over the encrypted SSO channel.
///
/// The variant order is part of the SCALE wire protocol used inside
/// statement-store session statements.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessage {
    /// The peer is ending the SSO session.
    Disconnected,
    /// Ask the signing host to sign a payload or raw data with a product account.
    SignRequest(Box<SigningRequest>),
    /// Signing host's answer to [`RemoteMessage::SignRequest`].
    SignResponse(SigningResponse),
    /// Ask the Account Holder for a contextual alias.
    RingVrfAliasRequest(RingVrfAliasRequest),
    /// Account Holder's answer to [`RemoteMessage::RingVrfAliasRequest`].
    RingVrfAliasResponse(RingVrfAliasResponse),
    /// Ask the signing host to allocate SSO-backed resources.
    ResourceAllocationRequest(ResourceAllocationRequest),
    /// Signing host's answer to [`RemoteMessage::ResourceAllocationRequest`].
    ResourceAllocationResponse(ResourceAllocationResponse),
    /// Ask the signing host to create a signed product-account transaction.
    CreateTransactionRequest(CreateTransactionRequest),
    /// Signing host's answer to either transaction-creation request.
    CreateTransactionResponse(CreateTransactionResponse),
    /// Ask the signing host to create a signed legacy-account transaction.
    CreateTransactionLegacyRequest(CreateTransactionLegacyRequest),
    /// Ask the signing host to sign raw data with a legacy account.
    SignRawLegacyRequest(SignRawLegacyRequest),
    /// Signing host's answer to [`RemoteMessage::SignRawLegacyRequest`].
    SignRawLegacyResponse(SignRawLegacyResponse),
    /// Ask the Account Holder for a ring-VRF proof.
    RingVrfProofRequest(RingVrfProofRequest),
    /// Account Holder's answer to [`RemoteMessage::RingVrfProofRequest`].
    RingVrfProofResponse(RingVrfProofResponse),
    /// Ask the signing host to sign an exact statement-store payload.
    StatementStoreProductSignRequest(StatementStoreProductSignRequest),
    /// Signing host's answer to
    /// [`RemoteMessage::StatementStoreProductSignRequest`].
    StatementStoreProductSignResponse(StatementStoreProductSignResponse),
}
