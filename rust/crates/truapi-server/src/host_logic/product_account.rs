//! Product account derivation shared by all hosts.
//!
//! Mirrors dotli's `packages/auth/src/account.ts`: derive an sr25519 public
//! key through soft HDKD junctions `["product", product_id, derivation_index]`.

use blake2_rfc::blake2b::blake2b;
use parity_scale_codec::Encode;
use schnorrkel::PublicKey;
use schnorrkel::derive::{ChainCode, Derivation};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

const JUNCTION_ID_LEN: usize = 32;
const PRODUCT_JUNCTION: &str = "product";
const SS58_PREFIX: &[u8] = b"SS58PRE";
const SUBSTRATE_GENERIC_SS58_PREFIX: u8 = 42;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductAccountError {
    #[error("invalid sr25519 root public key")]
    InvalidRootPublicKey,
    #[error("numeric derivation junction is outside u64 range")]
    NumericJunctionOutOfRange,
}

/// Whether `identifier` is a product scope the core is allowed to derive for.
pub fn is_product_identifier(identifier: &str) -> bool {
    let normalized = normalize_product_identifier(identifier);
    normalized.ends_with(".dot")
        || normalized == "localhost"
        || normalized.starts_with("localhost:")
}

/// Normalize product identifiers before derivation and policy checks.
pub fn normalize_product_identifier(identifier: &str) -> String {
    identifier.nfc().collect::<String>().to_lowercase()
}

/// Derive a product account public key from the paired root public key.
pub fn derive_product_public_key(
    root_public_key: [u8; 32],
    product_id: &str,
    derivation_index: u32,
) -> Result<[u8; 32], ProductAccountError> {
    let mut public_key = PublicKey::from_bytes(&root_public_key)
        .map_err(|_| ProductAccountError::InvalidRootPublicKey)?;

    for junction in [
        PRODUCT_JUNCTION.to_string(),
        product_id.to_string(),
        derivation_index.to_string(),
    ] {
        let chain_code = ChainCode(create_chain_code(&junction)?);
        let (derived, _) = public_key.derived_key_simple(chain_code, []);
        public_key = derived;
    }

    Ok(public_key.to_bytes())
}

/// Encode a product account public key as a generic Substrate SS58 address.
pub fn product_public_key_to_address(public_key: [u8; 32]) -> String {
    let mut payload = Vec::with_capacity(35);
    payload.push(SUBSTRATE_GENERIC_SS58_PREFIX);
    payload.extend_from_slice(&public_key);

    let mut checksum_input = Vec::with_capacity(SS58_PREFIX.len() + payload.len());
    checksum_input.extend_from_slice(SS58_PREFIX);
    checksum_input.extend_from_slice(&payload);
    let checksum = blake2b(64, &[], &checksum_input);
    payload.extend_from_slice(&checksum.as_bytes()[..2]);

    bs58::encode(payload).into_string()
}

fn create_chain_code(code: &str) -> Result<[u8; 32], ProductAccountError> {
    let encoded = if is_numeric_junction(code) {
        code.parse::<u64>()
            .map_err(|_| ProductAccountError::NumericJunctionOutOfRange)?
            .encode()
    } else {
        code.encode()
    };

    let mut chain_code = [0u8; JUNCTION_ID_LEN];
    if encoded.len() > JUNCTION_ID_LEN {
        let hash = blake2b(JUNCTION_ID_LEN, &[], &encoded);
        chain_code.copy_from_slice(hash.as_bytes());
    } else {
        chain_code[..encoded.len()].copy_from_slice(&encoded);
    }
    Ok(chain_code)
}

fn is_numeric_junction(code: &str) -> bool {
    !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT_PUBLIC_KEY: [u8; 32] = [
        0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d, 0xb8,
        0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab, 0xc1, 0xd7,
        0x20, 0x23,
    ];

    #[test]
    fn derives_dotli_product_account_vector() {
        let derived = derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", 0).unwrap();
        assert_eq!(
            hex::encode(derived),
            "281489e3dd1c4dbe88cd670a59edcc9c44d64f510d302bd527ec306f10292f08"
        );
    }

    #[test]
    fn derives_different_index_vector() {
        let derived = derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", 1).unwrap();
        assert_eq!(
            hex::encode(derived),
            "ec8a80808b46e44c1351b68e295eb975c55bda4855e5ea9fc1325be7296a2a4e"
        );
    }

    #[test]
    fn derives_long_product_id_vector() {
        let derived = derive_product_public_key(
            ROOT_PUBLIC_KEY,
            "w-credentialless-staticblitz-com.local-credentialless.webcontainer-api.io",
            0,
        )
        .unwrap();
        assert_eq!(
            hex::encode(derived),
            "56769a234038defb62a7ad42f251091cc24846c2473a31b5bdd17d366c38c211"
        );
    }

    #[test]
    fn ss58_address_matches_dotli_vector() {
        let derived = derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", 0).unwrap();
        assert_eq!(
            product_public_key_to_address(derived),
            "5CyFsdhwjXy7wWpDEM6isungQ3LfGnu9UXkt7paBQ6DYRxk1"
        );
    }

    #[test]
    fn accepts_dot_and_localhost_product_identifiers() {
        assert!(is_product_identifier("Example.DOT"));
        assert!(is_product_identifier("localhost"));
        assert!(is_product_identifier("localhost:3000"));
        assert!(!is_product_identifier("example.com"));
    }

    #[test]
    fn chain_code_matches_dotli_encoding_rules() {
        let product = create_chain_code("product").unwrap();
        assert_eq!(
            &product[..8],
            &[0x1c, b'p', b'r', b'o', b'd', b'u', b'c', b't']
        );

        let zero = create_chain_code("0").unwrap();
        assert_eq!(&zero[..8], &[0; 8]);

        let long = create_chain_code(
            "w-credentialless-staticblitz-com.local-credentialless.webcontainer-api.io",
        )
        .unwrap();
        assert_ne!(&long[..8], &[0; 8]);
    }
}
