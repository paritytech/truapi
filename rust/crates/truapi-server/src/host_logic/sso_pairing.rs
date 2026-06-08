//! SSO pairing bootstrap helpers.
//!
//! This module owns the byte shape of the QR/deeplink payload described in
//! `docs/design/host-contract-and-core-impl/H - sso-pairing-protocol.md`.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use blake2_rfc::blake2b::blake2b;
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, MiniSecretKey};
use sha2::Sha256;
use thiserror::Error;
use truapi_platform::{PairingDeeplinkScheme, RuntimeConfig};

use crate::host_logic::session::SsoSessionInfo;

const HANDSHAKE_TOPIC_SUFFIX: &[u8] = b"topic";
const MAX_P256_SECRET_ATTEMPTS: usize = 64;
const AES_GCM_NONCE_LEN: usize = 12;
const SESSION_PREFIX: &[u8] = b"session";
const PIN_SEPARATOR: &[u8] = b"/";
const REQUEST_CHANNEL_SUFFIX: &[u8] = b"request";
const RESPONSE_CHANNEL_SUFFIX: &[u8] = b"response";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingBootstrap {
    pub deeplink: String,
    pub topic: [u8; 32],
    pub statement_store_public_key: [u8; 32],
    pub statement_store_secret: [u8; 64],
    pub encryption_public_key: [u8; 65],
    pub encryption_secret_key: [u8; 32],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PairingBootstrapError {
    #[error("failed to generate random pairing material: {0}")]
    Random(String),
    #[error("failed to generate P-256 pairing key")]
    InvalidP256Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub enum HostHandshakeData {
    #[codec(index = 0)]
    V1 {
        statement_store_public_key: [u8; 32],
        encryption_public_key: [u8; 65],
        metadata: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum AppHandshakeData {
    #[codec(index = 0)]
    V1 {
        encrypted_message: Vec<u8>,
        public_key: [u8; 65],
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SsoHandshakeAnswerSensitiveData {
    pub shared_secret_derivation_key: [u8; 65],
    pub root_user_account_id: [u8; 32],
    pub identity_account_id: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoStatementData {
    #[codec(index = 0)]
    Request {
        request_id: String,
        data: Vec<Vec<u8>>,
    },
    #[codec(index = 1)]
    Response {
        request_id: String,
        response_code: u8,
    },
}

pub fn decode_app_handshake_data(blob: &[u8]) -> Result<AppHandshakeData, String> {
    let mut input = blob;
    let value: AppHandshakeData =
        Decode::decode(&mut input).map_err(|err| format!("invalid app handshake data: {err}"))?;
    if !input.is_empty() {
        return Err("invalid app handshake data: trailing bytes".to_string());
    }
    Ok(value)
}

pub fn decrypt_handshake_answer(
    core_encryption_secret_key: [u8; 32],
    wallet_ephemeral_public_key: [u8; 65],
    encrypted_message: &[u8],
) -> Result<SsoHandshakeAnswerSensitiveData, String> {
    let plaintext = decrypt_p256_hkdf_aes_gcm(
        core_encryption_secret_key,
        wallet_ephemeral_public_key,
        encrypted_message,
    )?;
    let mut input = plaintext.as_slice();
    let value = SsoHandshakeAnswerSensitiveData::decode(&mut input)
        .map_err(|err| format!("invalid SSO handshake answer: {err}"))?;
    if !input.is_empty() {
        return Err("invalid SSO handshake answer: trailing bytes".to_string());
    }
    Ok(value)
}

pub fn establish_sso_session_info(
    bootstrap: &PairingBootstrap,
    answer: &SsoHandshakeAnswerSensitiveData,
) -> Result<SsoSessionInfo, String> {
    let shared_secret = shared_secret(
        bootstrap.encryption_secret_key,
        answer.shared_secret_derivation_key,
    )?;
    let shared_secret_bytes: [u8; 32] = (*shared_secret.raw_secret_bytes()).into();
    let session_id_own = create_session_id(
        shared_secret_bytes,
        bootstrap.statement_store_public_key,
        answer.identity_account_id,
    );
    let session_id_peer = create_session_id(
        shared_secret_bytes,
        answer.identity_account_id,
        bootstrap.statement_store_public_key,
    );

    Ok(SsoSessionInfo {
        ss_secret: bootstrap.statement_store_secret,
        ss_public_key: bootstrap.statement_store_public_key,
        enc_secret: bootstrap.encryption_secret_key,
        peer_enc_pubkey: answer.shared_secret_derivation_key,
        identity_account_id: answer.identity_account_id,
        session_id_own,
        session_id_peer,
        request_channel: keyed_hash(session_id_own, REQUEST_CHANNEL_SUFFIX),
        response_channel: keyed_hash(session_id_own, RESPONSE_CHANNEL_SUFFIX),
        peer_request_channel: keyed_hash(session_id_peer, REQUEST_CHANNEL_SUFFIX),
    })
}

pub fn encrypt_session_statement_data(
    session: &SsoSessionInfo,
    data: &SsoStatementData,
) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; AES_GCM_NONCE_LEN];
    getrandom::getrandom(&mut nonce)
        .map_err(|err| format!("failed to generate AES-GCM nonce: {err}"))?;
    encrypt_session_statement_data_with_nonce(session, data, nonce)
}

pub fn encrypt_session_statement_data_with_nonce(
    session: &SsoSessionInfo,
    data: &SsoStatementData,
    nonce: [u8; AES_GCM_NONCE_LEN],
) -> Result<Vec<u8>, String> {
    let aes_key = session_aes_key(session)?;
    let cipher = Aes256Gcm::new_from_slice(&aes_key)
        .map_err(|err| format!("failed to initialize AES-GCM: {err}"))?;
    let mut encrypted = nonce.to_vec();
    encrypted.extend(
        cipher
            .encrypt(Nonce::from_slice(&nonce), data.encode().as_slice())
            .map_err(|err| format!("failed to encrypt SSO statement data: {err}"))?,
    );
    Ok(encrypted)
}

pub fn decrypt_session_statement_data(
    session: &SsoSessionInfo,
    encrypted_message: &[u8],
) -> Result<SsoStatementData, String> {
    let plaintext = decrypt_session_message(session, encrypted_message)?;
    let mut input = plaintext.as_slice();
    let data = SsoStatementData::decode(&mut input)
        .map_err(|err| format!("invalid SSO statement data: {err}"))?;
    if !input.is_empty() {
        return Err("invalid SSO statement data: trailing bytes".to_string());
    }
    Ok(data)
}

fn decrypt_p256_hkdf_aes_gcm(
    own_secret_key: [u8; 32],
    peer_public_key: [u8; 65],
    encrypted_message: &[u8],
) -> Result<Vec<u8>, String> {
    if encrypted_message.len() < AES_GCM_NONCE_LEN {
        return Err("encrypted SSO handshake answer is too short".to_string());
    }
    let shared_secret = shared_secret(own_secret_key, peer_public_key)?;
    let aes_key = aes_key_from_shared_secret(&shared_secret)?;

    decrypt_aes_gcm_with_key(aes_key, encrypted_message, "handshake answer")
}

fn decrypt_session_message(
    session: &SsoSessionInfo,
    encrypted_message: &[u8],
) -> Result<Vec<u8>, String> {
    decrypt_aes_gcm_with_key(
        session_aes_key(session)?,
        encrypted_message,
        "statement data",
    )
}

fn decrypt_aes_gcm_with_key(
    aes_key: [u8; 32],
    encrypted_message: &[u8],
    label: &str,
) -> Result<Vec<u8>, String> {
    if encrypted_message.len() < AES_GCM_NONCE_LEN {
        return Err(format!("encrypted SSO {label} is too short"));
    }
    let (nonce, ciphertext) = encrypted_message.split_at(AES_GCM_NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(&aes_key)
        .map_err(|err| format!("failed to initialize AES-GCM: {err}"))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|err| format!("failed to decrypt SSO {label}: {err}"))
}

fn session_aes_key(session: &SsoSessionInfo) -> Result<[u8; 32], String> {
    let shared_secret = shared_secret(session.enc_secret, session.peer_enc_pubkey)?;
    aes_key_from_shared_secret(&shared_secret)
}

fn aes_key_from_shared_secret(
    shared_secret: &p256::ecdh::SharedSecret,
) -> Result<[u8; 32], String> {
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret.raw_secret_bytes());
    let mut aes_key = [0u8; 32];
    hkdf.expand(&[], &mut aes_key)
        .map_err(|err| format!("failed to derive AES key: {err}"))?;
    Ok(aes_key)
}

fn shared_secret(
    own_secret_key: [u8; 32],
    peer_public_key: [u8; 65],
) -> Result<p256::ecdh::SharedSecret, String> {
    let secret = SecretKey::from_slice(&own_secret_key)
        .map_err(|err| format!("invalid P-256 secret key: {err}"))?;
    let peer_public = PublicKey::from_sec1_bytes(&peer_public_key)
        .map_err(|err| format!("invalid P-256 public key: {err}"))?;
    Ok(diffie_hellman(
        secret.to_nonzero_scalar(),
        peer_public.as_affine(),
    ))
}

fn create_session_id(
    shared_secret: [u8; 32],
    account_a: [u8; 32],
    account_b: [u8; 32],
) -> [u8; 32] {
    let mut message = Vec::with_capacity(SESSION_PREFIX.len() + 32 + 32 + 2);
    message.extend_from_slice(SESSION_PREFIX);
    message.extend_from_slice(&account_a);
    message.extend_from_slice(&account_b);
    message.extend_from_slice(PIN_SEPARATOR);
    message.extend_from_slice(PIN_SEPARATOR);
    keyed_hash(shared_secret, &message)
}

fn keyed_hash(key: [u8; 32], message: &[u8]) -> [u8; 32] {
    let digest = blake2b(32, &key, message);
    let mut output = [0u8; 32];
    output.copy_from_slice(digest.as_bytes());
    output
}

pub fn create_pairing_bootstrap(
    config: &RuntimeConfig,
) -> Result<PairingBootstrap, PairingBootstrapError> {
    let (statement_store_secret, statement_store_public_key) = generate_statement_store_keypair()?;
    let (encryption_secret_key, encryption_public_key) = generate_p256_keypair()?;
    let deeplink = build_pairing_deeplink(
        config.pairing_deeplink_scheme,
        statement_store_public_key,
        encryption_public_key,
        &config.host_metadata_url,
    );
    let topic = bootstrap_topic(statement_store_public_key, encryption_public_key);

    Ok(PairingBootstrap {
        deeplink,
        topic,
        statement_store_public_key,
        statement_store_secret,
        encryption_public_key,
        encryption_secret_key,
    })
}

pub fn build_pairing_deeplink(
    scheme: PairingDeeplinkScheme,
    statement_store_public_key: [u8; 32],
    encryption_public_key: [u8; 65],
    metadata: &str,
) -> String {
    let handshake = HostHandshakeData::V1 {
        statement_store_public_key,
        encryption_public_key,
        metadata: metadata.to_string(),
    };
    format!(
        "{}pair?handshake={}",
        deeplink_scheme_prefix(scheme),
        hex::encode(handshake.encode())
    )
}

pub fn bootstrap_topic(
    statement_store_public_key: [u8; 32],
    encryption_public_key: [u8; 65],
) -> [u8; 32] {
    let mut message =
        Vec::with_capacity(encryption_public_key.len() + HANDSHAKE_TOPIC_SUFFIX.len());
    message.extend_from_slice(&encryption_public_key);
    message.extend_from_slice(HANDSHAKE_TOPIC_SUFFIX);

    let digest = blake2b(32, &statement_store_public_key, &message);
    let mut topic = [0u8; 32];
    topic.copy_from_slice(digest.as_bytes());
    topic
}

fn deeplink_scheme_prefix(scheme: PairingDeeplinkScheme) -> &'static str {
    match scheme {
        PairingDeeplinkScheme::PolkadotApp => "polkadotapp://",
        PairingDeeplinkScheme::PolkadotAppDev => "polkadotappdev://",
    }
}

fn generate_statement_store_keypair() -> Result<([u8; 64], [u8; 32]), PairingBootstrapError> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|err| PairingBootstrapError::Random(err.to_string()))?;
    let mini_secret = MiniSecretKey::from_bytes(&seed)
        .map_err(|err| PairingBootstrapError::Random(err.to_string()))?;
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    Ok((keypair.secret.to_bytes(), keypair.public.to_bytes()))
}

fn generate_p256_keypair() -> Result<([u8; 32], [u8; 65]), PairingBootstrapError> {
    for _ in 0..MAX_P256_SECRET_ATTEMPTS {
        let mut candidate = [0u8; 32];
        getrandom::getrandom(&mut candidate)
            .map_err(|err| PairingBootstrapError::Random(err.to_string()))?;
        let Ok(secret) = SecretKey::from_slice(&candidate) else {
            continue;
        };
        let public = secret.public_key().to_encoded_point(false);
        let public = public.as_bytes();
        if public.len() != 65 {
            return Err(PairingBootstrapError::InvalidP256Secret);
        }
        let mut encryption_public_key = [0u8; 65];
        encryption_public_key.copy_from_slice(public);
        let mut encryption_secret_key = [0u8; 32];
        encryption_secret_key.copy_from_slice(secret.to_bytes().as_slice());
        return Ok((encryption_secret_key, encryption_public_key));
    }

    Err(PairingBootstrapError::InvalidP256Secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SS_PUBLIC: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];
    const ENC_PUBLIC: [u8; 65] = [
        0x04, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
        0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
        0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b,
        0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a,
        0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
    ];

    #[test]
    fn builds_v1_pairing_deeplink() {
        let deeplink = build_pairing_deeplink(
            PairingDeeplinkScheme::PolkadotApp,
            SS_PUBLIC,
            ENC_PUBLIC,
            "https://example.invalid/metadata.json",
        );

        assert_eq!(
            deeplink,
            "polkadotapp://pair?handshake=00000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f04000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f9468747470733a2f2f6578616d706c652e696e76616c69642f6d657461646174612e6a736f6e"
        );
    }

    #[test]
    fn builds_dev_pairing_deeplink() {
        let deeplink = build_pairing_deeplink(
            PairingDeeplinkScheme::PolkadotAppDev,
            SS_PUBLIC,
            ENC_PUBLIC,
            "https://example.invalid/metadata.json",
        );

        assert!(deeplink.starts_with("polkadotappdev://pair?handshake="));
    }

    #[test]
    fn derives_bootstrap_topic_vector() {
        assert_eq!(
            hex::encode(bootstrap_topic(SS_PUBLIC, ENC_PUBLIC)),
            "031c589833c39b1dfbe3c1304ced75fa7b0d841035db008e5b407bfadd2779a4"
        );
    }

    #[test]
    fn generated_bootstrap_uses_real_key_shapes() {
        let config = RuntimeConfig {
            product_label: "myapp".to_string(),
            product_id: "myapp.dot".to_string(),
            site_id: "test".to_string(),
            host_metadata_url: "https://example.invalid/metadata.json".to_string(),
            people_chain_genesis_hash: [0; 32],
            pairing_deeplink_scheme: PairingDeeplinkScheme::PolkadotApp,
        };

        let bootstrap = create_pairing_bootstrap(&config).unwrap();

        assert!(
            bootstrap
                .deeplink
                .starts_with("polkadotapp://pair?handshake=")
        );
        assert_eq!(bootstrap.encryption_public_key[0], 0x04);
        assert_eq!(
            bootstrap.topic,
            bootstrap_topic(
                bootstrap.statement_store_public_key,
                bootstrap.encryption_public_key
            )
        );
    }

    #[test]
    fn decodes_app_handshake_answer() {
        let answer = AppHandshakeData::V1 {
            encrypted_message: vec![0xde, 0xad],
            public_key: ENC_PUBLIC,
        };

        assert_eq!(decode_app_handshake_data(&answer.encode()).unwrap(), answer);
    }

    #[test]
    fn rejects_app_handshake_trailing_bytes() {
        let mut encoded = AppHandshakeData::V1 {
            encrypted_message: vec![0xde, 0xad],
            public_key: ENC_PUBLIC,
        }
        .encode();
        encoded.push(0);

        assert_eq!(
            decode_app_handshake_data(&encoded).unwrap_err(),
            "invalid app handshake data: trailing bytes"
        );
    }

    #[test]
    fn decrypts_handshake_answer() {
        let core_secret = SecretKey::from_slice(&[1; 32]).unwrap();
        let wallet_ephemeral_secret = SecretKey::from_slice(&[2; 32]).unwrap();
        let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
        let mut wallet_ephemeral_public_bytes = [0u8; 65];
        wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());

        let shared_secret = diffie_hellman(
            wallet_ephemeral_secret.to_nonzero_scalar(),
            core_secret.public_key().as_affine(),
        );
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.raw_secret_bytes());
        let mut aes_key = [0u8; 32];
        hkdf.expand(&[], &mut aes_key).unwrap();

        let sensitive = SsoHandshakeAnswerSensitiveData {
            shared_secret_derivation_key: ENC_PUBLIC,
            root_user_account_id: [7; 32],
            identity_account_id: [8; 32],
        };
        let nonce = [9u8; AES_GCM_NONCE_LEN];
        let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
        let mut encrypted = nonce.to_vec();
        encrypted.extend(
            cipher
                .encrypt(Nonce::from_slice(&nonce), sensitive.encode().as_slice())
                .unwrap(),
        );

        assert_eq!(
            decrypt_handshake_answer(
                core_secret.to_bytes().into(),
                wallet_ephemeral_public_bytes,
                &encrypted
            )
            .unwrap(),
            sensitive
        );
    }

    #[test]
    fn rejects_short_handshake_ciphertext() {
        assert_eq!(
            decrypt_handshake_answer([1; 32], ENC_PUBLIC, &[0; AES_GCM_NONCE_LEN - 1]).unwrap_err(),
            "encrypted SSO handshake answer is too short"
        );
    }

    #[test]
    fn establishes_session_ids_and_channels() {
        let core_secret = SecretKey::from_slice(&[1; 32]).unwrap();
        let core_public = core_secret.public_key().to_encoded_point(false);
        let mut core_public_bytes = [0u8; 65];
        core_public_bytes.copy_from_slice(core_public.as_bytes());
        let bootstrap = PairingBootstrap {
            deeplink: "polkadotapp://pair?handshake=00".to_string(),
            topic: [0x11; 32],
            statement_store_public_key: [0x22; 32],
            statement_store_secret: [0x33; 64],
            encryption_public_key: core_public_bytes,
            encryption_secret_key: [1; 32],
        };
        let peer_secret = SecretKey::from_slice(&[2; 32]).unwrap();
        let peer_public = peer_secret.public_key().to_encoded_point(false);
        let answer = SsoHandshakeAnswerSensitiveData {
            shared_secret_derivation_key: peer_public.as_bytes().try_into().unwrap(),
            root_user_account_id: [0x44; 32],
            identity_account_id: [0x55; 32],
        };

        let info = establish_sso_session_info(&bootstrap, &answer).unwrap();

        assert_eq!(info.ss_secret, [0x33; 64]);
        assert_eq!(info.ss_public_key, [0x22; 32]);
        assert_eq!(info.enc_secret, [1; 32]);
        assert_eq!(info.peer_enc_pubkey, answer.shared_secret_derivation_key);
        assert_eq!(info.identity_account_id, [0x55; 32]);
        assert_ne!(info.session_id_own, info.session_id_peer);
        assert_eq!(
            info.request_channel,
            keyed_hash(info.session_id_own, b"request")
        );
        assert_eq!(
            info.response_channel,
            keyed_hash(info.session_id_own, b"response")
        );
        assert_eq!(
            info.peer_request_channel,
            keyed_hash(info.session_id_peer, b"request")
        );
    }

    #[test]
    fn statement_data_codec_round_trips_request_and_response() {
        let request = SsoStatementData::Request {
            request_id: "req-1".to_string(),
            data: vec![vec![0xde, 0xad], vec![0xbe, 0xef]],
        };
        let response = SsoStatementData::Response {
            request_id: "req-1".to_string(),
            response_code: 0,
        };

        assert_eq!(
            SsoStatementData::decode(&mut &request.encode()[..]).unwrap(),
            request
        );
        assert_eq!(
            SsoStatementData::decode(&mut &response.encode()[..]).unwrap(),
            response
        );
        assert_eq!(request.encode()[0], 0);
        assert_eq!(response.encode()[0], 1);
    }

    #[test]
    fn encrypts_and_decrypts_session_statement_data() {
        let core_secret = SecretKey::from_slice(&[1; 32]).unwrap();
        let core_public = core_secret.public_key().to_encoded_point(false);
        let mut core_public_bytes = [0u8; 65];
        core_public_bytes.copy_from_slice(core_public.as_bytes());
        let bootstrap = PairingBootstrap {
            deeplink: "polkadotapp://pair?handshake=00".to_string(),
            topic: [0x11; 32],
            statement_store_public_key: [0x22; 32],
            statement_store_secret: [0x33; 64],
            encryption_public_key: core_public_bytes,
            encryption_secret_key: [1; 32],
        };
        let peer_secret = SecretKey::from_slice(&[2; 32]).unwrap();
        let answer = SsoHandshakeAnswerSensitiveData {
            shared_secret_derivation_key: peer_secret
                .public_key()
                .to_encoded_point(false)
                .as_bytes()
                .try_into()
                .unwrap(),
            root_user_account_id: [0x44; 32],
            identity_account_id: [0x55; 32],
        };
        let session = establish_sso_session_info(&bootstrap, &answer).unwrap();
        let data = SsoStatementData::Request {
            request_id: "req-1".to_string(),
            data: vec![vec![0xde, 0xad]],
        };
        let nonce = [9u8; AES_GCM_NONCE_LEN];

        let encrypted = encrypt_session_statement_data_with_nonce(&session, &data, nonce).unwrap();

        assert_eq!(&encrypted[..AES_GCM_NONCE_LEN], nonce);
        assert_eq!(
            decrypt_session_statement_data(&session, &encrypted).unwrap(),
            data
        );
    }
}
