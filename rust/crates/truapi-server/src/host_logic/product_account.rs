//! Product account derivation shared by all hosts.
//!
//! Mirrors host product-account derivation: derive an sr25519 public
//! key through soft HDKD junctions `["product", product_id, derivation_index]`,
//! where the derivation index is the 32-byte format defined by RFC-0022.
//! Host-spec C.5-C.7 define the product-account derivation, SS58 address, and
//! `ProductAccountId` shape:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L66-L128>

use parity_scale_codec::Encode;
use schnorrkel::derive::{ChainCode, Derivation};
use schnorrkel::{ExpansionMode, Keypair, PublicKey};
use std::str::FromStr;
use thiserror::Error;

const JUNCTION_ID_LEN: usize = 32;
const PRODUCT_JUNCTION: &str = "product";

/// Substrate sr25519 signing-context string, shared by every sr25519 signature
/// the core produces (statement store, product raw signing).
pub(crate) const SR25519_SIGNING_CONTEXT: &[u8] = b"substrate";

/// Error deriving product accounts or keys.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductAccountError {
    /// Root public key bytes are not a valid sr25519 point.
    #[error("invalid sr25519 root public key")]
    InvalidRootPublicKey,
    /// All-digit junction strings encode as `u64`, and this one overflows it.
    #[error("numeric derivation junction is outside u64 range")]
    NumericJunctionOutOfRange,
    /// Entropy bytes could not be expanded into a mini secret.
    #[error("invalid BIP-39 entropy: {0}")]
    InvalidEntropy(String),
}

/// Derive the root sr25519 keypair from raw BIP-39 entropy.
///
/// Host-spec C.1 defines the BIP-39 entropy to sr25519 mini-secret path:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L24-L41>
///
/// Matches the Substrate mini-secret scheme (`sp_core::sr25519::Pair::from_entropy`)
/// used by polkadot-app-ios-v2: PBKDF2 over the entropy to a 32-byte mini
/// secret, then Ed25519-mode expansion. The public key of this keypair is the
/// `rootAccountId` shared with paired hosts.
pub fn derive_root_keypair_from_entropy(entropy: &[u8]) -> Result<Keypair, ProductAccountError> {
    let mini_secret = substrate_bip39::mini_secret_from_entropy(entropy, "")
        .map_err(|err| ProductAccountError::InvalidEntropy(format!("{err:?}")))?;
    Ok(mini_secret.expand_to_keypair(ExpansionMode::Ed25519))
}

/// 28-byte magic separating plain-index space from raw 32-byte indexes:
/// `blake2b256("product-account-index")[..28]`.
fn index_magic() -> [u8; 28] {
    let digest = sp_crypto_hashing::blake2_256(b"product-account-index");
    let mut magic = [0u8; 28];
    magic.copy_from_slice(&digest[..28]);
    magic
}

/// 32-byte derivation index for a plain `u32` index: the index little-endian
/// followed by the index magic.
pub fn index_bytes(index: u32) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&index.to_le_bytes());
    bytes[4..].copy_from_slice(&index_magic());
    bytes
}

/// Internal 32-byte derivation index for a wire-level account selector.
pub fn derivation_index_bytes(index: &truapi::v01::DerivationIndex) -> [u8; 32] {
    match index {
        truapi::v01::DerivationIndex::Left(index) => index_bytes(*index),
        truapi::v01::DerivationIndex::Right(bytes) => *bytes,
    }
}

/// Derive a product-account keypair from the root keypair.
///
/// Applies the same soft HDKD junctions `["product", product_id,
/// derivation_index]` as [`derive_product_public_key`] on the secret side, so
/// the resulting public key equals the seedless public derivation by
/// construction.
pub fn derive_product_keypair(
    root: &Keypair,
    product_id: &str,
    derivation_index: [u8; 32],
) -> Result<Keypair, ProductAccountError> {
    let mut keypair = root.clone();
    for chain_code in product_chain_codes(product_id, derivation_index)? {
        keypair = keypair.derived_key_simple(ChainCode(chain_code), []).0;
    }
    Ok(keypair)
}

/// Derive a product account public key from the paired root public key.
pub fn derive_product_public_key(
    root_public_key: [u8; 32],
    product_id: &str,
    derivation_index: [u8; 32],
) -> Result<[u8; 32], ProductAccountError> {
    let mut public_key = PublicKey::from_bytes(&root_public_key)
        .map_err(|_| ProductAccountError::InvalidRootPublicKey)?;

    for chain_code in product_chain_codes(product_id, derivation_index)? {
        let (derived, _) = public_key.derived_key_simple(ChainCode(chain_code), []);
        public_key = derived;
    }

    Ok(public_key.to_bytes())
}

/// Chain codes for the product-account junction path
/// `["product", product_id, derivation_index]`. The 32-byte derivation index
/// is used directly as its junction's chain code.
fn product_chain_codes(
    product_id: &str,
    derivation_index: [u8; 32],
) -> Result<[[u8; 32]; 3], ProductAccountError> {
    Ok([
        create_chain_code(PRODUCT_JUNCTION)?,
        create_chain_code(product_id)?,
        derivation_index,
    ])
}

/// Encode a product account public key as a generic Substrate SS58 address.
///
/// Delegates to subxt's `AccountId32` Display, which is the generic-substrate
/// prefix-42 SS58-check encoding host-spec C.6 mandates; the test vector
/// below pins the format against drift.
pub fn product_public_key_to_address(public_key: [u8; 32]) -> String {
    subxt::utils::AccountId32(public_key).to_string()
}

/// Decode a Substrate SS58 account address into its raw public key.
pub fn public_key_from_address(address: &str) -> Option<[u8; 32]> {
    Some(subxt::utils::AccountId32::from_str(address).ok()?.0)
}

/// Create a Substrate soft-derivation chain code for one junction.
fn create_chain_code(code: &str) -> Result<[u8; 32], ProductAccountError> {
    let encoded = if !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_digit()) {
        code.parse::<u64>()
            .map_err(|_| ProductAccountError::NumericJunctionOutOfRange)?
            .encode()
    } else {
        code.encode()
    };
    Ok(normalize_chain_code(encoded))
}

/// Normalize a SCALE-encoded junction to a 32-byte chain code.
fn normalize_chain_code(encoded: Vec<u8>) -> [u8; 32] {
    let mut chain_code = [0u8; JUNCTION_ID_LEN];
    if encoded.len() > JUNCTION_ID_LEN {
        chain_code = sp_crypto_hashing::blake2_256(&encoded);
    } else {
        chain_code[..encoded.len()].copy_from_slice(&encoded);
    }
    chain_code
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
    fn derives_product_account_vector() {
        // Self-computed regression pin for the RFC-0022 32-byte-index path;
        // replace with a cross-implementation vector once the Account Holder
        // ships the scheme.
        let derived =
            derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", index_bytes(0)).unwrap();
        assert_eq!(
            hex::encode(derived),
            "0c7da1b57ade0827b6518174da49945b24d79541ee5e5403f646537e5746c80b"
        );
    }

    #[test]
    fn derives_different_index_vector() {
        let derived =
            derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", index_bytes(1)).unwrap();
        assert_eq!(
            hex::encode(derived),
            "20cce591a5e5306591de475e3c2efec3d94c6a00b8f52d3703a21f132555ee44"
        );
    }

    #[test]
    fn derives_long_product_id_vector() {
        let derived = derive_product_public_key(
            ROOT_PUBLIC_KEY,
            "w-credentialless-staticblitz-com.local-credentialless.webcontainer-api.io",
            index_bytes(0),
        )
        .unwrap();
        assert_eq!(
            hex::encode(derived),
            "06b64516f806d13dceafca5fda4aeac4c99265bc2e5ab3036decef3e7371e03f"
        );
    }

    #[test]
    fn ss58_address_regression_pin() {
        let derived =
            derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", index_bytes(0)).unwrap();
        assert_eq!(
            product_public_key_to_address(derived),
            "5CM5kaayBqheti7ugSEty5ptuzFhaP16fVm3ujAMVEtZqnKy"
        );
    }

    #[test]
    fn ss58_address_round_trips_to_public_key() {
        let derived =
            derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", index_bytes(0)).unwrap();
        let address = product_public_key_to_address(derived);

        assert_eq!(public_key_from_address(&address), Some(derived));
        assert_eq!(public_key_from_address("not-an-address"), None);
    }

    #[test]
    fn product_secret_derivation_matches_public_derivation() {
        // The signing-host secret path and the seedless public path must agree
        // on the product public key for any root, index, and product id.
        let entropy = [0xABu8; 16];
        let root = derive_root_keypair_from_entropy(&entropy).unwrap();
        let root_public = root.public.to_bytes();
        for (product_id, index) in [
            ("myapp.dot", index_bytes(0)),
            ("myapp.dot", index_bytes(1)),
            ("localhost:3000", index_bytes(7)),
            ("myapp.dot", [0xEE; 32]),
        ] {
            let keypair = derive_product_keypair(&root, product_id, index).unwrap();
            let public = derive_product_public_key(root_public, product_id, index).unwrap();
            assert_eq!(
                keypair.public.to_bytes(),
                public,
                "{product_id}#{index:02x?} secret vs public derivation",
            );
        }
    }

    #[test]
    fn index_bytes_layout_pin() {
        let index = index_bytes(5);
        assert_eq!(&index[..4], &[5, 0, 0, 0]);
        assert_eq!(
            index[4..],
            sp_crypto_hashing::blake2_256(b"product-account-index")[..28]
        );
    }

    #[test]
    fn derivation_index_bytes_maps_both_selector_forms() {
        use truapi::v01::DerivationIndex;

        assert_eq!(
            derivation_index_bytes(&DerivationIndex::Left(7)),
            index_bytes(7)
        );
        assert_eq!(
            derivation_index_bytes(&DerivationIndex::Right([0xEE; 32])),
            [0xEE; 32]
        );
    }

    #[test]
    fn raw_index_space_is_disjoint_from_plain_indexes() {
        // A raw all-zero index must not collide with plain index 0: the magic
        // keeps the two spaces separate.
        let indexed =
            derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", index_bytes(0)).unwrap();
        let raw = derive_product_public_key(ROOT_PUBLIC_KEY, "myapp.dot", [0u8; 32]).unwrap();
        assert_ne!(indexed, raw);
    }

    #[test]
    fn root_keypair_from_entropy_regression_pin() {
        // Regression pin for the entropy -> mini-secret -> sr25519 root path
        // (substrate-bip39 + schnorrkel Ed25519 expansion). This guards
        // against an accidental change to that path (dep bump, expansion mode)
        // that the pub-vs-secret self-consistency test cannot catch, since it
        // derives both sides from the same root.
        //
        // NOTE: this is a self-computed regression value, NOT yet cross-checked
        // against a polkadot-app-ios-v2 `deriveAccount` vector. Replace with an
        // iOS-sourced value once available to make it a true interop anchor.
        let root = derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
        assert_eq!(
            hex::encode(root.public.to_bytes()),
            "0062ba8ae929ea64bc2ad6f21359e96a29e236a41d376d1c5ba76491da94fc72",
        );
    }

    #[test]
    fn product_secret_signs_verifiably() {
        let root = derive_root_keypair_from_entropy(&[0xABu8; 16]).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let message = b"<Bytes>hello</Bytes>";
        let signature = keypair
            .secret
            .sign_simple(b"substrate", message, &keypair.public);
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", message, &signature)
                .is_ok()
        );
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
