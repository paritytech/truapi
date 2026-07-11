//! sr25519 transaction signing shared by chain-facing runtime services.
//!
//! [`Sr25519Signer`] is the one subxt [`Signer`] in the crate; Bulletin
//! preimage submission and the signing-host product key both go through it.
//! [`build_signed_extrinsic_v4`] assembles a signed V4 extrinsic from
//! caller-supplied, already-SCALE-encoded parts (local `create_transaction`),
//! so it needs no metadata at all.

use parity_scale_codec::Encode;
use schnorrkel::{PublicKey, SecretKey};
use subxt::config::substrate::SubstrateConfig;
use subxt::tx::Signer;
use subxt::utils::{AccountId32, MultiAddress, MultiSignature};
use truapi::v01;

use crate::host_logic::product_account::SR25519_SIGNING_CONTEXT;

/// Parse a 64-byte sr25519 secret in either of the two wire encodings.
///
/// Rust-generated keys use schnorrkel's canonical scalar bytes; legacy
/// JS-derived keys use scure/ed25519-expanded scalar bytes. Signatures are
/// identical for both once parsed, so callers never need to know which form
/// they hold.
pub(crate) fn sr25519_secret_from_bytes(secret: &[u8; 64]) -> Result<SecretKey, String> {
    match SecretKey::from_bytes(secret) {
        Ok(secret) => Ok(secret),
        Err(canonical_error) => SecretKey::from_ed25519_bytes(secret).map_err(|ed_error| {
            format!("invalid sr25519 secret: canonical={canonical_error}; ed25519={ed_error}")
        }),
    }
}

/// sr25519 [`Signer`] over a parsed schnorrkel key.
///
/// Holds only the parsed key (schnorrkel zeroizes it on drop), never the raw
/// secret bytes.
#[derive(derive_more::Debug)]
pub(crate) struct Sr25519Signer {
    #[debug("\"<redacted>\"")]
    secret: SecretKey,
    #[debug("{}", hex::encode(public.to_bytes()))]
    public: PublicKey,
}

impl Sr25519Signer {
    /// Parse a signer from 64-byte secret material.
    pub(crate) fn from_secret_bytes(secret: &[u8; 64]) -> Result<Self, String> {
        let secret = sr25519_secret_from_bytes(secret)?;
        let public = secret.to_public();
        Ok(Self { secret, public })
    }

    /// Build a signer from an already-derived schnorrkel keypair (e.g. a
    /// signing-host product key), reusing the same `sign`/`account_id` path.
    pub(crate) fn from_keypair(keypair: &schnorrkel::Keypair) -> Self {
        Self {
            secret: keypair.secret.clone(),
            public: keypair.public,
        }
    }
}

impl Signer<SubstrateConfig> for Sr25519Signer {
    fn account_id(&self) -> AccountId32 {
        AccountId32(self.public.to_bytes())
    }

    fn sign(&self, signer_payload: &[u8]) -> MultiSignature {
        let signature =
            self.secret
                .sign_simple(SR25519_SIGNING_CONTEXT, signer_payload, &self.public);
        MultiSignature::Sr25519(signature.to_bytes())
    }
}

/// The V4 signer payload: `call_data ++ Σextra ++ Σadditional_signed`, replaced
/// by its blake2_256 hash only when it exceeds 256 bytes.
///
/// Note the order differs from the extrinsic body (which puts `extra` before
/// the call): the call comes first here, extras next, implicits last.
fn v4_signer_payload(call_data: &[u8], extensions: &[v01::TxPayloadExtension]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(call_data.len());
    payload.extend_from_slice(call_data);
    for ext in extensions {
        payload.extend_from_slice(&ext.extra);
    }
    for ext in extensions {
        payload.extend_from_slice(&ext.additional_signed);
    }
    if payload.len() > 256 {
        sp_crypto_hashing::blake2_256(&payload).to_vec()
    } else {
        payload
    }
}

/// Assemble a signed Extrinsic V4 from caller-supplied, already-SCALE-encoded
/// parts, entirely offline.
///
/// Body layout (matches `frame_decode::encode_v4_signed` and subxt's own
/// assembler byte-for-byte):
///
/// ```text
/// Compact(len) ++ 0x84 ++ 0x00 ++ signer(32) ++ 0x01 ++ signature(64)
///              ++ Σ extension.extra ++ call_data
/// ```
///
/// `0x84` is the V4 "signed" version byte, `0x00` the `MultiAddress::Id`
/// discriminant, `0x01` the `MultiSignature::Sr25519` discriminant. `extra`
/// bytes go in the body (in the given order, which must be the runtime's
/// canonical extension order); `additional_signed` bytes appear only in the
/// signed payload. The chain binding (genesis, spec/tx version, mortality
/// anchor, nonce, tip) lives inside the caller's extension bytes, so nothing
/// here is metadata-driven.
pub(crate) fn build_signed_extrinsic_v4(
    signer: &Sr25519Signer,
    call_data: &[u8],
    extensions: &[v01::TxPayloadExtension],
) -> Vec<u8> {
    /// `0b1000_0000 | 4`: the "signed" bit plus extrinsic format version 4.
    const EXTRINSIC_V4_SIGNED: u8 = 0x84;

    let signature = signer.sign(&v4_signer_payload(call_data, extensions));
    let address = MultiAddress::<AccountId32, u32>::Id(signer.account_id());

    let mut inner = Vec::new();
    inner.push(EXTRINSIC_V4_SIGNED);
    address.encode_to(&mut inner);
    signature.encode_to(&mut inner);
    for ext in extensions {
        inner.extend_from_slice(&ext.extra);
    }
    inner.extend_from_slice(call_data);

    // `Vec<u8>::encode` prepends the SCALE compact length, giving the outer
    // length-prefixed opaque extrinsic.
    inner.encode()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use parity_scale_codec::{Compact, Decode};
    use subxt::client::{OfflineClient, OfflineClientAtBlock};
    use subxt::config::substrate::{SpecVersionForRange, SubstrateConfigBuilder};
    use subxt::ext::frame_metadata::RuntimeMetadataPrefixed;
    use subxt::metadata::{ArcMetadata, Metadata};
    use subxt::utils::H256;

    /// Everything needed to build transactions offline for one chain at one
    /// runtime version, mirroring what the production path gets from the
    /// genesis-pinned online client.
    pub(crate) struct OfflineChainState {
        pub(crate) genesis_hash: [u8; 32],
        pub(crate) spec_version: u32,
        pub(crate) transaction_version: u32,
        pub(crate) metadata: ArcMetadata,
    }

    impl OfflineChainState {
        /// Build an offline subxt client pinned at `block_number`.
        pub(crate) fn client_at(
            &self,
            block_number: u64,
        ) -> Result<OfflineClientAtBlock<SubstrateConfig>, String> {
            let config = SubstrateConfigBuilder::new()
                .set_genesis_hash(H256(self.genesis_hash))
                .set_spec_version_for_block_ranges([SpecVersionForRange {
                    block_range: 0..u64::MAX,
                    spec_version: self.spec_version,
                    transaction_version: self.transaction_version,
                }])
                .set_metadata_for_spec_versions([(self.spec_version, self.metadata.clone())])
                .build();
            OfflineClient::new_with_config(config)
                .at_block(block_number)
                .map_err(|err| format!("offline client unavailable: {err}"))
        }
    }

    /// Raw `RuntimeMetadataPrefixed` bytes captured from the live
    /// bulletin-paseo chain (`state_getMetadata`, spec 1000020, metadata v14).
    pub(crate) const BULLETIN_METADATA_BYTES: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bulletin_paseo_metadata.scale"
    ));

    /// Decode the checked-in bulletin metadata fixture.
    pub(crate) fn bulletin_metadata() -> Metadata {
        let prefixed = RuntimeMetadataPrefixed::decode(&mut &BULLETIN_METADATA_BYTES[..]).unwrap();
        Metadata::try_from(prefixed).unwrap()
    }

    /// Offline chain state over the bulletin fixture with a recognizable
    /// genesis hash.
    pub(crate) fn bulletin_chain_state() -> OfflineChainState {
        OfflineChainState {
            genesis_hash: [0xbb; 32],
            spec_version: 1_000_020,
            transaction_version: 1,
            metadata: ArcMetadata::from(bulletin_metadata()),
        }
    }

    #[test]
    fn parses_canonical_and_ed25519_expanded_secrets() {
        let canonical: [u8; 64] = hex::decode(
            "0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
             573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
        )
        .unwrap()
        .try_into()
        .unwrap();
        assert!(sr25519_secret_from_bytes(&canonical).is_ok());

        let mini = schnorrkel::MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let expanded = mini
            .expand(schnorrkel::ExpansionMode::Ed25519)
            .to_ed25519_bytes();
        let expanded: [u8; 64] = expanded.as_slice().try_into().unwrap();
        let parsed = sr25519_secret_from_bytes(&expanded).unwrap();
        assert_eq!(
            parsed.to_public().to_bytes(),
            mini.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
                .public
                .to_bytes()
        );
    }

    #[test]
    fn signer_signs_under_substrate_context() {
        let mini = schnorrkel::MiniSecretKey::from_bytes(&[9; 32]).unwrap();
        let secret: [u8; 64] = mini
            .expand(schnorrkel::ExpansionMode::Ed25519)
            .to_bytes()
            .as_slice()
            .try_into()
            .unwrap();
        let signer = Sr25519Signer::from_secret_bytes(&secret).unwrap();

        let payload = b"payload";
        let MultiSignature::Sr25519(signature) = signer.sign(payload) else {
            panic!("expected sr25519 signature");
        };
        let public = PublicKey::from_bytes(&signer.account_id().0).unwrap();
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    payload,
                    &schnorrkel::Signature::from_bytes(&signature).unwrap()
                )
                .is_ok()
        );
    }

    fn test_signer() -> Sr25519Signer {
        let keypair = schnorrkel::MiniSecretKey::from_bytes(&[3; 32])
            .unwrap()
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        Sr25519Signer::from_keypair(&keypair)
    }

    fn ext(id: &str, extra: &[u8], additional: &[u8]) -> v01::TxPayloadExtension {
        v01::TxPayloadExtension {
            id: id.to_string(),
            extra: extra.to_vec(),
            additional_signed: additional.to_vec(),
        }
    }

    /// Split a length-prefixed V4 signed extrinsic into
    /// (account, signature, trailing-bytes-after-signature).
    pub(crate) fn split_v4(extrinsic: &[u8]) -> ([u8; 32], [u8; 64], Vec<u8>) {
        let mut input = extrinsic;
        let len = Compact::<u32>::decode(&mut input).unwrap().0 as usize;
        assert_eq!(input.len(), len, "compact length must cover the remainder");
        assert_eq!(input[0], 0x84, "V4 signed version byte");
        assert_eq!(input[1], 0x00, "MultiAddress::Id discriminant");
        let account: [u8; 32] = input[2..34].try_into().unwrap();
        assert_eq!(input[34], 0x01, "MultiSignature::Sr25519 discriminant");
        let signature: [u8; 64] = input[35..99].try_into().unwrap();
        (account, signature, input[99..].to_vec())
    }

    #[test]
    fn v4_layout_and_signature_verify() {
        let signer = test_signer();
        let call_data = vec![0x2a, 0x00, 0xde, 0xad];
        let extensions = vec![
            ext("CheckNonce", &[0x04], &[]),
            ext("CheckGenesis", &[], &[9; 32]),
        ];

        let extrinsic = build_signed_extrinsic_v4(&signer, &call_data, &extensions);
        let (account, signature, tail) = split_v4(&extrinsic);

        // Body tail is Σextra ++ call_data (extra before call).
        let mut expected_tail = Vec::new();
        expected_tail.extend_from_slice(&extensions[0].extra);
        expected_tail.extend_from_slice(&extensions[1].extra);
        expected_tail.extend_from_slice(&call_data);
        assert_eq!(account, signer.account_id().0);
        assert_eq!(tail, expected_tail);

        // Signature verifies over call ++ Σextra ++ Σadditional (call first).
        let mut payload = call_data.clone();
        payload.extend_from_slice(&extensions[0].extra);
        payload.extend_from_slice(&extensions[1].extra);
        payload.extend_from_slice(&extensions[0].additional_signed);
        payload.extend_from_slice(&extensions[1].additional_signed);
        let public = PublicKey::from_bytes(&account).unwrap();
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    &payload,
                    &schnorrkel::Signature::from_bytes(&signature).unwrap()
                )
                .is_ok()
        );
    }

    #[test]
    fn v4_signer_payload_hashes_when_over_256_bytes() {
        let signer = test_signer();
        let call_data = vec![1u8; 200];
        // Push the payload (call ++ extra ++ additional) over 256 bytes.
        let extensions = vec![ext("Big", &[2u8; 60], &[3u8; 60])];
        let extrinsic = build_signed_extrinsic_v4(&signer, &call_data, &extensions);
        let (account, signature, _) = split_v4(&extrinsic);
        let public = PublicKey::from_bytes(&account).unwrap();
        let sig = schnorrkel::Signature::from_bytes(&signature).unwrap();

        let mut raw = call_data.clone();
        raw.extend_from_slice(&extensions[0].extra);
        raw.extend_from_slice(&extensions[0].additional_signed);
        assert!(raw.len() > 256);
        let hashed = sp_crypto_hashing::blake2_256(&raw);

        // Signs over the hash, not the raw concatenation.
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &hashed, &sig)
                .is_ok()
        );
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &raw, &sig)
                .is_err()
        );
    }

    #[test]
    fn v4_preserves_extension_order() {
        let signer = test_signer();
        let call_data = vec![0xff];
        // Deliberately not in sorted order; the assembler must not reorder.
        let extensions = vec![ext("B", &[0xbb], &[]), ext("A", &[0xaa], &[])];
        let (_, _, tail) = split_v4(&build_signed_extrinsic_v4(&signer, &call_data, &extensions));
        assert_eq!(tail, vec![0xbb, 0xaa, 0xff]);
    }
}
