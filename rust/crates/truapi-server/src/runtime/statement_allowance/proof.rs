//! Bandersnatch ring-VRF one-shot membership proof (prover side).
//!
//! Wraps `verifiable`'s prover-gated `open` + `create` into the single-shot
//! proof a `RegisterStatementStoreAllowance` needs: prove that our member key is
//! in the LitePeople ring, bound to a slot `context` and the extrinsic proof
//! `message`. Mirrors signing-bot `ring-proof.ts` `oneShotProof`.

use verifiable::GenerateVerifiable;
use verifiable::ring::RingDomainSize;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

/// A single-context ring-VRF signature is exactly 785 bytes.
pub const RING_VRF_PROOF_LEN: usize = 785;

/// Map an on-chain `RingExponent` (9 / 10 / 14) to the FFT domain size
/// (power = exponent + 2).
pub fn domain_for_ring_exponent(exponent: u8) -> Result<RingDomainSize, String> {
    match exponent {
        9 => Ok(RingDomainSize::Domain11),
        10 => Ok(RingDomainSize::Domain12),
        14 => Ok(RingDomainSize::Domain16),
        other => Err(format!("unsupported ring exponent {other}")),
    }
}

/// The ring member key for a bandersnatch entropy (`blake2b256(bip39_entropy)`).
pub fn member_key(entropy: [u8; 32]) -> [u8; 32] {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    BandersnatchVrfVerifiable::member_from_secret(&secret)
}

/// Produce the 785-byte ring-VRF membership proof over `members` (already
/// sliced to the ring's included prefix), bound to `context` and `message`.
///
/// `entropy` is the bandersnatch entropy; its member key must be present in
/// `members` or `open` fails with `NotInRing`.
pub fn ring_vrf_proof(
    domain: RingDomainSize,
    entropy: [u8; 32],
    members: &[[u8; 32]],
    context: &[u8],
    message: &[u8],
) -> Result<Vec<u8>, String> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let member = BandersnatchVrfVerifiable::member_from_secret(&secret);
    let commitment = BandersnatchVrfVerifiable::open(domain, &member, members.iter().copied())
        .map_err(|err| format!("ring-VRF open failed: {err:?}"))?;
    let (proof, _alias) = BandersnatchVrfVerifiable::create(commitment, &secret, context, message)
        .map_err(|err| format!("ring-VRF create failed: {err:?}"))?;
    let bytes = proof.into_inner();
    if bytes.len() != RING_VRF_PROOF_LEN {
        return Err(format!(
            "ring-VRF proof is {} bytes, expected {RING_VRF_PROOF_LEN}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponent_maps_to_domain() {
        assert_eq!(
            domain_for_ring_exponent(9).unwrap(),
            RingDomainSize::Domain11
        );
        assert_eq!(
            domain_for_ring_exponent(10).unwrap(),
            RingDomainSize::Domain12
        );
        assert_eq!(
            domain_for_ring_exponent(14).unwrap(),
            RingDomainSize::Domain16
        );
        assert!(domain_for_ring_exponent(11).is_err());
    }

    #[test]
    fn proof_is_785_bytes_for_a_single_member_ring() {
        let entropy = [0x11u8; 32];
        let member = member_key(entropy);
        let members = vec![member];
        let proof = ring_vrf_proof(
            RingDomainSize::Domain11,
            entropy,
            &members,
            b"SSS_SLOT:test-context-padding..",
            &[0x42; 32],
        )
        .unwrap();
        assert_eq!(proof.len(), RING_VRF_PROOF_LEN);
    }

    #[test]
    fn open_fails_when_member_absent_from_ring() {
        let entropy = [0x11u8; 32];
        let other = member_key([0x22u8; 32]);
        let err = ring_vrf_proof(
            RingDomainSize::Domain11,
            entropy,
            &[other],
            b"SSS_SLOT:test-context-padding..",
            &[0x42; 32],
        )
        .unwrap_err();
        assert!(err.contains("open failed"), "unexpected error: {err}");
    }
}
