//! SCALE codecs for host-papp SSO session-channel messages.
//!
//! These are the encrypted payloads carried inside statement-store
//! `SsoStatementData::Request` / `Response` frames.
//! The runtime builds them when forwarding TrUAPI account, signing, resource
//! allocation, and transaction requests to the paired signing host, then
//! decodes the signing host's responses while waiting on the SSO
//! statement-store channels.
//! The encrypted statement envelope and message identifiers are specified in
//! host-spec:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L151-L183>
//! The baseline remote message catalog is specified in host-spec:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L194-L208>
//! Deployed extension variants are tracked as a host-spec divergence:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/divergences.md?plain=1#L26-L33>
//! Field order and enum variant order are kept wire-compatible with host-papp:
//! <https://github.com/paritytech/triangle-js-sdks/blob/18c12d3bd1c51a9520eb247dc038ace2996dc2e7/packages/host-papp/src/sso/sessionManager/scale/remoteMessage.ts#L23-L35>
//! <https://github.com/paritytech/triangle-js-sdks/blob/18c12d3bd1c51a9520eb247dc038ace2996dc2e7/packages/host-papp/src/sso/sessionManager/scale/signing.ts#L6-L68>
//! <https://github.com/paritytech/triangle-js-sdks/blob/18c12d3bd1c51a9520eb247dc038ace2996dc2e7/packages/host-papp/src/sso/sessionManager/scale/ringVrf.ts#L5-L15>
//! <https://github.com/paritytech/triangle-js-sdks/blob/18c12d3bd1c51a9520eb247dc038ace2996dc2e7/packages/host-papp/src/sso/sessionManager/scale/createTransaction.ts#L6-L25>

use parity_scale_codec::{Decode, Encode, OptionBool};
use truapi::latest::{
    AccountId, AllocatableResource, HostAccountCreateProofResponse, HostAccountGetAliasResponse,
    HostSignPayloadRequest, HostSignRawRequest, LegacyAccountTxPayload, ProductAccountId,
    ProductAccountTxPayload, ProductProofContext, RawPayload, RingLocation,
};

use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::pairing::{
    AES_GCM_NONCE_LEN, SsoStatementData, decrypt_session_statement_data,
    encrypt_session_statement_data, encrypt_session_statement_data_with_nonce,
};
use crate::host_logic::statement_store::{
    build_signed_session_request_statement, current_unix_secs, decode_verified_statement_data,
    statement_expiry_elapsed,
};

pub mod v1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, derive_more::Display)]
enum SsoResponseCode {
    #[codec(index = 0)]
    #[display("success")]
    Success,
    #[codec(index = 1)]
    #[display("decryptionFailed")]
    DecryptionFailed,
    #[codec(index = 2)]
    #[display("decodingFailed")]
    DecodingFailed,
}

impl TryFrom<u8> for SsoResponseCode {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Success),
            1 => Ok(Self::DecryptionFailed),
            2 => Ok(Self::DecodingFailed),
            _ => Err(()),
        }
    }
}

/// Top-level remote message sent over the encrypted SSO channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteMessage {
    /// Correlation id used to match signing-host responses to pairing-host requests.
    pub message_id: String,
    /// Versioned remote message body.
    pub data: RemoteMessageData,
}

/// Versioned remote message body.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessageData {
    /// Version 1 of the remote message catalog.
    V1(v1::RemoteMessage),
}

/// Signing request flavor sent to the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRequest {
    /// Sign a full Substrate extrinsic payload.
    Payload(Box<SigningPayloadRequest>),
    /// Sign raw bytes or a string message.
    Raw(SigningRawRequest),
}

/// Request sent when a product asks the paired signing host to sign a Substrate
/// payload with a product-derived account.
///
/// Built from [`HostSignPayloadRequest`] but kept as a dedicated wire type
/// because the host-papp SSO dialect flattens the public request payload and
/// encodes `with_signed_transaction` as `OptionBool`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningPayloadRequest {
    /// Product account that signs the payload.
    pub product_account_id: ProductAccountId,
    /// Reference block hash.
    pub block_hash: Vec<u8>,
    /// Reference block number.
    pub block_number: Vec<u8>,
    /// Mortality era encoding.
    pub era: Vec<u8>,
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// SCALE-encoded call data.
    pub method: Vec<u8>,
    /// Account nonce.
    pub nonce: Vec<u8>,
    /// Runtime spec version.
    pub spec_version: Vec<u8>,
    /// Transaction tip.
    pub tip: Vec<u8>,
    /// Transaction format version.
    pub transaction_version: Vec<u8>,
    /// Extension identifiers.
    pub signed_extensions: Vec<String>,
    /// Extrinsic version.
    pub version: u32,
    /// For multi-asset tips.
    pub asset_id: Option<Vec<u8>>,
    /// CheckMetadataHash extension.
    pub metadata_hash: Option<Vec<u8>>,
    /// Metadata mode.
    pub mode: Option<u32>,
    /// Request the full signed transaction back.
    pub with_signed_transaction: OptionBool,
}

impl SigningPayloadRequest {
    fn from_host_request(value: HostSignPayloadRequest) -> Self {
        let payload = value.payload;
        Self {
            product_account_id: value.account,
            block_hash: payload.block_hash,
            block_number: payload.block_number,
            era: payload.era,
            genesis_hash: payload.genesis_hash,
            method: payload.method,
            nonce: payload.nonce,
            spec_version: payload.spec_version,
            tip: payload.tip,
            transaction_version: payload.transaction_version,
            signed_extensions: payload.signed_extensions,
            version: payload.version,
            asset_id: payload.asset_id,
            metadata_hash: payload.metadata_hash,
            mode: payload.mode,
            with_signed_transaction: OptionBool(payload.with_signed_transaction),
        }
    }
}

/// Request sent when a product asks the paired signing host to sign raw bytes or a
/// string message with a product-derived account.
///
/// Built from [`HostSignRawRequest`] and wrapped in
/// [`v1::RemoteMessage::SignRequest`] before being encrypted into an SSO session
/// statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningRawRequest {
    /// Product account that signs the payload.
    pub product_account_id: ProductAccountId,
    /// Raw bytes or string message to sign.
    pub data: SigningRawPayload,
}

impl SigningRawRequest {
    fn from_host_request(value: HostSignRawRequest) -> Self {
        Self {
            product_account_id: value.account,
            data: value.payload.into(),
        }
    }
}

/// Request sent when a product asks the paired signing host to sign raw data with a
/// user-imported legacy account.
///
/// Unlike product-account signing, the signer is the raw account id selected
/// from the user's legacy accounts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignRawLegacyRequest {
    /// Legacy account that signs the payload.
    pub account: AccountId,
    /// Raw bytes or string message to sign.
    pub data: SigningRawPayload,
}

/// Raw data accepted by SSO signing requests.
///
/// Used by both product-account raw signing and legacy-account raw signing to
/// distinguish binary payloads from string messages on the session-channel
/// wire.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRawPayload {
    /// Raw binary payload.
    Bytes(Vec<u8>),
    /// String message payload.
    Payload(String),
}

impl From<RawPayload> for SigningRawPayload {
    fn from(value: RawPayload) -> Self {
        match value {
            RawPayload::Bytes { bytes } => Self::Bytes(bytes),
            RawPayload::Payload { payload } => Self::Payload(payload),
        }
    }
}

/// Response returned by the signing host for a product-account signing request.
///
/// Decoded from [`v1::RemoteMessage::SignResponse`] while the runtime is waiting
/// for a matching SSO remote message id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningResponse {
    /// `message_id` of the signing request being answered.
    pub responding_to: String,
    /// Signing result, or an error description from the signing host.
    pub payload: Result<SigningPayloadResponseData, String>,
}

/// Successful product-account signing result returned by the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningPayloadResponseData {
    /// The cryptographic signature.
    pub signature: Vec<u8>,
    /// Full signed transaction, when the request asked for it.
    pub signed_transaction: Option<Vec<u8>>,
}

/// Response returned by the signing host for a legacy-account raw signing request.
///
/// Decoded from [`v1::RemoteMessage::SignRawLegacyResponse`] and mapped back to
/// the public raw-signing response shape.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignRawLegacyResponse {
    /// `message_id` of the legacy raw-signing request being answered.
    pub responding_to: String,
    /// Signature bytes, or an error description from the signing host.
    pub signature: Result<Vec<u8>, String>,
}

/// Failure returned by the Account Holder for a ring-VRF proof or alias request.
///
/// Mirrors the identical error sets of `Account::create_account_proof` and
/// `Account::get_account_alias` (RFC 0004): the two operations perform the same
/// ring resolution and member-key selection, so they share these failure modes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RingVrfError {
    /// The `RingLocation` did not resolve to a known ring.
    RingNotFound,
    /// The selected member key is not a member of the requested ring.
    NotMember,
    /// User or Account Holder rejected the request.
    Rejected,
    /// Catch-all failure, carrying a diagnostic reason.
    Unknown {
        /// Diagnostic failure description.
        reason: String,
    },
}

/// Request sent when a product asks the Account Holder for a contextual alias.
///
/// Used by `Account::get_account_alias`; `calling_product_id` names the caller
/// so the Account Holder can scope context derivation, while `context` and
/// `ring_location` select the member key and bind the derived alias (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasRequest {
    /// Product id of the calling product.
    pub calling_product_id: String,
    /// Context that scopes the derived alias.
    pub context: ProductProofContext,
    /// Ring whose member key derives the alias.
    pub ring_location: RingLocation,
}

/// Response returned by the Account Holder for a ring-VRF alias request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasResponse {
    /// `message_id` of the alias request being answered.
    pub responding_to: String,
    /// Derived alias, or the ring-VRF failure.
    pub payload: Result<HostAccountGetAliasResponse, RingVrfError>,
}

/// Request sent when a product asks the Account Holder for a ring-VRF proof.
///
/// Used by `Account::create_account_proof`; carries the same `(context,
/// ring_location)` as the alias request plus the opaque `message` bound into
/// the proof (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfProofRequest {
    /// Product id of the calling product.
    pub calling_product_id: String,
    /// Context that scopes the proof.
    pub context: ProductProofContext,
    /// Ring whose member key produces the proof.
    pub ring_location: RingLocation,
    /// Opaque message bound into the proof.
    pub message: Vec<u8>,
}

/// Response returned by the Account Holder for a ring-VRF proof request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfProofResponse {
    /// `message_id` of the proof request being answered.
    pub responding_to: String,
    /// Created proof, or the ring-VRF failure.
    pub payload: Result<HostAccountCreateProofResponse, RingVrfError>,
}

/// Request sent when a product asks the signing host to allocate SSO-backed
/// resources.
///
/// Used by `ResourceAllocation::request` for capabilities from
/// `docs/rfcs/0010-allowance.md`, such as statement-store allowance and
/// auto-signing material.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationRequest {
    /// Product id the allocation is requested for.
    pub calling_product_id: String,
    /// Resources to allocate; outcomes come back in the same order.
    pub resources: Vec<SsoAllocatableResource>,
    /// Policy applied when an allocation already exists for this product.
    pub on_existing: OnExistingAllowancePolicy,
}

/// Resources the signing host may allocate for the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatableResource {
    /// Statement Store slot allowance for the product's allowance account.
    StatementStoreAllowance,
    /// Bulletin chain slot allowance for the product's allowance account.
    BulletinAllowance,
    /// Pre-warmed PGAS balance for the smart-contract account at the given
    /// derivation index.
    SmartContractAllowance(u32),
    /// Transfer of the product subtree key so the host can sign locally.
    AutoSigning,
}

impl From<AllocatableResource> for SsoAllocatableResource {
    fn from(value: AllocatableResource) -> Self {
        match value {
            AllocatableResource::StatementStoreAllowance => Self::StatementStoreAllowance,
            AllocatableResource::BulletinAllowance => Self::BulletinAllowance,
            AllocatableResource::SmartContractAllowance(index) => {
                Self::SmartContractAllowance(index)
            }
            AllocatableResource::AutoSigning => Self::AutoSigning,
        }
    }
}

/// Signing-host policy for already-existing resource allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum OnExistingAllowancePolicy {
    /// Return the existing allocation unchanged; allocate only if none exists.
    Ignore,
    /// Assign one additional slot to the existing allowance account.
    Increase,
}

/// Response returned by the signing host for a resource-allocation request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationResponse {
    /// `message_id` of the allocation request being answered.
    pub responding_to: String,
    /// Per-resource outcomes in request order, or an error description.
    pub payload: Result<Vec<SsoAllocationOutcome>, String>,
}

/// Per-resource allocation result from the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocationOutcome {
    /// Resource granted, carrying the allocated material.
    Allocated(SsoAllocatedResource),
    /// User or signing host declined this resource.
    Rejected,
    /// Signing host cannot currently grant this resource.
    NotAvailable,
}

/// Resource material allocated by the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatedResource {
    /// Statement Store slot allowance material.
    StatementStoreAllowance {
        /// Private key of the allowance account assigned to the slot.
        slot_account_key: Vec<u8>,
    },
    /// Bulletin chain slot allowance material.
    BulletinAllowance {
        /// Private key of the allowance account assigned to the slot.
        slot_account_key: Vec<u8>,
    },
    /// Smart-contract allowance carries no key material.
    SmartContractAllowance,
    /// Auto-signing material for the product subtree.
    AutoSigning {
        /// Secret component of the per-product soft-derivation path.
        product_derivation_secret: String,
        /// Private key of the product subtree root.
        product_root_private_key: Vec<u8>,
    },
}

/// Request sent when a product asks the signing host to create a signed transaction
/// for a product-derived account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionRequest {
    /// Transaction payload to build and sign.
    pub payload: CreateTransactionPayload,
}

/// Versioned transaction-creation payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionPayload {
    /// Version 1 product-account payload.
    V1(ProductAccountTxPayload),
}

/// Request sent when a product asks the signing host to create a signed transaction
/// for a user-imported legacy account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionLegacyRequest {
    /// Transaction payload to build and sign.
    pub payload: CreateTransactionLegacyPayload,
}

/// Versioned legacy transaction-creation payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionLegacyPayload {
    /// Version 1 legacy-account payload.
    V1(LegacyAccountTxPayload),
}

/// Response returned by the signing host for either product-account or legacy-account
/// transaction creation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionResponse {
    /// `message_id` of the transaction-creation request being answered.
    pub responding_to: String,
    /// SCALE-encoded signed transaction, or an error description.
    pub signed_transaction: Result<Vec<u8>, String>,
}

/// Decoded inbound statement-channel outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoSessionStatement {
    /// The outbound request statement was acknowledged with a success code.
    RequestAccepted,
    /// Remote response matching the pending remote message id.
    RemoteResponse(SsoRemoteResponse),
    /// The peer ended the SSO session.
    Disconnected,
}

/// Signing-host response variants that can satisfy a pending remote request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoRemoteResponse {
    /// Product-account signing response.
    Sign(SigningResponse),
    /// Legacy-account raw-signing response.
    SignRawLegacy(SignRawLegacyResponse),
    /// Contextual-alias response.
    RingVrfAlias(RingVrfAliasResponse),
    /// Ring-VRF proof response.
    RingVrfProof(RingVrfProofResponse),
    /// Resource-allocation response.
    ResourceAllocation(ResourceAllocationResponse),
    /// Transaction-creation response.
    CreateTransaction(CreateTransactionResponse),
}

/// Decode and classify an inbound encrypted SSO session statement.
pub fn decode_sso_session_statement(
    session: &SsoSessionInfo,
    statement: &[u8],
    expected_statement_request_id: &str,
    expected_remote_message_id: &str,
) -> Result<Option<SsoSessionStatement>, String> {
    let verified =
        decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
    // Freshness gate against replay: a statement whose expiry is in the past
    // is ignored. Trusts the local clock.
    if verified
        .expiry
        .is_some_and(|expiry| statement_expiry_elapsed(expiry, current_unix_secs()))
    {
        return Ok(None);
    }
    let encrypted = verified.data;
    let data = decrypt_session_statement_data(session, &encrypted)?;
    if verified.signer == session.ss_public_key {
        return match data {
            SsoStatementData::Response {
                request_id,
                response_code,
            } if request_id == expected_statement_request_id => {
                classify_response_ack(request_id, response_code).map(Some)
            }
            _ => Ok(None),
        };
    }
    if verified.signer != session.identity_account_id {
        return Err("statement proof signer does not match expected peer".to_string());
    }
    match data {
        SsoStatementData::Response {
            request_id,
            response_code,
        } if request_id == expected_statement_request_id => {
            classify_response_ack(request_id, response_code).map(Some)
        }
        SsoStatementData::Response { .. } => Ok(None),
        SsoStatementData::Request { data, .. } => {
            for message in data {
                let message = RemoteMessage::decode(&mut message.as_slice())
                    .map_err(|err| format!("invalid SSO remote message: {err}"))?;
                if matches!(
                    &message.data,
                    RemoteMessageData::V1(v1::RemoteMessage::Disconnected)
                ) {
                    return Ok(Some(SsoSessionStatement::Disconnected));
                }
                if let Some(response) =
                    remote_response_for_message(message, expected_remote_message_id)
                {
                    return Ok(Some(SsoSessionStatement::RemoteResponse(response)));
                }
            }
            Ok(None)
        }
    }
}

fn classify_response_ack(
    request_id: String,
    response_code: u8,
) -> Result<SsoSessionStatement, String> {
    match SsoResponseCode::try_from(response_code) {
        Ok(SsoResponseCode::Success) => Ok(SsoSessionStatement::RequestAccepted),
        Ok(code) => Err(format!("SSO request {request_id} was rejected: {code}")),
        Err(()) => Err(format!("SSO request {request_id} was rejected: unknown")),
    }
}

fn remote_response_for_message(
    message: RemoteMessage,
    expected_remote_message_id: &str,
) -> Option<SsoRemoteResponse> {
    let RemoteMessageData::V1(data) = message.data;
    match data {
        v1::RemoteMessage::SignResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::Sign(response))
        }
        v1::RemoteMessage::RingVrfAliasResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::RingVrfAlias(response))
        }
        v1::RemoteMessage::RingVrfProofResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::RingVrfProof(response))
        }
        v1::RemoteMessage::SignRawLegacyResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::SignRawLegacy(response))
        }
        v1::RemoteMessage::ResourceAllocationResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::ResourceAllocation(response))
        }
        v1::RemoteMessage::CreateTransactionResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::CreateTransaction(response))
        }
        _ => None,
    }
}

/// Build a signing-host payload-signing request message.
pub fn sign_payload_message(message_id: String, request: HostSignPayloadRequest) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::SignRequest(Box::new(
            SigningRequest::Payload(Box::new(SigningPayloadRequest::from_host_request(request))),
        ))),
    }
}

/// Build a signing-host raw-signing request message.
pub fn sign_raw_message(message_id: String, request: HostSignRawRequest) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::SignRequest(Box::new(
            SigningRequest::Raw(SigningRawRequest::from_host_request(request)),
        ))),
    }
}

/// Build a signing-host legacy raw-signing request message.
pub fn sign_raw_legacy_message(
    message_id: String,
    account: AccountId,
    payload: RawPayload,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::SignRawLegacyRequest(
            SignRawLegacyRequest {
                account,
                data: payload.into(),
            },
        )),
    }
}

/// Build an Account Holder contextual-alias request message.
pub fn alias_request_message(
    message_id: String,
    calling_product_id: String,
    context: ProductProofContext,
    ring_location: RingLocation,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::RingVrfAliasRequest(
            RingVrfAliasRequest {
                calling_product_id,
                context,
                ring_location,
            },
        )),
    }
}

/// Build an Account Holder ring-VRF proof request message.
pub fn proof_request_message(
    message_id: String,
    calling_product_id: String,
    context: ProductProofContext,
    ring_location: RingLocation,
    message: Vec<u8>,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::RingVrfProofRequest(
            RingVrfProofRequest {
                calling_product_id,
                context,
                ring_location,
                message,
            },
        )),
    }
}

/// Build a signing-host resource-allocation request message.
pub fn resource_allocation_message(
    message_id: String,
    calling_product_id: String,
    resources: Vec<AllocatableResource>,
    on_existing: OnExistingAllowancePolicy,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationRequest(
            ResourceAllocationRequest {
                calling_product_id,
                resources: resources.into_iter().map(Into::into).collect(),
                on_existing,
            },
        )),
    }
}

/// Build a signing-host transaction-creation request message.
pub fn create_transaction_message(
    message_id: String,
    payload: ProductAccountTxPayload,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::CreateTransactionRequest(
            CreateTransactionRequest {
                payload: CreateTransactionPayload::V1(payload),
            },
        )),
    }
}

/// Build a signing-host legacy-account transaction-creation request message.
pub fn create_transaction_legacy_message(
    message_id: String,
    payload: LegacyAccountTxPayload,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::CreateTransactionLegacyRequest(
            CreateTransactionLegacyRequest {
                payload: CreateTransactionLegacyPayload::V1(payload),
            },
        )),
    }
}

/// Build a signed outbound SSO request statement with a random nonce.
pub fn build_outgoing_request_statement(
    session: &SsoSessionInfo,
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
    expiry: u64,
) -> Result<Vec<u8>, String> {
    let encrypted = encrypt_outgoing_request_data(session, statement_request_id, messages)?;
    build_signed_session_request_statement(session, encrypted, expiry)
}

/// Build a signed outbound SSO request statement with a caller-supplied nonce.
pub fn build_outgoing_request_statement_with_nonce(
    session: &SsoSessionInfo,
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
    expiry: u64,
    nonce: [u8; AES_GCM_NONCE_LEN],
) -> Result<Vec<u8>, String> {
    let encrypted =
        encrypt_outgoing_request_data_with_nonce(session, statement_request_id, messages, nonce)?;
    build_signed_session_request_statement(session, encrypted, expiry)
}

fn encrypt_outgoing_request_data(
    session: &SsoSessionInfo,
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
) -> Result<Vec<u8>, String> {
    encrypt_session_statement_data(
        session,
        &outgoing_request_data(statement_request_id, messages),
    )
}

fn encrypt_outgoing_request_data_with_nonce(
    session: &SsoSessionInfo,
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
    nonce: [u8; AES_GCM_NONCE_LEN],
) -> Result<Vec<u8>, String> {
    encrypt_session_statement_data_with_nonce(
        session,
        &outgoing_request_data(statement_request_id, messages),
        nonce,
    )
}

fn outgoing_request_data(
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
) -> SsoStatementData {
    SsoStatementData::Request {
        request_id: statement_request_id,
        data: messages
            .into_iter()
            .map(|message| message.encode())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::sso::pairing::decrypt_session_statement_data;
    use crate::host_logic::statement_store::{
        StatementField, build_signed_statement, decode_statement_data,
    };
    use p256::SecretKey as P256SecretKey;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use schnorrkel::{ExpansionMode, MiniSecretKey};
    use truapi::latest::{HostSignPayloadData, TxPayloadExtension};

    fn account() -> ProductAccountId {
        ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: 7,
        }
    }

    fn fresh_expiry() -> u64 {
        (current_unix_secs() + 60) << 32
    }

    fn elapsed_expiry() -> u64 {
        (current_unix_secs() - 60) << 32
    }

    fn session() -> SsoSessionInfo {
        let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
        let core_secret = P256SecretKey::from_slice(&[1; 32]).unwrap();
        let peer_secret = P256SecretKey::from_slice(&[2; 32]).unwrap();
        SsoSessionInfo {
            ss_secret: keypair.secret.to_bytes(),
            ss_public_key: keypair.public.to_bytes(),
            enc_secret: core_secret.to_bytes().into(),
            peer_enc_pubkey: peer_secret
                .public_key()
                .to_encoded_point(false)
                .as_bytes()
                .try_into()
                .unwrap(),
            identity_account_id: [3; 32],
            session_id_own: [4; 32],
            session_id_peer: [5; 32],
            request_channel: [6; 32],
            response_channel: [7; 32],
            peer_request_channel: [8; 32],
        }
    }

    #[test]
    fn disconnected_message_matches_host_papp_variant_order() {
        let message = RemoteMessage {
            message_id: String::new(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };

        assert_eq!(message.encode(), vec![0, 0, 0]);
    }

    #[test]
    fn raw_sign_request_uses_remote_message_variant_indices() {
        let message = sign_raw_message(
            "m1".to_string(),
            HostSignRawRequest {
                account: account(),
                payload: RawPayload::Bytes {
                    bytes: vec![0xde, 0xad],
                },
            },
        );
        let encoded = message.encode();

        assert_eq!(&encoded[..3], &[8, b'm', b'1']);
        assert_eq!(encoded[3], 0);
        assert_eq!(encoded[4], 1);
        assert_eq!(encoded[5], 1);
    }

    #[test]
    fn late_remote_message_variants_match_host_papp_order() {
        let legacy_tx = create_transaction_legacy_message(
            String::new(),
            LegacyAccountTxPayload {
                signer: [1; 32],
                genesis_hash: [2; 32],
                call_data: Vec::new(),
                extensions: Vec::new(),
                tx_ext_version: 0,
            },
        )
        .encode();
        let legacy_raw =
            sign_raw_legacy_message(String::new(), [1; 32], RawPayload::Bytes { bytes: vec![] })
                .encode();

        assert_eq!(legacy_tx[..3], [0, 0, 9]);
        assert_eq!(legacy_raw[..3], [0, 0, 10]);
    }

    fn sequential_bytes<const N: usize>(start: u8) -> [u8; N] {
        std::array::from_fn(|index| start.wrapping_add(index as u8))
    }

    fn assert_host_papp_0_8_8_fixture(message: RemoteMessage, expected: &str) {
        assert_eq!(
            hex::encode(message.encode()),
            expected.trim_start_matches("0x")
        );
    }

    #[test]
    fn resource_allocation_message_matches_host_papp_0_8_8_fixture() {
        let message = resource_allocation_message(
            "m-resource".to_string(),
            "truapi-playground.dot".to_string(),
            vec![
                AllocatableResource::StatementStoreAllowance,
                AllocatableResource::BulletinAllowance,
                AllocatableResource::SmartContractAllowance(9),
                AllocatableResource::AutoSigning,
            ],
            OnExistingAllowancePolicy::Increase,
        );

        assert_host_papp_0_8_8_fixture(
            message,
            "0x286d2d7265736f757263650005547472756170692d706c617967726f756e642e646f7410000102090000000301",
        );
    }

    #[test]
    fn create_transaction_message_matches_host_papp_0_8_8_fixture() {
        let message = create_transaction_message(
            "m-product-tx".to_string(),
            ProductAccountTxPayload {
                signer: ProductAccountId {
                    dot_ns_identifier: "truapi-playground.dot".to_string(),
                    derivation_index: 0,
                },
                genesis_hash: sequential_bytes(32),
                call_data: vec![0, 0],
                extensions: vec![TxPayloadExtension {
                    id: "CheckNonce".to_string(),
                    extra: vec![1],
                    additional_signed: vec![2, 3],
                }],
                tx_ext_version: 0,
            },
        );

        assert_host_papp_0_8_8_fixture(
            message,
            "0x306d2d70726f647563742d7478000700547472756170692d706c617967726f756e642e646f7400000000202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f0800000428436865636b4e6f6e6365040108020300",
        );
    }

    #[test]
    fn playground_create_transaction_message_matches_host_papp_0_8_8_fixture() {
        let message = create_transaction_message(
            "create-transaction-1".to_string(),
            ProductAccountTxPayload {
                signer: ProductAccountId {
                    dot_ns_identifier: "truapi-playground.dot".to_string(),
                    derivation_index: 0,
                },
                genesis_hash: [
                    0xbf, 0x04, 0x88, 0xdb, 0xe9, 0xda, 0xa1, 0xde, 0x1c, 0x08, 0xc5, 0xf7, 0x43,
                    0xe2, 0x6f, 0xdc, 0x2a, 0x4e, 0xcd, 0x74, 0xcf, 0x87, 0xdd, 0x1b, 0x4b, 0x1e,
                    0xeb, 0x99, 0xae, 0x4e, 0xf1, 0x9f,
                ],
                call_data: vec![0, 0],
                extensions: vec![],
                tx_ext_version: 0,
            },
        );

        assert_host_papp_0_8_8_fixture(
            message,
            "0x506372656174652d7472616e73616374696f6e2d31000700547472756170692d706c617967726f756e642e646f7400000000bf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f0800000000",
        );
    }

    #[test]
    fn create_transaction_legacy_message_matches_host_papp_0_8_8_fixture() {
        let message = create_transaction_legacy_message(
            "m-legacy-tx".to_string(),
            LegacyAccountTxPayload {
                signer: sequential_bytes(0),
                genesis_hash: sequential_bytes(32),
                call_data: vec![0, 0],
                extensions: vec![TxPayloadExtension {
                    id: "CheckNonce".to_string(),
                    extra: vec![1],
                    additional_signed: vec![2, 3],
                }],
                tx_ext_version: 0,
            },
        );

        assert_host_papp_0_8_8_fixture(
            message,
            "0x2c6d2d6c65676163792d7478000900000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f0800000428436865636b4e6f6e6365040108020300",
        );
    }

    #[test]
    fn sign_raw_legacy_messages_match_host_papp_0_8_8_fixtures() {
        assert_host_papp_0_8_8_fixture(
            sign_raw_legacy_message(
                "m-legacy-raw".to_string(),
                sequential_bytes(0),
                RawPayload::Bytes {
                    bytes: b"Hi".to_vec(),
                },
            ),
            "0x306d2d6c65676163792d726177000a000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f00084869",
        );
        assert_host_papp_0_8_8_fixture(
            sign_raw_legacy_message(
                "m-legacy-raw-payload".to_string(),
                sequential_bytes(0),
                RawPayload::Payload {
                    payload: "<Bytes>Hi</Bytes>".to_string(),
                },
            ),
            "0x506d2d6c65676163792d7261772d7061796c6f6164000a000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f01443c42797465733e48693c2f42797465733e",
        );
    }

    #[test]
    fn option_bool_matches_host_papp_option_bool_encoding() {
        let mut request = HostSignPayloadRequest {
            account: account(),
            payload: HostSignPayloadData {
                block_hash: vec![],
                block_number: vec![],
                era: vec![],
                genesis_hash: vec![],
                method: vec![],
                nonce: vec![],
                spec_version: vec![],
                tip: vec![],
                transaction_version: vec![],
                signed_extensions: vec![],
                version: 4,
                asset_id: None,
                metadata_hash: None,
                mode: None,
                with_signed_transaction: Some(true),
            },
        };
        let true_encoded = SigningPayloadRequest::from_host_request(request.clone()).encode();
        request.payload.with_signed_transaction = Some(false);
        let false_encoded = SigningPayloadRequest::from_host_request(request.clone()).encode();
        request.payload.with_signed_transaction = None;
        let none_encoded = SigningPayloadRequest::from_host_request(request).encode();

        assert_eq!(true_encoded.last(), Some(&1));
        assert_eq!(false_encoded.last(), Some(&2));
        assert_eq!(none_encoded.last(), Some(&0));
    }

    #[test]
    fn maps_public_resource_names_to_sso_dialect() {
        let message = resource_allocation_message(
            "alloc".to_string(),
            "myapp.dot".to_string(),
            vec![
                AllocatableResource::StatementStoreAllowance,
                AllocatableResource::BulletinAllowance,
                AllocatableResource::SmartContractAllowance(9),
                AllocatableResource::AutoSigning,
            ],
            OnExistingAllowancePolicy::Increase,
        );
        let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationRequest(request)) =
            message.data
        else {
            panic!("expected resource allocation request");
        };

        assert_eq!(
            request.resources,
            vec![
                SsoAllocatableResource::StatementStoreAllowance,
                SsoAllocatableResource::BulletinAllowance,
                SsoAllocatableResource::SmartContractAllowance(9),
                SsoAllocatableResource::AutoSigning,
            ]
        );
        assert_eq!(request.on_existing, OnExistingAllowancePolicy::Increase);
    }

    #[test]
    fn builds_signed_encrypted_outgoing_request_statement() {
        let session = session();
        let remote_message = sign_raw_message(
            "remote-1".to_string(),
            HostSignRawRequest {
                account: account(),
                payload: RawPayload::Payload {
                    payload: "<Bytes>hello</Bytes>".to_string(),
                },
            },
        );

        let statement = build_outgoing_request_statement_with_nonce(
            &session,
            "statement-1".to_string(),
            vec![remote_message.clone()],
            99,
            [9; AES_GCM_NONCE_LEN],
        )
        .unwrap();
        let encrypted = decode_statement_data(&statement).unwrap();
        let decrypted = decrypt_session_statement_data(&session, &encrypted).unwrap();

        let SsoStatementData::Request { request_id, data } = decrypted else {
            panic!("expected request statement data");
        };
        assert_eq!(request_id, "statement-1");
        assert_eq!(data.len(), 1);
        assert_eq!(
            RemoteMessage::decode(&mut data[0].as_slice()).unwrap(),
            remote_message
        );

        let fields = Vec::<StatementField>::decode(&mut statement.as_slice()).unwrap();
        assert_eq!(fields[1], StatementField::Expiry(99));
        assert_eq!(fields[2], StatementField::Channel(session.request_channel));
        assert_eq!(fields[3], StatementField::Topic1(session.session_id_own));
    }

    #[test]
    fn ignores_own_echoed_session_request_statement() {
        let session = session();
        let remote_message = sign_raw_message(
            "remote-1".to_string(),
            HostSignRawRequest {
                account: account(),
                payload: RawPayload::Payload {
                    payload: "<Bytes>hello</Bytes>".to_string(),
                },
            },
        );
        let statement = build_outgoing_request_statement_with_nonce(
            &session,
            "statement-1".to_string(),
            vec![remote_message],
            fresh_expiry(),
            [9; AES_GCM_NONCE_LEN],
        )
        .unwrap();

        let decoded =
            decode_sso_session_statement(&session, &statement, "statement-1", "remote-1").unwrap();

        assert_eq!(decoded, None);
    }

    fn response_ack_statement(session: &SsoSessionInfo, expiry: u64) -> Vec<u8> {
        let encrypted = encrypt_session_statement_data_with_nonce(
            session,
            &SsoStatementData::Response {
                request_id: "statement-1".to_string(),
                response_code: 0,
            },
            [9; AES_GCM_NONCE_LEN],
        )
        .unwrap();
        build_signed_statement(
            session,
            session.response_channel,
            session.session_id_own,
            encrypted,
            expiry,
        )
        .unwrap()
    }

    #[test]
    fn accepts_own_echoed_session_response_ack() {
        let session = session();
        let statement = response_ack_statement(&session, fresh_expiry());

        let decoded =
            decode_sso_session_statement(&session, &statement, "statement-1", "remote-1").unwrap();

        assert_eq!(decoded, Some(SsoSessionStatement::RequestAccepted));
    }

    /// A statement whose expiry is in the past must be ignored even when it
    /// would otherwise match the pending request (replay protection).
    #[test]
    fn ignores_expired_session_response_ack() {
        let session = session();
        let statement = response_ack_statement(&session, elapsed_expiry());

        let decoded =
            decode_sso_session_statement(&session, &statement, "statement-1", "remote-1").unwrap();

        assert_eq!(decoded, None);
    }
}
