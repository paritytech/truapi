//! Bandersnatch ring-VRF product-account aliases (signing host).
//!
//! Mirrors the mobile app's `ProductAccountHolder.deriveAlias`: the alias is a
//! thin bandersnatch VRF output over a per-product context, using a
//! bandersnatch secret derived from the wallet's BIP-39 entropy. No ring
//! commitment or SRS is involved (that machinery is only for membership
//! proofs, which this path does not use).
//!
//! Reference: polkadot-app-ios-v2 `Packages/Products/.../ProductAccountHolder.swift`
//! and `verifiable-swift` over `paritytech/verifiable`.

use verifiable::GenerateVerifiable;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

/// A product-account contextual alias.
pub struct ProductAlias {
    /// 32-byte context identifier (blake2b-256 of the derivation path).
    pub context: [u8; 32],
    /// 32-byte ring-VRF alias output.
    pub alias: [u8; 32],
}

/// Derive the contextual alias for a product account from the wallet entropy.
///
/// - `context = blake2b_256("/product/{product_id}/{derivation_index}")`
/// - `bandersnatch_entropy = blake2b_256(root_entropy)`
/// - `alias = BandersnatchVrf::alias_in_context(new_secret(bandersnatch_entropy), context)`
pub fn derive_product_alias(
    root_entropy: &[u8],
    product_id: &str,
    derivation_index: u32,
) -> Result<ProductAlias, String> {
    let derivation_path = format!("/product/{product_id}/{derivation_index}");
    let context = blake2b256(derivation_path.as_bytes());
    let bandersnatch_entropy = blake2b256(root_entropy);
    let secret = BandersnatchVrfVerifiable::new_secret(bandersnatch_entropy);
    let alias = BandersnatchVrfVerifiable::alias_in_context(&secret, &context)
        .map_err(|err| format!("ring-VRF alias derivation failed: {err:?}"))?;
    Ok(ProductAlias { context, alias })
}

fn blake2b256(message: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .hash(message)
        .as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alias is deterministic in the entropy, product id, and index, and
    /// the context is the blake2b-256 of the derivation path.
    #[test]
    fn alias_is_deterministic_with_expected_context() {
        let entropy = [0xABu8; 16];
        let first = derive_product_alias(&entropy, "truapi-playground.dot", 0).unwrap();
        let again = derive_product_alias(&entropy, "truapi-playground.dot", 0).unwrap();

        assert_eq!(first.context, again.context);
        assert_eq!(first.alias, again.alias);
        assert_eq!(
            first.context,
            blake2b256(b"/product/truapi-playground.dot/0")
        );
    }

    #[test]
    fn alias_varies_by_product_and_index() {
        let entropy = [0xABu8; 16];
        let base = derive_product_alias(&entropy, "a.dot", 0).unwrap();
        let other_product = derive_product_alias(&entropy, "b.dot", 0).unwrap();
        let other_index = derive_product_alias(&entropy, "a.dot", 1).unwrap();

        assert_ne!(base.alias, other_product.alias);
        assert_ne!(base.alias, other_index.alias);
    }
}
