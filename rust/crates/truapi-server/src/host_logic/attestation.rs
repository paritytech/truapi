//! Lite-person username registration parameters (signing host, native only).
//!
//! Builds the client-side proofs the People-chain identity backend needs to
//! attest a lite username for an account: an sr25519 proof-of-ownership, a
//! bandersnatch ring-VRF member key + plain-VRF proof, and an sr25519
//! consumer-registration signature. The backend submits the on-chain
//! `register_lite_person` extrinsic; the host never signs a chain extrinsic.
//!
//! Byte layout mirrors signing-bot `src/core/attestation.ts` for backend
//! parity. The registered account is the account whose secret signs here; the
//! paired host resolves the username from `Resources.Consumers[that account]`.

use blake2_rfc::blake2b::blake2b;
use parity_scale_codec::Encode;
use verifiable::GenerateVerifiable;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

use crate::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derive_sr25519_hard_path, product_public_key_to_address,
};
use crate::host_logic::sso::pairing::derive_p256_keypair_from_entropy;

/// sr25519 proof-of-ownership message prefix (exact bytes; one space).
const REGISTER_PREFIX: &[u8] = b"pop:people-lite:register using";
/// Domain label for the P-256 identifier key advertised to the backend.
const IDENTIFIER_KEY_LABEL: &[u8] = b"chat-encryption";

/// Client-computed parameters for `POST /usernames`.
pub struct LiteRegistration {
    /// SS58 (prefix 42) of the candidate account.
    pub candidate_account_id: String,
    /// Raw 32-byte candidate public key (the future `Resources.Consumers` key).
    pub candidate_public_key: [u8; 32],
    /// sr25519 signature over `prefix ‖ candidate_pub ‖ ring_vrf_key`.
    pub candidate_signature: [u8; 64],
    /// Bandersnatch ring-VRF member key.
    pub ring_vrf_key: [u8; 32],
    /// Plain bandersnatch VRF proof over the same proof message.
    pub proof_of_ownership: [u8; 64],
    /// 65-byte uncompressed P-256 identifier key.
    pub identifier_key: [u8; 65],
    /// sr25519 signature over the SCALE consumer-registration tuple.
    pub consumer_registration_signature: [u8; 64],
}

/// Build the lite-person registration parameters for `username_base`
/// (6+ lowercase letters, no digit suffix) against the backend `verifier`.
pub fn build_lite_registration(
    entropy: &[u8],
    verifier_account_id: [u8; 32],
    username_base: &str,
) -> Result<LiteRegistration, String> {
    // The candidate is the `//wallet//sso` statement account, matching the
    // account the SSO responder presents as `identity_account_id`.
    let candidate = derive_sr25519_hard_path(entropy, &["wallet", "sso"])
        .map_err(|err| format!("//wallet//sso derivation failed: {err}"))?;
    let candidate_public_key = candidate.public.to_bytes();

    let vrf_entropy = blake2b256(entropy);
    let vrf_secret = BandersnatchVrfVerifiable::new_secret(vrf_entropy);
    let ring_vrf_key = BandersnatchVrfVerifiable::member_from_secret(&vrf_secret);

    let mut proof_message = Vec::with_capacity(REGISTER_PREFIX.len() + 64);
    proof_message.extend_from_slice(REGISTER_PREFIX);
    proof_message.extend_from_slice(&candidate_public_key);
    proof_message.extend_from_slice(&ring_vrf_key);

    let candidate_signature = candidate
        .secret
        .sign_simple(SR25519_SIGNING_CONTEXT, &proof_message, &candidate.public)
        .to_bytes();
    let proof_of_ownership = BandersnatchVrfVerifiable::sign(&vrf_secret, &proof_message)
        .map_err(|err| format!("ring-VRF proof-of-ownership failed: {err:?}"))?;

    let (_identifier_secret, identifier_key) =
        derive_p256_keypair_from_entropy(entropy, IDENTIFIER_KEY_LABEL)
            .map_err(|err| format!("identifier key derivation failed: {err}"))?;

    // SCALE Tuple(Bytes(32), Bytes(32), Bytes(65), str, Option(Bytes())=None).
    let mut consumer_message = Vec::new();
    consumer_message.extend_from_slice(&candidate_public_key);
    consumer_message.extend_from_slice(&verifier_account_id);
    consumer_message.extend_from_slice(&identifier_key);
    consumer_message.extend_from_slice(&username_base.encode());
    consumer_message.push(0u8);
    let consumer_registration_signature = candidate
        .secret
        .sign_simple(
            SR25519_SIGNING_CONTEXT,
            &consumer_message,
            &candidate.public,
        )
        .to_bytes();

    Ok(LiteRegistration {
        candidate_account_id: product_public_key_to_address(candidate_public_key),
        candidate_public_key,
        candidate_signature,
        ring_vrf_key,
        proof_of_ownership,
        identifier_key,
        consumer_registration_signature,
    })
}

fn blake2b256(message: &[u8]) -> [u8; 32] {
    blake2b(32, &[], message)
        .as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use schnorrkel::{PublicKey, Signature};

    const ENTROPY: [u8; 16] = [0xAB; 16];

    #[test]
    fn registration_params_have_expected_shapes_and_verify() {
        let verifier = [0x11u8; 32];
        let reg = build_lite_registration(&ENTROPY, verifier, "headlesstester").unwrap();

        assert_eq!(reg.identifier_key[0], 0x04, "P-256 uncompressed prefix");
        assert!(
            reg.candidate_account_id
                .chars()
                .all(|c| c.is_alphanumeric())
        );

        // candidateSignature verifies over prefix ‖ candidate_pub ‖ ring_vrf_key.
        let mut proof_message = Vec::new();
        proof_message.extend_from_slice(REGISTER_PREFIX);
        proof_message.extend_from_slice(&reg.candidate_public_key);
        proof_message.extend_from_slice(&reg.ring_vrf_key);
        let public = PublicKey::from_bytes(&reg.candidate_public_key).unwrap();
        let sig = Signature::from_bytes(&reg.candidate_signature).unwrap();
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &proof_message, &sig)
                .is_ok(),
            "candidate signature verifies"
        );

        // proofOfOwnership verifies as a plain VRF signature for the member key.
        assert!(
            BandersnatchVrfVerifiable::verify_signature(
                &reg.proof_of_ownership,
                &proof_message,
                &reg.ring_vrf_key
            ),
            "ring-VRF proof-of-ownership validates against the member key"
        );
    }

    #[test]
    fn registration_is_deterministic_per_entropy_and_username() {
        let verifier = [0x22u8; 32];
        let first = build_lite_registration(&ENTROPY, verifier, "aliceheadless").unwrap();
        let again = build_lite_registration(&ENTROPY, verifier, "aliceheadless").unwrap();
        assert_eq!(first.candidate_public_key, again.candidate_public_key);
        assert_eq!(first.ring_vrf_key, again.ring_vrf_key);
        assert_eq!(first.candidate_account_id, again.candidate_account_id);
    }
}
