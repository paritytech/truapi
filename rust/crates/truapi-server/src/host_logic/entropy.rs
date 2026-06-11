//! Product-scoped deterministic entropy derivation.
//!
//! Matches dotli's product entropy contract: three keyed BLAKE2b-256 layers
//! over the session secret, product id, and caller key.

use blake2_rfc::blake2b::blake2b;
use thiserror::Error;

const DOMAIN_SEPARATOR: &[u8] = b"product-entropy-derivation";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductEntropyError {
    #[error("\"key\" must be between 1 and 32 bytes, got {0}")]
    InvalidKeyLength(usize),
    #[error("entropy secret is missing")]
    MissingSecret,
}

pub fn derive_product_entropy(
    entropy_secret: &[u8],
    product_id: &str,
    key: &[u8],
) -> Result<[u8; 32], ProductEntropyError> {
    let root_entropy_source = blake2b256_keyed(entropy_secret, DOMAIN_SEPARATOR);
    derive_product_entropy_from_source(&root_entropy_source, product_id, key)
}

pub fn derive_product_entropy_from_source(
    root_entropy_source: &[u8; 32],
    product_id: &str,
    key: &[u8],
) -> Result<[u8; 32], ProductEntropyError> {
    if key.is_empty() || key.len() > 32 {
        return Err(ProductEntropyError::InvalidKeyLength(key.len()));
    }

    let product_id_hash = blake2b256(product_id.as_bytes());
    let per_product_entropy = blake2b256_keyed(root_entropy_source, &product_id_hash);
    Ok(blake2b256_keyed(&per_product_entropy, key))
}

fn blake2b256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
    let hash = blake2b(32, key, message);
    hash.as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

fn blake2b256(message: &[u8]) -> [u8; 32] {
    blake2b256_keyed(message, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> [u8; 32] {
        let mut secret = [0u8; 32];
        for (i, byte) in secret.iter_mut().enumerate() {
            *byte = i as u8;
        }
        secret
    }

    #[test]
    fn derives_dotli_entropy_single_byte_key_vector() {
        let entropy = derive_product_entropy(&secret(), "myapp.dot", &[1]).unwrap();
        assert_eq!(
            hex::encode(entropy),
            "4bafd6a34182959bad8914dcff88c6b6842d551d6f0067afbd407e9584223404"
        );
    }

    #[test]
    fn derives_dotli_entropy_text_key_vector() {
        let entropy = derive_product_entropy(&secret(), "myapp.dot", b"product-key").unwrap();
        assert_eq!(
            hex::encode(entropy),
            "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36"
        );
    }

    #[test]
    fn derives_dotli_entropy_localhost_vector() {
        let key: Vec<u8> = (0..32).map(|i| 255 - i).collect();
        let entropy = derive_product_entropy(&secret(), "localhost:3000", &key).unwrap();
        assert_eq!(
            hex::encode(entropy),
            "437d0a6236c51fe114cf6a16b79c9c2b5f95b1e105e2d5269cc254a8c593925f"
        );
    }

    #[test]
    fn rejects_empty_and_long_keys_like_dotli() {
        assert_eq!(
            derive_product_entropy(&secret(), "myapp.dot", &[]).unwrap_err(),
            ProductEntropyError::InvalidKeyLength(0)
        );
        assert_eq!(
            derive_product_entropy(&secret(), "myapp.dot", &[0u8; 33]).unwrap_err(),
            ProductEntropyError::InvalidKeyLength(33)
        );
    }
}
