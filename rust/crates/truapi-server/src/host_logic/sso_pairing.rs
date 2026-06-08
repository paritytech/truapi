//! SSO pairing bootstrap helpers.
//!
//! This module owns the byte shape of the QR/deeplink payload described in
//! `docs/design/host-contract-and-core-impl/H - sso-pairing-protocol.md`.

use blake2_rfc::blake2b::blake2b;
use p256::SecretKey;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use parity_scale_codec::Encode;
use schnorrkel::{ExpansionMode, MiniSecretKey};
use thiserror::Error;
use truapi_platform::{PairingDeeplinkScheme, RuntimeConfig};

const HANDSHAKE_TOPIC_SUFFIX: &[u8] = b"topic";
const MAX_P256_SECRET_ATTEMPTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingBootstrap {
    pub deeplink: String,
    pub topic: [u8; 32],
    pub statement_store_public_key: [u8; 32],
    pub statement_store_secret_seed: [u8; 32],
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

pub fn create_pairing_bootstrap(
    config: &RuntimeConfig,
) -> Result<PairingBootstrap, PairingBootstrapError> {
    let (statement_store_secret_seed, statement_store_public_key) =
        generate_statement_store_keypair()?;
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
        statement_store_secret_seed,
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

fn generate_statement_store_keypair() -> Result<([u8; 32], [u8; 32]), PairingBootstrapError> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|err| PairingBootstrapError::Random(err.to_string()))?;
    let mini_secret = MiniSecretKey::from_bytes(&seed)
        .map_err(|err| PairingBootstrapError::Random(err.to_string()))?;
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    Ok((seed, keypair.public.to_bytes()))
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
}
