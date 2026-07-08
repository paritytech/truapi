//! Product account derivation shared by all hosts.
//!
//! Mirrors host product-account derivation: derive an sr25519 public
//! key through soft HDKD junctions `["product", product_id, derivation_index]`.
//! Host-spec C.5-C.7 define the product-account derivation, SS58 address, and
//! `ProductAccountId` shape:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L66-L128>

use blake2_rfc::blake2b::blake2b;
use parity_scale_codec::Encode;
use schnorrkel::derive::{ChainCode, Derivation};
use schnorrkel::{ExpansionMode, Keypair, PublicKey};
use thiserror::Error;

const JUNCTION_ID_LEN: usize = 32;
const PRODUCT_JUNCTION: &str = "product";
const SS58_PREFIX: &[u8] = b"SS58PRE";
const SUBSTRATE_GENERIC_SS58_PREFIX: u8 = 42;

/// Substrate sr25519 signing-context string, shared by every sr25519 signature
/// the core produces (statement store, product raw signing).
pub(crate) const SR25519_SIGNING_CONTEXT: &[u8] = b"substrate";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductAccountError {
    #[error("invalid sr25519 root public key")]
    InvalidRootPublicKey,
    #[error("numeric derivation junction is outside u64 range")]
    NumericJunctionOutOfRange,
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

/// Derive a product-account keypair from the root keypair.
///
/// Applies the same soft HDKD junctions `["product", product_id,
/// derivation_index]` as [`derive_product_public_key`] on the secret side, so
/// the resulting public key equals the seedless public derivation by
/// construction.
pub fn derive_product_keypair(
    root: &Keypair,
    product_id: &str,
    derivation_index: u32,
) -> Result<Keypair, ProductAccountError> {
    let mut keypair = root.clone();
    let derivation_index = derivation_index.to_string();
    for junction in [PRODUCT_JUNCTION, product_id, derivation_index.as_str()] {
        let chain_code = ChainCode(create_chain_code(junction)?);
        keypair = keypair.derived_key_simple(chain_code, []).0;
    }
    Ok(keypair)
}

/// Derive a product account public key from the paired root public key.
pub fn derive_product_public_key(
    root_public_key: [u8; 32],
    product_id: &str,
    derivation_index: u32,
) -> Result<[u8; 32], ProductAccountError> {
    let mut public_key = PublicKey::from_bytes(&root_public_key)
        .map_err(|_| ProductAccountError::InvalidRootPublicKey)?;

    let derivation_index = derivation_index.to_string();
    for junction in [PRODUCT_JUNCTION, product_id, derivation_index.as_str()] {
        let chain_code = ChainCode(create_chain_code(junction)?);
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

/// Create a Substrate soft-derivation chain code for one junction.
fn create_chain_code(code: &str) -> Result<[u8; 32], ProductAccountError> {
    let encoded = if !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_digit()) {
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
    fn product_secret_derivation_matches_public_derivation() {
        // The signing-host secret path and the seedless public path must agree
        // on the product public key for any root, index, and product id.
        let entropy = [0xABu8; 16];
        let root = derive_root_keypair_from_entropy(&entropy).unwrap();
        let root_public = root.public.to_bytes();
        for (product_id, index) in [("myapp.dot", 0u32), ("myapp.dot", 1), ("localhost:3000", 7)] {
            let keypair = derive_product_keypair(&root, product_id, index).unwrap();
            let public = derive_product_public_key(root_public, product_id, index).unwrap();
            assert_eq!(
                keypair.public.to_bytes(),
                public,
                "{product_id}#{index} secret vs public derivation",
            );
        }
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
        let keypair = derive_product_keypair(&root, "myapp.dot", 0).unwrap();
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
