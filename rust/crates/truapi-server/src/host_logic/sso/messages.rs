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
    AccountId, AllocatableResource, HostAccountGetAliasResponse, HostSignPayloadRequest,
    HostSignRawRequest, LegacyAccountTxPayload, ProductAccountId, ProductAccountTxPayload,
    RawPayload,
};

use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::pairing::{
    AES_GCM_NONCE_LEN, SsoStatementData, decrypt_session_statement_data,
    encrypt_session_statement_data, encrypt_session_statement_data_with_nonce,
    peer_response_channel,
};
use crate::host_logic::statement_store::{
    build_signed_session_request_statement, build_signed_statement, current_unix_secs,
    decode_verified_statement_data, statement_expiry_elapsed,
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
    V1(v1::RemoteMessage),
}

/// Signing request flavor sent to the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRequest {
    Payload(Box<SigningPayloadRequest>),
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
    pub product_account_id: ProductAccountId,
    pub block_hash: Vec<u8>,
    pub block_number: Vec<u8>,
    pub era: Vec<u8>,
    pub genesis_hash: Vec<u8>,
    pub method: Vec<u8>,
    pub nonce: Vec<u8>,
    pub spec_version: Vec<u8>,
    pub tip: Vec<u8>,
    pub transaction_version: Vec<u8>,
    pub signed_extensions: Vec<String>,
    pub version: u32,
    pub asset_id: Option<Vec<u8>>,
    pub metadata_hash: Option<Vec<u8>>,
    pub mode: Option<u32>,
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

impl From<SigningPayloadRequest> for truapi::v01::HostSignPayloadRequest {
    fn from(value: SigningPayloadRequest) -> Self {
        Self {
            account: value.product_account_id,
            payload: truapi::v01::HostSignPayloadData {
                block_hash: value.block_hash,
                block_number: value.block_number,
                era: value.era,
                genesis_hash: value.genesis_hash,
                method: value.method,
                nonce: value.nonce,
                spec_version: value.spec_version,
                tip: value.tip,
                transaction_version: value.transaction_version,
                signed_extensions: value.signed_extensions,
                version: value.version,
                asset_id: value.asset_id,
                metadata_hash: value.metadata_hash,
                mode: value.mode,
                with_signed_transaction: value.with_signed_transaction.0,
            },
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
    pub product_account_id: ProductAccountId,
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
    pub account: AccountId,
    pub data: SigningRawPayload,
}

/// Raw data accepted by SSO signing requests.
///
/// Used by both product-account raw signing and legacy-account raw signing to
/// distinguish binary payloads from string messages on the session-channel
/// wire.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRawPayload {
    Bytes(Vec<u8>),
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

impl From<SigningRawPayload> for RawPayload {
    fn from(value: SigningRawPayload) -> Self {
        match value {
            SigningRawPayload::Bytes(bytes) => Self::Bytes { bytes },
            SigningRawPayload::Payload(payload) => Self::Payload { payload },
        }
    }
}

impl From<SigningRawRequest> for truapi::v01::HostSignRawRequest {
    fn from(value: SigningRawRequest) -> Self {
        Self {
            account: value.product_account_id,
            payload: value.data.into(),
        }
    }
}

/// Response returned by the signing host for a product-account signing request.
///
/// Decoded from [`v1::RemoteMessage::SignResponse`] while the runtime is waiting
/// for a matching SSO remote message id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningResponse {
    pub responding_to: String,
    pub payload: Result<SigningPayloadResponseData, String>,
}

/// Successful product-account signing result returned by the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningPayloadResponseData {
    pub signature: Vec<u8>,
    pub signed_transaction: Option<Vec<u8>>,
}

/// Response returned by the signing host for a legacy-account raw signing request.
///
/// Decoded from [`v1::RemoteMessage::SignRawLegacyResponse`] and mapped back to
/// the public raw-signing response shape.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignRawLegacyResponse {
    pub responding_to: String,
    pub signature: Result<Vec<u8>, String>,
}

/// Request sent when a product asks the paired signing host to sign exact
/// statement-store proof bytes with a product-derived account.
///
/// This cannot reuse raw signing: raw-signing requests apply the public
/// `<Bytes>...</Bytes>` payload convention, while statement proofs sign the
/// unsigned statement payload bytes directly.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StatementStoreProductSignRequest {
    pub product_account_id: ProductAccountId,
    pub payload: Vec<u8>,
}

/// Response returned by the signing host for exact statement-store proof
/// signing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StatementStoreProductSignResponse {
    pub responding_to: String,
    pub signature: Result<Vec<u8>, String>,
}

/// Request sent when a product asks the signing host for a ring-VRF alias.
///
/// Used by `Account::get_account_alias`; the product account identifies the
/// alias target, while `product_id` identifies the caller that the signing host is
/// authorizing over the SSO channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasRequest {
    pub product_account_id: ProductAccountId,
    pub product_id: String,
}

/// Response returned by the signing host for a ring-VRF alias request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasResponse {
    pub responding_to: String,
    pub payload: Result<HostAccountGetAliasResponse, String>,
}

/// Request sent when a product asks the signing host to allocate SSO-backed
/// resources.
///
/// Used by `ResourceAllocation::request` for capabilities from
/// `docs/rfcs/0010-allowance.md`, such as statement-store allowance and
/// auto-signing material.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationRequest {
    pub calling_product_id: String,
    pub resources: Vec<SsoAllocatableResource>,
    pub on_existing: OnExistingAllowancePolicy,
}

/// Resources the signing host may allocate for the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatableResource {
    StatementStoreAllowance,
    BulletinAllowance,
    SmartContractAllowance(u32),
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
    Ignore,
    Increase,
}

/// Response returned by the signing host for a resource-allocation request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationResponse {
    pub responding_to: String,
    pub payload: Result<Vec<SsoAllocationOutcome>, String>,
}

/// Per-resource allocation result from the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocationOutcome {
    Allocated(SsoAllocatedResource),
    Rejected,
    NotAvailable,
}

/// Resource material allocated by the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatedResource {
    StatementStoreAllowance {
        slot_account_key: Vec<u8>,
    },
    BulletinAllowance {
        slot_account_key: Vec<u8>,
    },
    SmartContractAllowance,
    AutoSigning {
        product_derivation_secret: String,
        product_root_private_key: Vec<u8>,
    },
}

/// Request sent when a product asks the signing host to create a signed transaction
/// for a product-derived account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionRequest {
    pub payload: CreateTransactionPayload,
}

/// Versioned transaction-creation payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionPayload {
    V1(ProductAccountTxPayload),
}

/// Request sent when a product asks the signing host to create a signed transaction
/// for a user-imported legacy account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionLegacyRequest {
    pub payload: CreateTransactionLegacyPayload,
}

/// Versioned legacy transaction-creation payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionLegacyPayload {
    V1(LegacyAccountTxPayload),
}

/// Response returned by the signing host for either product-account or legacy-account
/// transaction creation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionResponse {
    pub responding_to: String,
    pub signed_transaction: Result<Vec<u8>, String>,
}

/// Decoded inbound statement-channel outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoSessionStatement {
    RequestAccepted,
    RemoteResponse(SsoRemoteResponse),
    Disconnected,
}

/// Signing-host response variants that can satisfy a pending remote request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoRemoteResponse {
    Sign(SigningResponse),
    SignRawLegacy(SignRawLegacyResponse),
    StatementStoreProductSign(StatementStoreProductSignResponse),
    RingVrfAlias(RingVrfAliasResponse),
    ResourceAllocation(ResourceAllocationResponse),
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
        v1::RemoteMessage::SignRawLegacyResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::SignRawLegacy(response))
        }
        v1::RemoteMessage::StatementStoreProductSignResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::StatementStoreProductSign(response))
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

/// Build a signing-host exact statement-store proof signing request message.
pub fn statement_store_product_sign_message(
    message_id: String,
    product_account_id: ProductAccountId,
    payload: Vec<u8>,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::StatementStoreProductSignRequest(
            StatementStoreProductSignRequest {
                product_account_id,
                payload,
            },
        )),
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

/// Build a signing-host account-alias request message.
pub fn alias_request_message(
    message_id: String,
    product_account_id: ProductAccountId,
    product_id: String,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(v1::RemoteMessage::RingVrfAliasRequest(
            RingVrfAliasRequest {
                product_account_id,
                product_id,
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

/// Inbound request decoded from a peer-signed session statement.
///
/// `request_id` identifies the statement for the transport-level ack;
/// `messages` are the application messages batched into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingSsoRequest {
    pub request_id: String,
    pub messages: Vec<RemoteMessage>,
}

/// Decode an inbound session statement into the peer's request batch.
///
/// Returns `Ok(None)` for statements that carry no peer request: own echoes,
/// transport-level acks, and expired statements. Used by the signing-host
/// responder, which serves peer requests instead of matching pending ones.
pub fn decode_incoming_sso_request(
    session: &SsoSessionInfo,
    statement: &[u8],
) -> Result<Option<IncomingSsoRequest>, String> {
    let verified =
        decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
    if verified.signer == session.ss_public_key {
        return Ok(None);
    }
    if verified.signer != session.identity_account_id {
        return Err("statement proof signer does not match expected peer".to_string());
    }
    if verified
        .expiry
        .is_some_and(|expiry| statement_expiry_elapsed(expiry, current_unix_secs()))
    {
        return Ok(None);
    }
    match decrypt_session_statement_data(session, &verified.data)? {
        SsoStatementData::Response { .. } => Ok(None),
        SsoStatementData::Request { request_id, data } => {
            let messages = data
                .iter()
                .map(|message| {
                    RemoteMessage::decode(&mut message.as_slice())
                        .map_err(|err| format!("invalid SSO remote message: {err}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(IncomingSsoRequest {
                request_id,
                messages,
            }))
        }
    }
}

/// Build the signed transport-level ack for a peer-initiated request
/// statement: topic `session_id_peer`, channel [`peer_response_channel`].
pub fn build_signed_session_response_statement(
    session: &SsoSessionInfo,
    request_id: String,
    response_code: u8,
    expiry: u64,
) -> Result<Vec<u8>, String> {
    let encrypted = encrypt_session_statement_data(
        session,
        &SsoStatementData::Response {
            request_id,
            response_code,
        },
    )?;
    build_signed_statement(
        session,
        peer_response_channel(session),
        session.session_id_peer,
        encrypted,
        expiry,
    )
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

    fn host_and_responder_sessions() -> (SsoSessionInfo, SsoSessionInfo) {
        use crate::host_logic::sso::pairing::{
            ResponderIdentity, create_pairing_bootstrap, derive_p256_keypair_from_entropy,
            establish_responder_session_info, establish_sso_session_info,
        };
        use truapi_platform::{HostInfo, PairingHostConfig, PlatformInfo};

        let config = PairingHostConfig::new(
            HostInfo {
                name: "Test Host".to_string(),
                icon: None,
                version: None,
            },
            PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
            "polkadotapp".to_string(),
        )
        .expect("test pairing config is valid");
        let bootstrap = create_pairing_bootstrap(&config).unwrap();
        let statement_keypair = MiniSecretKey::from_bytes(&[7; 32])
            .unwrap()
            .expand_to_keypair(ExpansionMode::Ed25519);
        let (encryption_secret_key, encryption_public_key) =
            derive_p256_keypair_from_entropy(&[0xAB; 16], b"sso-encryption").unwrap();
        let responder = ResponderIdentity {
            statement_secret: statement_keypair.secret.to_bytes(),
            statement_public_key: statement_keypair.public.to_bytes(),
            encryption_secret_key,
            encryption_public_key,
        };
        let responder_session = establish_responder_session_info(
            &responder,
            bootstrap.statement_store_public_key,
            bootstrap.encryption_public_key,
        )
        .unwrap();
        let host_session = establish_sso_session_info(
            &bootstrap,
            responder.statement_public_key,
            responder.encryption_public_key,
        )
        .unwrap();
        (host_session, responder_session)
    }

    /// A host-built request statement decodes on the responder side into the
    /// batched remote messages, and the responder's ack plus response
    /// statements resolve the host's pending wait.
    #[test]
    fn host_request_round_trips_through_responder_statements() {
        let (host_session, responder_session) = host_and_responder_sessions();
        let request = sign_raw_message(
            "remote-1".to_string(),
            HostSignRawRequest {
                account: account(),
                payload: RawPayload::Payload {
                    payload: "<Bytes>hello</Bytes>".to_string(),
                },
            },
        );
        let host_statement = build_outgoing_request_statement(
            &host_session,
            "statement-1".to_string(),
            vec![request.clone()],
            fresh_expiry(),
        )
        .unwrap();

        let incoming = decode_incoming_sso_request(&responder_session, &host_statement)
            .unwrap()
            .expect("responder should surface the host request");
        assert_eq!(
            incoming,
            IncomingSsoRequest {
                request_id: "statement-1".to_string(),
                messages: vec![request],
            }
        );

        let ack = build_signed_session_response_statement(
            &responder_session,
            incoming.request_id.clone(),
            0,
            fresh_expiry(),
        )
        .unwrap();
        assert_eq!(
            decode_sso_session_statement(&host_session, &ack, "statement-1", "remote-1").unwrap(),
            Some(SsoSessionStatement::RequestAccepted)
        );

        let response = RemoteMessage {
            message_id: "resp-1".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::SignResponse(SigningResponse {
                responding_to: "remote-1".to_string(),
                payload: Ok(SigningPayloadResponseData {
                    signature: vec![9; 64],
                    signed_transaction: None,
                }),
            })),
        };
        let response_statement = build_outgoing_request_statement(
            &responder_session,
            "resp-statement-1".to_string(),
            vec![response],
            fresh_expiry(),
        )
        .unwrap();
        let decoded = decode_sso_session_statement(
            &host_session,
            &response_statement,
            "statement-1",
            "remote-1",
        )
        .unwrap();
        assert_eq!(
            decoded,
            Some(SsoSessionStatement::RemoteResponse(
                SsoRemoteResponse::Sign(SigningResponse {
                    responding_to: "remote-1".to_string(),
                    payload: Ok(SigningPayloadResponseData {
                        signature: vec![9; 64],
                        signed_transaction: None,
                    }),
                })
            ))
        );
    }

    #[test]
    fn responder_ignores_own_echo_and_transport_acks() {
        let (host_session, responder_session) = host_and_responder_sessions();
        let own_statement = build_outgoing_request_statement(
            &responder_session,
            "resp-statement-1".to_string(),
            vec![RemoteMessage {
                message_id: "resp-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
            }],
            fresh_expiry(),
        )
        .unwrap();
        assert_eq!(
            decode_incoming_sso_request(&responder_session, &own_statement).unwrap(),
            None
        );

        let host_ack = build_signed_session_response_statement(
            &host_session,
            "resp-statement-1".to_string(),
            0,
            fresh_expiry(),
        )
        .unwrap();
        assert_eq!(
            decode_incoming_sso_request(&responder_session, &host_ack).unwrap(),
            None
        );
    }

    #[test]
    fn responder_ignores_expired_host_request() {
        let (host_session, responder_session) = host_and_responder_sessions();
        let stale_statement = build_outgoing_request_statement(
            &host_session,
            "statement-1".to_string(),
            vec![RemoteMessage {
                message_id: "remote-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
            }],
            elapsed_expiry(),
        )
        .unwrap();

        assert_eq!(
            decode_incoming_sso_request(&responder_session, &stale_statement).unwrap(),
            None
        );
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
