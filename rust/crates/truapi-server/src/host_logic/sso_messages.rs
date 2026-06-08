//! SCALE codecs for host-papp 0.7.9 SSO session-channel messages.
//!
//! These are the encrypted payloads carried inside statement-store
//! `SsoStatementData::Request` / `Response` frames.

use parity_scale_codec::{Decode, Encode, Error, Input, Output};
use truapi::v01;

use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso_pairing::{
    AES_GCM_NONCE_LEN, SsoStatementData, decrypt_session_statement_data,
    encrypt_session_statement_data, encrypt_session_statement_data_with_nonce,
};
use crate::host_logic::statement_store::{
    build_signed_session_request_statement, decode_statement_data,
};

const SSO_RESPONSE_CODE_SUCCESS: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteMessage {
    pub message_id: String,
    pub data: RemoteMessageData,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessageData {
    #[codec(index = 0)]
    V1(RemoteMessageV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessageV1 {
    #[codec(index = 0)]
    Disconnected,
    #[codec(index = 1)]
    SignRequest(SigningRequest),
    #[codec(index = 2)]
    SignResponse(SigningResponse),
    #[codec(index = 3)]
    RingVrfAliasRequest(RingVrfAliasRequest),
    #[codec(index = 4)]
    RingVrfAliasResponse(RingVrfAliasResponse),
    #[codec(index = 5)]
    ResourceAllocationRequest(ResourceAllocationRequest),
    #[codec(index = 6)]
    ResourceAllocationResponse(ResourceAllocationResponse),
    #[codec(index = 7)]
    CreateTransactionRequest(CreateTransactionRequest),
    #[codec(index = 8)]
    CreateTransactionResponse(CreateTransactionResponse),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRequest {
    #[codec(index = 0)]
    Payload(SigningPayloadRequest),
    #[codec(index = 1)]
    Raw(SigningRawRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningPayloadRequest {
    pub product_account_id: v01::ProductAccountId,
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

impl From<v01::HostSignPayloadRequest> for SigningPayloadRequest {
    fn from(value: v01::HostSignPayloadRequest) -> Self {
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
            with_signed_transaction: payload.with_signed_transaction.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningRawRequest {
    pub product_account_id: v01::ProductAccountId,
    pub data: SigningRawPayload,
}

impl From<v01::HostSignRawRequest> for SigningRawRequest {
    fn from(value: v01::HostSignRawRequest) -> Self {
        Self {
            product_account_id: value.account,
            data: value.payload.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionBool(pub Option<bool>);

impl From<Option<bool>> for OptionBool {
    fn from(value: Option<bool>) -> Self {
        Self(value)
    }
}

impl Encode for OptionBool {
    fn size_hint(&self) -> usize {
        1
    }

    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        dest.push_byte(match self.0 {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
    }
}

impl Decode for OptionBool {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        match u8::decode(input)? {
            0 => Ok(Self(None)),
            1 => Ok(Self(Some(false))),
            2 => Ok(Self(Some(true))),
            _ => Err("invalid OptionBool discriminant".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SigningRawPayload {
    #[codec(index = 0)]
    Bytes(Vec<u8>),
    #[codec(index = 1)]
    Payload(String),
}

impl From<v01::RawPayload> for SigningRawPayload {
    fn from(value: v01::RawPayload) -> Self {
        match value {
            v01::RawPayload::Bytes { bytes } => Self::Bytes(bytes),
            v01::RawPayload::Payload { payload } => Self::Payload(payload),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningResponse {
    pub responding_to: String,
    pub payload: Result<SigningPayloadResponseData, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SigningPayloadResponseData {
    pub signature: Vec<u8>,
    pub signed_transaction: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasRequest {
    pub product_account_id: v01::ProductAccountId,
    pub product_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingVrfAliasResponse {
    pub responding_to: String,
    pub payload: Result<v01::HostAccountGetAliasResponse, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationRequest {
    pub calling_product_id: String,
    pub resources: Vec<SsoAllocatableResource>,
    pub on_existing: OnExistingAllowancePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatableResource {
    #[codec(index = 0)]
    StatementStoreAllowance,
    #[codec(index = 1)]
    BulletInAllowance,
    #[codec(index = 2)]
    SmartContractAllowance(u32),
    #[codec(index = 3)]
    AutoSigning,
}

impl From<v01::AllocatableResource> for SsoAllocatableResource {
    fn from(value: v01::AllocatableResource) -> Self {
        match value {
            v01::AllocatableResource::StatementStoreAllowance => Self::StatementStoreAllowance,
            v01::AllocatableResource::BulletinAllowance => Self::BulletInAllowance,
            v01::AllocatableResource::SmartContractAllowance(index) => {
                Self::SmartContractAllowance(index)
            }
            v01::AllocatableResource::AutoSigning => Self::AutoSigning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum OnExistingAllowancePolicy {
    #[codec(index = 0)]
    Ignore,
    #[codec(index = 1)]
    Increase,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationResponse {
    pub responding_to: String,
    pub payload: Result<Vec<SsoAllocationOutcome>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocationOutcome {
    #[codec(index = 0)]
    Allocated(SsoAllocatedResource),
    #[codec(index = 1)]
    Rejected,
    #[codec(index = 2)]
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatedResource {
    #[codec(index = 0)]
    StatementStoreAllowance { slot_account_key: Vec<u8> },
    #[codec(index = 1)]
    BulletInAllowance { slot_account_key: Vec<u8> },
    #[codec(index = 2)]
    SmartContractAllowance,
    #[codec(index = 3)]
    AutoSigning {
        product_derivation_secret: String,
        product_root_private_key: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionRequest {
    pub payload: CreateTransactionPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionPayload {
    #[codec(index = 0)]
    V1(v01::ProductAccountTxPayload),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionResponse {
    pub responding_to: String,
    pub signed_transaction: Result<Vec<u8>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoSessionStatement {
    RequestAccepted,
    RemoteResponse(SsoRemoteResponse),
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoRemoteResponse {
    Sign(SigningResponse),
    RingVrfAlias(RingVrfAliasResponse),
    ResourceAllocation(ResourceAllocationResponse),
    CreateTransaction(CreateTransactionResponse),
}

pub fn decode_sso_session_statement(
    session: &SsoSessionInfo,
    statement: &[u8],
    expected_statement_request_id: &str,
    expected_remote_message_id: &str,
) -> Result<Option<SsoSessionStatement>, String> {
    let encrypted = decode_statement_data(statement).map_err(|err| err.to_string())?;
    let data = decrypt_session_statement_data(session, &encrypted)?;
    match data {
        SsoStatementData::Response {
            request_id,
            response_code,
        } if request_id == expected_statement_request_id => {
            if response_code == SSO_RESPONSE_CODE_SUCCESS {
                Ok(Some(SsoSessionStatement::RequestAccepted))
            } else {
                Err(format!(
                    "SSO request {request_id} was rejected: {}",
                    sso_response_code_name(response_code)
                ))
            }
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
        1 => "decodingFailed",
        2 => "decryptionFailed",
        3 => "unknown",
        _ => "unrecognized response code",
    }
}

pub fn sign_payload_message(
    message_id: String,
    request: v01::HostSignPayloadRequest,
) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::SignRequest(SigningRequest::Payload(
            request.into(),
        ))),
    }
}

pub fn sign_raw_message(message_id: String, request: v01::HostSignRawRequest) -> RemoteMessage {
    RemoteMessage {
        message_id,
        data: RemoteMessageData::V1(RemoteMessageV1::SignRequest(SigningRequest::Raw(
            request.into(),
        ))),
    }
}

pub fn alias_request_message(
    message_id: String,
    product_account_id: v01::ProductAccountId,
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

pub fn resource_allocation_message(
    message_id: String,
    calling_product_id: String,
    resources: Vec<v01::AllocatableResource>,
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

pub fn create_transaction_message(
    message_id: String,
    payload: v01::ProductAccountTxPayload,
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

pub fn build_outgoing_request_statement(
    session: &SsoSessionInfo,
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
    expiry: u64,
) -> Result<Vec<u8>, String> {
    let encrypted = encrypt_outgoing_request_data(session, statement_request_id, messages)?;
    build_signed_session_request_statement(session, encrypted, expiry)
}

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
    use crate::host_logic::sso_pairing::decrypt_session_statement_data;
    use crate::host_logic::statement_store::{StatementField, decode_statement_data};
    use p256::SecretKey as P256SecretKey;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use schnorrkel::{ExpansionMode, MiniSecretKey};

    fn account() -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: 7,
        }
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
            v01::HostSignRawRequest {
                account: account(),
                payload: v01::RawPayload::Bytes {
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
    fn option_bool_matches_nova_option_bool_encoding() {
        let mut request = v01::HostSignPayloadRequest {
            account: account(),
            payload: v01::HostSignPayloadData {
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
        let true_encoded = SigningPayloadRequest::from(request.clone()).encode();
        request.payload.with_signed_transaction = Some(false);
        let false_encoded = SigningPayloadRequest::from(request.clone()).encode();
        request.payload.with_signed_transaction = None;
        let none_encoded = SigningPayloadRequest::from(request).encode();

        assert_eq!(true_encoded.last(), Some(&2));
        assert_eq!(false_encoded.last(), Some(&1));
        assert_eq!(none_encoded.last(), Some(&0));
    }

    #[test]
    fn maps_public_resource_names_to_sso_dialect() {
        let message = resource_allocation_message(
            "alloc".to_string(),
            "myapp.dot".to_string(),
            vec![
                v01::AllocatableResource::StatementStoreAllowance,
                v01::AllocatableResource::BulletinAllowance,
                v01::AllocatableResource::SmartContractAllowance(9),
                v01::AllocatableResource::AutoSigning,
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
            v01::HostSignRawRequest {
                account: account(),
                payload: v01::RawPayload::Payload {
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
}
