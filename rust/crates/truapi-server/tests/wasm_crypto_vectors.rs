#![cfg(target_arch = "wasm32")]

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use p256::SecretKey;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, MiniSecretKey};
use sha2::Sha256;
use truapi_platform::PairingDeeplinkScheme;
use truapi_server::host_logic::entropy::derive_product_entropy;
use truapi_server::host_logic::product_account::{
    derive_product_public_key, product_public_key_to_address,
};
use truapi_server::host_logic::session::SsoSessionInfo;
use truapi_server::host_logic::sso_pairing::{
    AES_GCM_NONCE_LEN, AppHandshakeData, PairingBootstrap, SsoHandshakeAnswerSensitiveData,
    SsoStatementData, bootstrap_topic, build_pairing_deeplink, decode_app_handshake_data,
    decrypt_handshake_answer, decrypt_session_statement_data,
    encrypt_session_statement_data_with_nonce, establish_sso_session_info,
};
use truapi_server::host_logic::statement_store::{
    build_signed_session_request_statement, decode_verified_statement_data,
};
use wasm_bindgen_test::wasm_bindgen_test;

const ROOT_PUBLIC_KEY: [u8; 32] = [
    0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d, 0xb8, 0xf5,
    0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab, 0xc1, 0xd7, 0x20, 0x23,
];

const SS_PUBLIC: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

const ENC_PUBLIC: [u8; 65] = [
    0x04, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
    0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
    0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
    0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e,
    0x3f,
];

fn entropy_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    for (i, byte) in secret.iter_mut().enumerate() {
        *byte = i as u8;
    }
    secret
}

fn statement_session() -> SsoSessionInfo {
    let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    SsoSessionInfo {
        ss_secret: keypair.secret.to_bytes(),
        ss_public_key: keypair.public.to_bytes(),
        enc_secret: [1; 32],
        peer_enc_pubkey: [2; 65],
        identity_account_id: [3; 32],
        session_id_own: [4; 32],
        session_id_peer: [5; 32],
        request_channel: [6; 32],
        response_channel: [7; 32],
        peer_request_channel: [8; 32],
    }
}

fn sso_session() -> SsoSessionInfo {
    let core_secret = SecretKey::from_slice(&[1; 32]).unwrap();
    let core_public = core_secret.public_key().to_encoded_point(false);
    let bootstrap = PairingBootstrap {
        deeplink: "polkadotapp://pair?handshake=00".to_string(),
        topic: [0x11; 32],
        statement_store_public_key: [0x22; 32],
        statement_store_secret: [0x33; 64],
        encryption_public_key: core_public.as_bytes().try_into().unwrap(),
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

    establish_sso_session_info(&bootstrap, &answer).unwrap()
}

#[wasm_bindgen_test]
fn product_account_and_entropy_vectors_match_dotli() {
    let derived = derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", 0).unwrap();
    assert_eq!(
        hex::encode(derived),
        "281489e3dd1c4dbe88cd670a59edcc9c44d64f510d302bd527ec306f10292f08"
    );
    assert_eq!(
        product_public_key_to_address(derived),
        "5CyFsdhwjXy7wWpDEM6isungQ3LfGnu9UXkt7paBQ6DYRxk1"
    );

    let entropy = derive_product_entropy(&entropy_secret(), "myapp.dot", b"product-key").unwrap();
    assert_eq!(
        hex::encode(entropy),
        "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36"
    );
}

#[wasm_bindgen_test]
fn pairing_deeplink_topic_and_scale_vectors_match_dotli() {
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
    assert_eq!(
        hex::encode(bootstrap_topic(SS_PUBLIC, ENC_PUBLIC)),
        "031c589833c39b1dfbe3c1304ced75fa7b0d841035db008e5b407bfadd2779a4"
    );

    let answer = AppHandshakeData::V1 {
        encrypted_message: vec![0xde, 0xad],
        public_key: ENC_PUBLIC,
    };
    assert_eq!(decode_app_handshake_data(&answer.encode()).unwrap(), answer);
}

#[wasm_bindgen_test]
fn p256_hkdf_aes_gcm_vectors_work_on_wasm() {
    let core_secret = SecretKey::from_slice(&[1; 32]).unwrap();
    let wallet_ephemeral_secret = SecretKey::from_slice(&[2; 32]).unwrap();
    let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);

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
            wallet_ephemeral_public.as_bytes().try_into().unwrap(),
            &encrypted,
        )
        .unwrap(),
        sensitive
    );
}

#[wasm_bindgen_test]
fn session_crypto_and_statement_proof_vectors_work_on_wasm() {
    let session = sso_session();
    let data = SsoStatementData::Request {
        request_id: "req-1".to_string(),
        data: vec![vec![0xde, 0xad]],
    };
    let nonce = [9u8; AES_GCM_NONCE_LEN];
    let encrypted = encrypt_session_statement_data_with_nonce(&session, &data, nonce).unwrap();

    assert_eq!(&encrypted[..AES_GCM_NONCE_LEN], nonce);
    assert_eq!(
        SsoStatementData::decode(&mut &data.encode()[..]).unwrap(),
        data
    );
    assert_eq!(
        decrypt_session_statement_data(&session, &encrypted).unwrap(),
        data
    );

    let statement_session = statement_session();
    let statement =
        build_signed_session_request_statement(&statement_session, vec![0xde, 0xad], 42).unwrap();
    let verified =
        decode_verified_statement_data(&statement, Some(statement_session.ss_public_key)).unwrap();

    assert_eq!(verified.signer, statement_session.ss_public_key);
    assert_eq!(verified.data, vec![0xde, 0xad]);
}
