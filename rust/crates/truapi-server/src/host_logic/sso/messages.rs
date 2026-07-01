//! SCALE codecs for host-papp SSO session-channel messages.
//!
//! These are the encrypted payloads carried inside statement-store
//! `SsoStatementData::Request` / `Response` frames.
//! The runtime builds them when forwarding TrUAPI account, signing, resource
//! allocation, and transaction requests to the paired wallet, then decodes the
//! wallet's responses while waiting on the SSO statement-store channels.
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
};
use crate::host_logic::statement_store::{
    build_signed_session_request_statement, current_unix_secs, decode_verified_statement_data,
    statement_expiry_elapsed,
};

const SSO_RESPONSE_CODE_SUCCESS: u8 = 0;

/// Top-level wallet remote message sent over the encrypted SSO channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteMessage {
    /// Correlation id used to match wallet responses to host requests.
    pub message_id: String,
    /// Versioned remote message body.
    pub data: RemoteMessageData,
}

/// Versioned remote message body.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessageData {
    V1(RemoteMessageV1),
}

/// v1 messages exchanged with the paired wallet over the encrypted SSO channel.
///
/// The variant order is part of the SCALE wire protocol used inside
/// statement-store session statements.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessageV1 {
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

/// Signing request flavor sent to the wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRequest {
    Payload(Box<SigningPayloadRequest>),
    Raw(SigningRawRequest),
}

/// Request sent when a product asks the paired wallet to sign a Substrate
/// payload with a product-derived account.
///
/// Built from [`HostSignPayloadRequest`] and wrapped in
/// [`RemoteMessageV1::SignRequest`] before being encrypted into an SSO session
/// statement.
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

fn signing_payload_request_from(value: HostSignPayloadRequest) -> SigningPayloadRequest {
    let payload = value.payload;
    SigningPayloadRequest {
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

/// Request sent when a product asks the paired wallet to sign raw bytes or a
/// string message with a product-derived account.
///
/// Built from [`HostSignRawRequest`] and wrapped in
/// [`RemoteMessageV1::SignRequest`] before being encrypted into an SSO session
/// statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningRawRequest {
    pub product_account_id: ProductAccountId,
    pub data: SigningRawPayload,
}

fn signing_raw_request_from(value: HostSignRawRequest) -> SigningRawRequest {
    SigningRawRequest {
        product_account_id: value.account,
        data: value.payload.into(),
    }
}

/// Request sent when a product asks the paired wallet to sign raw data with a
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

/// Response returned by the wallet for a product-account signing request.
///
/// Decoded from [`RemoteMessageV1::SignResponse`] while the runtime is waiting
/// for a matching SSO remote message id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningResponse {
    pub responding_to: String,
    pub payload: Result<SigningPayloadResponseData, String>,
}

/// Successful product-account signing result returned by the wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningPayloadResponseData {
    pub signature: Vec<u8>,
    pub signed_transaction: Option<Vec<u8>>,
}

/// Response returned by the wallet for a legacy-account raw signing request.
///
/// Decoded from [`RemoteMessageV1::SignRawLegacyResponse`] and mapped back to
/// the public raw-signing response shape.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignRawLegacyResponse {
    pub responding_to: String,
    pub signature: Result<Vec<u8>, String>,
}

/// Request sent when a product asks the wallet for a ring-VRF alias.
///
/// Used by `Account::get_account_alias`; the product account identifies the
/// alias target, while `product_id` identifies the caller that the wallet is
/// authorizing over the SSO channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasRequest {
    pub product_account_id: ProductAccountId,
    pub product_id: String,
}

/// Response returned by the wallet for a ring-VRF alias request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasResponse {
    pub responding_to: String,
    pub payload: Result<HostAccountGetAliasResponse, String>,
}

/// Request sent when a product asks the wallet to allocate SSO-backed
/// resources.
///
/// Used by `ResourceAllocation::request` for capabilities such as statement
/// store allowance and auto-signing material.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationRequest {
    pub calling_product_id: String,
    pub resources: Vec<SsoAllocatableResource>,
    pub on_existing: OnExistingAllowancePolicy,
}

/// Resources the wallet may allocate for the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatableResource {
    StatementStoreAllowance,
    BulletInAllowance,
    SmartContractAllowance(u32),
    AutoSigning,
}

impl From<AllocatableResource> for SsoAllocatableResource {
    fn from(value: AllocatableResource) -> Self {
        match value {
            AllocatableResource::StatementStoreAllowance => Self::StatementStoreAllowance,
            AllocatableResource::BulletinAllowance => Self::BulletInAllowance,
            AllocatableResource::SmartContractAllowance(index) => {
                Self::SmartContractAllowance(index)
            }
            AllocatableResource::AutoSigning => Self::AutoSigning,
        }
    }
}

/// Wallet policy for already-existing resource allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum OnExistingAllowancePolicy {
    Ignore,
    Increase,
}

/// Response returned by the wallet for a resource-allocation request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationResponse {
    pub responding_to: String,
    pub payload: Result<Vec<SsoAllocationOutcome>, String>,
}

/// Per-resource allocation result from the wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocationOutcome {
    Allocated(SsoAllocatedResource),
    Rejected,
    NotAvailable,
}

/// Resource material allocated by the wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatedResource {
    StatementStoreAllowance {
        slot_account_key: Vec<u8>,
    },
    BulletInAllowance {
        slot_account_key: Vec<u8>,
    },
    SmartContractAllowance,
    AutoSigning {
        product_derivation_secret: String,
        product_root_private_key: Vec<u8>,
    },
}

/// Request sent when a product asks the wallet to create a signed transaction
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

/// Request sent when a product asks the wallet to create a signed transaction
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

/// Response returned by the wallet for either product-account or legacy-account
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

/// Wallet response variants that can satisfy a pending remote request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoRemoteResponse {
    Sign(SigningResponse),
    SignRawLegacy(SignRawLegacyResponse),
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
                    RemoteMessageData::V1(RemoteMessageV1::Disconnected)
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
    if response_code == SSO_RESPONSE_CODE_SUCCESS {
        Ok(SsoSessionStatement::RequestAccepted)
    } else {
        Err(format!(
            "SSO request {request_id} was rejected: {}",
            sso_response_code_name(response_code)
        ))
    }
}

fn remote_response_for_message(
    message: RemoteMessage,
    expected_remote_message_id: &str,
) -> Option<SsoRemoteResponse> {
    let RemoteMessageData::V1(data) = message.data;
    match data {
        RemoteMessageV1::SignResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::Sign(response))
        }
        RemoteMessageV1::RingVrfAliasResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::RingVrfAlias(response))
        }
        RemoteMessageV1::SignRawLegacyResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::SignRawLegacy(response))
        }
        RemoteMessageV1::ResourceAllocationResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::ResourceAllocation(response))
        }
        RemoteMessageV1::CreateTransactionResponse(response)
            if response.responding_to == expected_remote_message_id =>
        {
            Some(SsoRemoteResponse::CreateTransaction(response))
        }
        _ => None,
    }
}

fn sso_response_code_name(code: u8) -> &'static str {
    match code {
        1 => "decryptionFailed",
        2 => "decodingFailed",
        _ => "unknown",
    }
}

/// Build a wallet payload-signing request message.
pub fn sign_payload_message(message_id: String, request: HostSignPayloadRequest) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::SignRequest(Box::new(
            SigningRequest::Payload(Box::new(signing_payload_request_from(request))),
        ))),
    }
}

/// Build a wallet raw-signing request message.
pub fn sign_raw_message(message_id: String, request: HostSignRawRequest) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::SignRequest(Box::new(SigningRequest::Raw(
            signing_raw_request_from(request),
        )))),
    }
}

/// Build a wallet legacy raw-signing request message.
pub fn sign_raw_legacy_message(
    message_id: String,
    account: AccountId,
    payload: RawPayload,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::SignRawLegacyRequest(
            SignRawLegacyRequest {
                account,
                data: payload.into(),
            },
        )),
    }
}

/// Build a wallet account-alias request message.
pub fn alias_request_message(
    message_id: String,
    product_account_id: ProductAccountId,
    product_id: String,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::RingVrfAliasRequest(RingVrfAliasRequest {
            product_account_id,
            product_id,
        })),
    }
}

/// Build a wallet resource-allocation request message.
pub fn resource_allocation_message(
    message_id: String,
    calling_product_id: String,
    resources: Vec<AllocatableResource>,
    on_existing: OnExistingAllowancePolicy,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::ResourceAllocationRequest(
            ResourceAllocationRequest {
                calling_product_id,
                resources: resources.into_iter().map(Into::into).collect(),
                on_existing,
            },
        )),
    }
}

/// Build a wallet transaction-creation request message.
pub fn create_transaction_message(
    message_id: String,
    payload: ProductAccountTxPayload,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::CreateTransactionRequest(
            CreateTransactionRequest {
                payload: CreateTransactionPayload::V1(payload),
            },
        )),
    }
}

/// Build a wallet legacy-account transaction-creation request message.
pub fn create_transaction_legacy_message(
    message_id: String,
    payload: LegacyAccountTxPayload,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::CreateTransactionLegacyRequest(
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
    use truapi::latest::HostSignPayloadData;
    use truapi::v01;

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
            data: RemoteMessageData::V1(RemoteMessageV1::Disconnected),
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
            v01::LegacyAccountTxPayload {
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
            v01::ProductAccountTxPayload {
                signer: v01::ProductAccountId {
                    dot_ns_identifier: "truapi-playground.dot".to_string(),
                    derivation_index: 0,
                },
                genesis_hash: sequential_bytes(32),
                call_data: vec![0, 0],
                extensions: vec![v01::TxPayloadExtension {
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
            v01::ProductAccountTxPayload {
                signer: v01::ProductAccountId {
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
            v01::LegacyAccountTxPayload {
                signer: sequential_bytes(0),
                genesis_hash: sequential_bytes(32),
                call_data: vec![0, 0],
                extensions: vec![v01::TxPayloadExtension {
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
        let true_encoded = signing_payload_request_from(request.clone()).encode();
        request.payload.with_signed_transaction = Some(false);
        let false_encoded = signing_payload_request_from(request.clone()).encode();
        request.payload.with_signed_transaction = None;
        let none_encoded = signing_payload_request_from(request).encode();

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
        let RemoteMessageData::V1(RemoteMessageV1::ResourceAllocationRequest(request)) =
            message.data
        else {
            panic!("expected resource allocation request");
        };

        assert_eq!(
            request.resources,
            vec![
                SsoAllocatableResource::StatementStoreAllowance,
                SsoAllocatableResource::BulletInAllowance,
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
                response_code: SSO_RESPONSE_CODE_SUCCESS,
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
