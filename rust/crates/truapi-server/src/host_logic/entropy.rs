//! Product-scoped deterministic entropy derivation.
//!
//! Matches dotli's product entropy contract: three keyed BLAKE2b-256 layers
//! over the session secret, product id, and caller key.
//! Host-spec C.8 defines the RFC-0007 product entropy algorithm:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L129-L147>

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

/// Derive product-scoped entropy from the session root entropy secret.
pub fn derive_product_entropy(
    entropy_secret: &[u8],
    product_id: &str,
    key: &[u8],
) -> Result<[u8; 32], ProductEntropyError> {
    derive_product_entropy_from_source(&root_entropy_source(entropy_secret), product_id, key)
}

/// Pre-hashed root entropy source (RFC-0007 layer 1). Signing hosts share this
/// value with paired hosts during the SSO handshake so both sides derive the
/// same product entropy.
pub fn root_entropy_source(entropy_secret: &[u8]) -> [u8; 32] {
    blake2b256_keyed(entropy_secret, DOMAIN_SEPARATOR)
}

/// Derive product-scoped entropy when the session already stores the
/// pre-hashed root entropy source.
pub fn derive_product_entropy_from_source(
    root_entropy_source: &[u8; 32],
    product_id: &str,
    key: &[u8],
) -> Result<[u8; 32], ProductEntropyError> {
    if key.is_empty() || key.len() > 32 {
        return Err(ProductEntropyError::InvalidKeyLength(key.len()));
    }

    let product_id_hash = blake2b256_keyed(product_id.as_bytes(), &[]);
    let per_product_entropy = blake2b256_keyed(root_entropy_source, &product_id_hash);
    Ok(blake2b256_keyed(&per_product_entropy, key))
}

fn blake2b256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
    let hash = blake2b(32, key, message);
    hash.as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
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
    fn product_entropy_cases() {
        struct SuccessCase {
            name: &'static str,
            product_id: &'static str,
            key: Vec<u8>,
            expected_hex: &'static str,
        }

        let success_cases = vec![
            SuccessCase {
                name: "single byte key",
                product_id: "myapp.dot",
                key: vec![1],
                expected_hex: "4bafd6a34182959bad8914dcff88c6b6842d551d6f0067afbd407e9584223404",
            },
            SuccessCase {
                name: "text key",
                product_id: "myapp.dot",
                key: b"product-key".to_vec(),
                expected_hex: "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36",
            },
            SuccessCase {
                name: "localhost product",
                product_id: "localhost:3000",
                key: (0..32).map(|i| 255 - i).collect(),
                expected_hex: "437d0a6236c51fe114cf6a16b79c9c2b5f95b1e105e2d5269cc254a8c593925f",
            },
        ];

        for case in success_cases {
            let entropy = derive_product_entropy(&secret(), case.product_id, &case.key).unwrap();
            assert_eq!(hex::encode(entropy), case.expected_hex, "{}", case.name);
        }

        // Byte-for-byte vectors from polkadot-app-ios-v2
        // (ProductRootEntropyDeriverTests): raw BIP-39 entropy of 16 * 0xAB.
        let ios_entropy = [0xABu8; 16];
        let ios_cases = [
            (
                "test.product.dot",
                b"my-key".as_slice(),
                "479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a",
            ),
            (
                "test.product.dot",
                b"other-key".as_slice(),
                "0d576d5d77cb179bf94b85cb1d644b7879315e74d9e69791fb9cbe94df3c7c39",
            ),
            (
                "other.product.dot",
                b"my-key".as_slice(),
                "e2f25271c106593c2977d5965f52fa1d2227da0fc110d682c8cb8f30b2ba21c8",
            ),
        ];
        for (product_id, key, expected_hex) in ios_cases {
            let entropy = derive_product_entropy(&ios_entropy, product_id, key).unwrap();
            assert_eq!(
                hex::encode(entropy),
                expected_hex,
                "ios vector {product_id}"
            );
        }

        let error_cases = vec![
            (Vec::new(), ProductEntropyError::InvalidKeyLength(0)),
            (vec![0u8; 33], ProductEntropyError::InvalidKeyLength(33)),
        ];
        for (key, expected) in error_cases {
            assert_eq!(
                derive_product_entropy(&secret(), "myapp.dot", &key).unwrap_err(),
                expected,
            );
        }
    }
}
