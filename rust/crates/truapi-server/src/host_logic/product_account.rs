//! RFC-0022 account and personhood derivation shared by all hosts.
//!
//! Product subtrees use hard HDKD at `//product//{product_id}`. Individual
//! accounts use one soft junction carrying the RFC-0022 32-byte derivation
//! index, so a paired host can derive children from the subtree public key.
//! Reserved built-ins additionally pin the `uid.dot` identity account and the
//! `peopl.dot` full/lite ring-VRF keyed-hash paths so activation, pairing,
//! registration, proof, allowance, and CLI code cannot drift.
//! Host-spec C.5-C.7 define the product-account derivation, SS58 address, and
//! `ProductAccountId` shape:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L66-L128>

use parity_scale_codec::Encode;
use schnorrkel::derive::{ChainCode, Derivation};
use schnorrkel::{ExpansionMode, Keypair, PublicKey, SecretKey};
use std::str::FromStr;
use thiserror::Error;

const JUNCTION_ID_LEN: usize = 32;
const PRODUCT_JUNCTION: &str = "product";
/// Reserved RFC-0022 product id for the public light-person identity account.
pub const IDENTITY_PRODUCT_ID: &str = "uid.dot";
/// Reserved RFC-0022 ring-VRF domain for full and light personhood.
pub const PERSONHOOD_PRODUCT_ID: &str = "peopl.dot";
const RING_VRF_ROOT_KEY: &[u8] = b"ring-vrf";

/// Substrate sr25519 signing-context string, shared by every sr25519 signature
/// the core produces (statement store, product raw signing).
pub(crate) const SR25519_SIGNING_CONTEXT: &[u8] = b"substrate";

/// Error deriving product accounts or keys.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductAccountError {
    /// Root public key bytes are not a valid sr25519 point.
    #[error("invalid sr25519 root public key")]
    InvalidRootPublicKey,
    /// Product subtree secret bytes are not a valid expanded sr25519 secret.
    #[error("invalid sr25519 product subtree secret")]
    InvalidProductSubtreeSecret,
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
/// Derive the RFC-0022 public light-person identity account:
/// `//product//uid.dot/index_bytes(0)`.
pub fn derive_identity_keypair(entropy: &[u8]) -> Result<Keypair, ProductAccountError> {
    let root = derive_root_keypair_from_entropy(entropy)?;
    let subtree = derive_hard_path_from_keypair(root, &[PRODUCT_JUNCTION, IDENTITY_PRODUCT_ID])?;
    Ok(subtree.derived_key_simple(ChainCode(index_bytes(0)), []).0)
}

/// Derive the RFC-0022 full-person ring-VRF entropy at
/// `hash(root_entropy, "ring-vrf")//peopl.dot//index_bytes(0)`.
pub fn derive_full_person_ring_vrf_entropy(root_entropy: &[u8]) -> [u8; 32] {
    derive_person_ring_vrf_entropy(root_entropy, 0)
}

/// Derive the RFC-0022 light-person ring-VRF entropy at
/// `hash(root_entropy, "ring-vrf")//peopl.dot//index_bytes(1)`.
pub fn derive_lite_person_ring_vrf_entropy(root_entropy: &[u8]) -> [u8; 32] {
    derive_person_ring_vrf_entropy(root_entropy, 1)
}

fn derive_person_ring_vrf_entropy(root_entropy: &[u8], index: u32) -> [u8; 32] {
    let root = blake2b256_keyed(root_entropy, RING_VRF_ROOT_KEY);
    let domain = blake2b256_keyed(
        &root,
        &create_chain_code(PERSONHOOD_PRODUCT_ID)
            .expect("the reserved personhood product id is a valid junction"),
    );
    blake2b256_keyed(&domain, &index_bytes(index))
}

fn blake2b256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .key(key)
        .hash(message)
        .as_bytes()
        .try_into()
        .expect("hash_length(32) configures BLAKE2b output to exactly 32 bytes; qed")
}

/// Derive the product-subtree keypair at `//product//{product_id}`.
///
/// The hard junctions are the security boundary: exposing this keypair grants
/// access to one product subtree without exposing the root account.
pub fn derive_product_subtree_keypair(
    root: &Keypair,
    product_id: &str,
) -> Result<Keypair, ProductAccountError> {
    derive_hard_path_from_keypair(root.clone(), &[PRODUCT_JUNCTION, product_id])
}

/// Derive a product-account keypair from the root keypair.
///
/// First derives the hard product subtree, then applies the account's 32-byte
/// derivation index as one soft junction.
pub fn derive_product_keypair(
    root: &Keypair,
    product_id: &str,
    derivation_index: [u8; 32],
) -> Result<Keypair, ProductAccountError> {
    let subtree = derive_product_subtree_keypair(root, product_id)?;
    Ok(subtree
        .derived_key_simple(ChainCode(derivation_index), [])
        .0)
}

/// Soft-derive a product account from an allocated 64-byte subtree secret.
pub fn derive_product_keypair_from_subtree_secret(
    product_subtree_secret: [u8; 64],
    derivation_index: [u8; 32],
) -> Result<Keypair, ProductAccountError> {
    let secret = SecretKey::from_bytes(&product_subtree_secret)
        .map_err(|_| ProductAccountError::InvalidProductSubtreeSecret)?;
    let subtree = Keypair {
        public: secret.to_public(),
        secret,
    };
    Ok(subtree
        .derived_key_simple(ChainCode(derivation_index), [])
        .0)
}

/// Soft-derive a product account from `//product//{product_id}`'s public key.
///
/// A root public key cannot cross the two hard product junctions. Pairing hosts
/// must obtain this subtree public key from the Account Holder once per product.
pub fn derive_product_public_key(
    product_subtree_public_key: [u8; 32],
    derivation_index: [u8; 32],
) -> Result<[u8; 32], ProductAccountError> {
    let public_key = PublicKey::from_bytes(&product_subtree_public_key)
        .map_err(|_| ProductAccountError::InvalidRootPublicKey)?;
    let (derived, _) = public_key.derived_key_simple(ChainCode(derivation_index), []);
    Ok(derived.to_bytes())
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

/// Derive an sr25519 keypair down a path of hard string junctions from the
/// canonical BIP-39 root key.
pub fn derive_sr25519_hard_path(
    entropy: &[u8],
    junctions: &[&str],
) -> Result<Keypair, ProductAccountError> {
    derive_hard_path_from_keypair(derive_root_keypair_from_entropy(entropy)?, junctions)
}

fn derive_hard_path_from_keypair(
    mut keypair: Keypair,
    junctions: &[&str],
) -> Result<Keypair, ProductAccountError> {
    for junction in junctions {
        let chain_code = ChainCode(create_chain_code(junction)?);
        let (mini_secret, _) = keypair
            .secret
            .hard_derive_mini_secret_key(Some(chain_code), b"");
        keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    }
    Ok(keypair)
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

    fn fixture_root() -> Keypair {
        derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap()
    }

    #[test]
    fn product_subtrees_are_hard_firewalls() {
        let root = fixture_root();
        let first = derive_product_subtree_keypair(&root, "myapp.dot").unwrap();
        let second = derive_product_subtree_keypair(&root, "other.dot").unwrap();

        assert_ne!(first.public, root.public);
        assert_ne!(first.public, second.public);
    }

    #[test]
    fn product_secret_derivation_matches_subtree_public_derivation() {
        let root = fixture_root();
        for (product_id, index) in [
            ("myapp.dot", index_bytes(0)),
            ("myapp.dot", index_bytes(1)),
            ("localhost:3000", index_bytes(7)),
            ("myapp.dot", [0xEE; 32]),
        ] {
            let subtree = derive_product_subtree_keypair(&root, product_id).unwrap();
            let keypair = derive_product_keypair(&root, product_id, index).unwrap();
            let public = derive_product_public_key(subtree.public.to_bytes(), index).unwrap();
            let allocated =
                derive_product_keypair_from_subtree_secret(subtree.secret.to_bytes(), index)
                    .unwrap();
            assert_eq!(
                keypair.public.to_bytes(),
                public,
                "{product_id}#{index:02x?} secret vs public derivation",
            );
            assert_eq!(keypair.public, allocated.public);
        }
    }

    #[test]
    fn root_public_key_cannot_substitute_for_product_subtree() {
        let root = fixture_root();
        let actual = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let unsafe_soft_only =
            derive_product_public_key(root.public.to_bytes(), index_bytes(0)).unwrap();

        assert_ne!(actual.public.to_bytes(), unsafe_soft_only);
    }

    #[test]
    fn long_product_id_derives_stably() {
        let root = fixture_root();
        let product_id =
            "w-credentialless-staticblitz-com.local-credentialless.webcontainer-api.io";
        let subtree = derive_product_subtree_keypair(&root, product_id).unwrap();
        let from_secret = derive_product_keypair(&root, product_id, index_bytes(0)).unwrap();
        let from_public =
            derive_product_public_key(subtree.public.to_bytes(), index_bytes(0)).unwrap();

        assert_eq!(from_secret.public.to_bytes(), from_public);
    }

    #[test]
    fn ss58_address_round_trips_to_public_key() {
        let root = fixture_root();
        let derived = derive_product_keypair(&root, "myapp.dot", index_bytes(0))
            .unwrap()
            .public
            .to_bytes();
        let address = product_public_key_to_address(derived);

        assert_eq!(public_key_from_address(&address), Some(derived));
        assert_eq!(public_key_from_address("not-an-address"), None);
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
    fn index_bytes_matches_ios_vector() {
        assert_eq!(
            hex::encode(index_bytes(0)),
            "0000000012e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f"
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
    fn person_ring_vrf_entropy_matches_ios_vectors() {
        let root_entropy: Vec<u8> = (1..=32).collect();
        assert_eq!(
            hex::encode(blake2b256_keyed(&root_entropy, RING_VRF_ROOT_KEY)),
            "372b08255c7798fe3193756296005adc4c44adb9f3986fb718aa98a48b4bf725"
        );
        assert_eq!(
            hex::encode(derive_full_person_ring_vrf_entropy(&root_entropy)),
            "c47086f94a7f4c05b7afd9f2339d3fea168f3823b5424ba1f7b31043d8ef60af"
        );
        assert_eq!(
            hex::encode(derive_lite_person_ring_vrf_entropy(&root_entropy)),
            "8d7f5e1510a7e8d813887e100f5a260ec9de60e68695477b93360ee7e3d16a9f"
        );
    }

    #[test]
    fn identity_is_uid_dot_default_product_account_and_signs() {
        let entropy = [0xAB; 16];
        let identity = derive_identity_keypair(&entropy).unwrap();
        let root = derive_root_keypair_from_entropy(&entropy).unwrap();
        let uid_subtree =
            derive_hard_path_from_keypair(root, &[PRODUCT_JUNCTION, IDENTITY_PRODUCT_ID]).unwrap();
        let expected = uid_subtree
            .derived_key_simple(ChainCode(index_bytes(0)), [])
            .0;
        assert_eq!(identity.public, expected.public);

        let message = b"RFC-0022 identity signing vector";
        let signature =
            identity
                .secret
                .sign_simple(SR25519_SIGNING_CONTEXT, message, &identity.public);
        assert!(
            identity
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, message, &signature)
                .is_ok()
        );
    }

    #[test]
    fn raw_index_space_is_disjoint_from_plain_indexes() {
        // A raw all-zero index must not collide with plain index 0: the magic
        // keeps the two spaces separate.
        let subtree = derive_product_subtree_keypair(&fixture_root(), "myapp.dot").unwrap();
        let indexed = derive_product_public_key(subtree.public.to_bytes(), index_bytes(0)).unwrap();
        let raw = derive_product_public_key(subtree.public.to_bytes(), [0u8; 32]).unwrap();
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
