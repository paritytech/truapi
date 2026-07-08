//! Bulletin allowance signing helpers shared by platform backends.

use schnorrkel::SecretKey;
use truapi_platform::{BulletinAllowanceKey, BulletinAllowanceSignError, BulletinAllowanceSigner};

use crate::host_logic::product_account::SR25519_SIGNING_CONTEXT;

/// Build a host-facing Bulletin signer from cached allowance key material.
pub(crate) fn bulletin_allowance_signer_from_key(
    key: BulletinAllowanceKey,
) -> Result<BulletinAllowanceSigner, String> {
    let secret = key.into_secret_bytes();
    let public_key = public_key_from_allowance_secret(secret)?;
    Ok(BulletinAllowanceSigner::new(public_key, move |payload| {
        sign_with_allowance_secret(secret, payload)
            .map_err(|reason| BulletinAllowanceSignError { reason })
    }))
}

/// Derive the public key for a mobile slot-account allowance secret.
pub(crate) fn public_key_from_allowance_secret(secret: [u8; 64]) -> Result<[u8; 32], String> {
    Ok(secret_key_from_allowance_secret(secret)?
        .to_public()
        .to_bytes())
}

fn sign_with_allowance_secret(secret: [u8; 64], payload: &[u8]) -> Result<[u8; 64], String> {
    let secret = secret_key_from_allowance_secret(secret)?;
    let public = secret.to_public();
    Ok(secret
        .sign_simple(SR25519_SIGNING_CONTEXT, payload, &public)
        .to_bytes())
}

fn secret_key_from_allowance_secret(secret: [u8; 64]) -> Result<SecretKey, String> {
    // Mobile allowance keys are `SlotAccountKey` values (`privateKey || nonce`)
    // and must use schnorrkel's canonical `SecretKey::from_bytes` path. Older
    // JS-derived keys used ed25519-expanded bytes, so keep the fallback for
    // compatibility with persisted allocations.
    match SecretKey::from_bytes(&secret) {
        Ok(secret) => Ok(secret),
        Err(canonical_error) => SecretKey::from_ed25519_bytes(&secret).map_err(|ed_error| {
            format!(
                "invalid bulletin allowance key: canonical={canonical_error}; ed25519={ed_error}"
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schnorrkel::{PublicKey, Signature};

    fn slot_secret_fixture() -> [u8; 64] {
        hex::decode(
            "0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
             573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
        )
        .unwrap()
        .try_into()
        .unwrap()
    }

    #[test]
    fn derives_mobile_slot_account_public_key() {
        let public_key = public_key_from_allowance_secret(slot_secret_fixture()).unwrap();

        assert_eq!(
            hex::encode(public_key),
            "10c68432943c68a6e1be650818b5e08db79e57823de9f34df7ba36d404d91e1d"
        );
    }

    #[test]
    fn signs_with_mobile_slot_account_secret() {
        let secret = slot_secret_fixture();
        let signer = bulletin_allowance_signer_from_key(
            BulletinAllowanceKey::from_secret_bytes(secret.to_vec()).unwrap(),
        )
        .unwrap();
        let payload = b"hello-slot";
        let signature = signer.sign(payload).unwrap();
        let public_key = PublicKey::from_bytes(&signer.public_key()).unwrap();
        let signature = Signature::from_bytes(&signature).unwrap();

        assert!(
            public_key
                .verify_simple(SR25519_SIGNING_CONTEXT, payload, &signature)
                .is_ok()
        );
    }
}
