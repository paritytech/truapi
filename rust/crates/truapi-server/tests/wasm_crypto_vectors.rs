//! Wasm-target vectors pinning RFC-0022 product-account, built-in, and SSO
//! X25519/ChaCha20-Poly1305 crypto to the values used by the mobile hosts.

#![cfg(target_arch = "wasm32")]

use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, MiniSecretKey};
use truapi_platform::{
    CoreStorageKey, HostDevicePermissionRequest, HostInfo, PairingHostConfig, PlatformInfo,
};
use truapi_server::host_logic::entropy::derive_product_entropy;
use truapi_server::host_logic::product_account::{
    derive_product_public_key, derive_product_subtree_keypair, derive_root_keypair_from_entropy,
    index_bytes,
};
use truapi_server::host_logic::session::SsoSessionInfo;
use truapi_server::host_logic::sso::pairing::{
    self, AEAD_NONCE_LEN, PairingBootstrap, SsoStatementData, VersionedHandshakeProposal,
    VersionedHandshakeResponse, bootstrap_topic, build_pairing_deeplink, decode_app_handshake_data,
    decrypt_session_statement_data, decrypt_v2_handshake_response,
    encrypt_session_statement_data_with_nonce, encrypt_v2_handshake_response,
    establish_sso_session_info,
};
use truapi_server::host_logic::statement_store::{
    build_signed_session_request_statement, decode_verified_statement_data,
};
use wasm_bindgen_test::wasm_bindgen_test;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

const SS_PUBLIC: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

fn encryption_public(seed: u8) -> [u8; 32] {
    X25519PublicKey::from(&X25519SecretKey::from([seed; 32])).to_bytes()
}

fn entropy_secret() -> [u8; 32] {
    std::array::from_fn(|i| i as u8)
}

fn runtime_config() -> PairingHostConfig {
    PairingHostConfig::new(
        HostInfo {
            name: "Polkadot Web".to_string(),
            icon: Some("https://example.invalid/dotli.png".to_string()),
            version: Some("1.2.3".to_string()),
        },
        PlatformInfo {
            kind: Some("Firefox".to_string()),
            version: Some("192.32".to_string()),
        },
        [0xa2; 32],
        [0xbb; 32],
        "polkadotapp".to_string(),
    )
    .expect("test runtime config is valid")
}

fn statement_session() -> SsoSessionInfo {
    let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    SsoSessionInfo {
        ss_secret: keypair.secret.to_bytes(),
        ss_public_key: keypair.public.to_bytes(),
        enc_secret: [1; 32],
        peer_enc_pubkey: encryption_public(2),
        identity_account_id: [3; 32],
        session_id_own: [4; 32],
        session_id_peer: [5; 32],
        request_channel: [6; 32],
        response_channel: [7; 32],
        peer_request_channel: [8; 32],
    }
}

fn sso_session() -> SsoSessionInfo {
    let bootstrap = PairingBootstrap {
        deeplink: "polkadotapp://pair?handshake=00".to_string(),
        topic: [0x11; 32],
        statement_store_public_key: [0x22; 32],
        statement_store_secret: [0x33; 64],
        encryption_public_key: encryption_public(1),
        encryption_secret_key: [1; 32],
    };

    establish_sso_session_info(&bootstrap, [0x55; 32], encryption_public(2)).unwrap()
}

#[wasm_bindgen_test]
fn product_account_and_entropy_vectors_match_mobile() {
    let root = derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree = derive_product_subtree_keypair(&root, "myapp.dot").unwrap();
    let derived = derive_product_public_key(subtree.public.to_bytes(), index_bytes(0)).unwrap();
    assert_eq!(
        hex::encode(derived),
        "1c1ae478b564572f806ffa6352b4273d612beb01610b19f4e5bf444521cd5b5c"
    );

    let entropy = derive_product_entropy(&entropy_secret(), "myapp.dot", b"product-key").unwrap();
    assert_eq!(
        hex::encode(entropy),
        "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36"
    );
}

#[wasm_bindgen_test]
fn pairing_deeplink_topic_and_scale_vectors_match_mobile() {
    let config = runtime_config();
    let encryption_public = encryption_public(1);
    let deeplink = build_pairing_deeplink("polkadotapp", SS_PUBLIC, encryption_public, &config);
    assert!(deeplink.starts_with("polkadotapp://pair?handshake=01"));
    let encoded = hex::decode(deeplink.split("handshake=").nth(1).unwrap()).unwrap();
    let decoded = VersionedHandshakeProposal::decode(&mut &encoded[..]).unwrap();
    let VersionedHandshakeProposal::V2(proposal) = decoded;
    assert_eq!(proposal.device.statement_account_id, SS_PUBLIC);
    assert_eq!(proposal.device.encryption_public_key, encryption_public);
    assert!(proposal.metadata.contains(&pairing::v2::MetadataEntry(
        pairing::v2::MetadataKey::HostName,
        "Polkadot Web".to_string()
    )));
    assert!(proposal.metadata.contains(&pairing::v2::MetadataEntry(
        pairing::v2::MetadataKey::HostIcon,
        "https://example.invalid/dotli.png".to_string()
    )));
    assert_eq!(
        hex::encode(bootstrap_topic(SS_PUBLIC, encryption_public)),
        "ec8c8d7993ef1b367a704f34cec0fa1fe01d0a060a918688f26b23e88452a6af"
    );

    let answer = VersionedHandshakeResponse::V2 {
        encrypted_message: vec![0xde, 0xad],
        public_key: encryption_public,
    };
    assert_eq!(decode_app_handshake_data(&answer.encode()).unwrap(), answer);
}

#[wasm_bindgen_test]
fn x25519_chacha20_poly1305_vectors_work_on_wasm() {
    let core_secret = X25519SecretKey::from([1; 32]);
    let core_public = X25519PublicKey::from(&core_secret).to_bytes();
    let sensitive = pairing::v2::EncryptedResponse::Success(Box::new(pairing::v2::Success {
        identity_account_id: [8; 32],
        root_account_id: [7; 32],
        identity_chat_private_key: [6; 32],
        sso_enc_pub_key: encryption_public(3),
        device_enc_pub_key: encryption_public(4),
        root_entropy_source: [5; 32],
    }));
    let answer = encrypt_v2_handshake_response(core_public, &sensitive).unwrap();
    let VersionedHandshakeResponse::V2 {
        encrypted_message,
        public_key,
    } = answer;

    assert_eq!(
        decrypt_v2_handshake_response(core_secret.to_bytes(), public_key, &encrypted_message)
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
    let nonce = [9u8; AEAD_NONCE_LEN];
    let encrypted = encrypt_session_statement_data_with_nonce(&session, &data, nonce).unwrap();

    assert_eq!(&encrypted[..AEAD_NONCE_LEN], nonce);
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

#[wasm_bindgen_test]
fn wasm_core_storage_descriptors_are_strict_and_stable() {
    let encoded = CoreStorageKey::device_permission_authorization(
        "product.dot",
        &HostDevicePermissionRequest::Camera,
    )
    .encode();
    let described =
        truapi_server::wasm::describe_core_storage_key_for_wasm(encoded).expect("valid key");
    assert_eq!(
        js_sys::Reflect::get(&described, &wasm_bindgen::JsValue::from_str("kind"))
            .expect("kind property")
            .as_string()
            .as_deref(),
        Some("PermissionAuthorization")
    );
    assert_eq!(
        js_sys::Reflect::get(&described, &wasm_bindgen::JsValue::from_str("productId"),)
            .expect("productId property")
            .as_string()
            .as_deref(),
        Some("product.dot")
    );

    assert!(truapi_server::wasm::describe_core_storage_key_for_wasm(Vec::new()).is_err());
    let mut trailing = CoreStorageKey::AuthSession.encode();
    trailing.push(0);
    assert!(truapi_server::wasm::describe_core_storage_key_for_wasm(trailing).is_err());
}
