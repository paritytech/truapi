//! People-chain identity lookup for paired SSO sessions.
//!
//! dotli's previous host-papp path read `Resources.Consumers[account]` from
//! the People chain and used only the username fields. Keep this module narrow:
//! it builds that storage key and decodes the leading username fields from the
//! SCALE value. The record begins with a fixed identifier public key; credibility
//! and statement-store slots are intentionally ignored.
//! Host-spec G defines the cross-host identity model and lookup behavior:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/G-identity.md?plain=1#L5-L47>

use parity_scale_codec::Decode;
use sp_crypto_hashing::{blake2_128, twox_128};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeopleIdentity {
    pub lite_username: Option<String>,
    pub full_username: Option<String>,
}

#[derive(Debug, Decode)]
struct ConsumerUsernamePrefix {
    full_username: Option<Vec<u8>>,
    lite_username: Vec<u8>,
}

/// Build the People-chain `Resources.Consumers` storage key for `account_id`.
pub fn resources_consumers_storage_key(account_id: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(32 + 16 + account_id.len());
    key.extend_from_slice(&twox_128(b"Resources"));
    key.extend_from_slice(&twox_128(b"Consumers"));
    key.extend_from_slice(&blake2_128(account_id));
    key.extend_from_slice(account_id);
    key
}

/// Decode the username fields from a `Resources.Consumers` storage value.
pub fn decode_people_identity(value: &[u8]) -> Result<PeopleIdentity, String> {
    if value.len() < 65 {
        return Err(format!(
            "invalid Resources.Consumers record: expected 65-byte identifier key, got {} bytes",
            value.len()
        ));
    }

    // ConsumerInfo starts with a fixed 65-byte P-256 identifier key. The
    // username fields follow immediately after it.
    let mut input = &value[65..];
    let decoded = ConsumerUsernamePrefix::decode(&mut input)
        .map_err(|err| format!("invalid Resources.Consumers record: {err}"))?;
    let lite_username = non_empty_string(decoded.lite_username)?;
    let full_username = decoded
        .full_username
        .map(non_empty_string)
        .transpose()?
        .flatten();
    Ok(PeopleIdentity {
        lite_username,
        full_username,
    })
}

fn non_empty_string(bytes: Vec<u8>) -> Result<Option<String>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let value = String::from_utf8(bytes)
        .map_err(|err| format!("Resources.Consumers username is not UTF-8: {err}"))?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;

    #[test]
    fn resources_consumers_key_uses_expected_prefix() {
        let key = resources_consumers_storage_key(&[0x42; 32]);

        assert_eq!(key.len(), 80);
        assert_eq!(&key[..16], &twox_128(b"Resources"));
        assert_eq!(&key[16..32], &twox_128(b"Consumers"));
        assert_eq!(&key[48..], &[0x42; 32]);
    }

    #[test]
    fn twox128_matches_substrate_storage_prefix_vector() {
        assert_eq!(
            hex::encode(twox_128(b"System")),
            "26aa394eea5630e07c48ae0c9558cef7"
        );
    }

    #[test]
    fn decodes_username_prefix_and_ignores_trailing_fields() {
        let mut value = vec![0x04; 65];
        value.extend((Some(b"Alice Smith".to_vec()), b"alice.01".to_vec()).encode());
        value.extend_from_slice(&[0xff; 8]);

        let decoded = decode_people_identity(&value).expect("identity should decode");

        assert_eq!(decoded.full_username.as_deref(), Some("Alice Smith"));
        assert_eq!(decoded.lite_username.as_deref(), Some("alice.01"));
    }

    #[test]
    fn empty_full_username_is_none() {
        let mut value = vec![0x04; 65];
        value.extend((Some(Vec::<u8>::new()), b"alice.01".to_vec()).encode());

        let decoded = decode_people_identity(&value).expect("identity should decode");

        assert_eq!(decoded.full_username, None);
        assert_eq!(decoded.lite_username.as_deref(), Some("alice.01"));
    }

    #[test]
    fn rejects_missing_identifier_key() {
        let value = (None::<Vec<u8>>, b"alice.01".to_vec()).encode();

        let error = decode_people_identity(&value).expect_err("identity should reject");

        assert!(error.contains("65-byte identifier key"));
    }
}
