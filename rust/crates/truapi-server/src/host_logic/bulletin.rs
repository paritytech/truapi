//! Bulletin `TransactionStorage.store` construction and signing.
//!
//! This module is the only place allowance-key material becomes a signer, and
//! it only ever signs the `store` call it builds itself: the public surface
//! takes raw preimage bytes plus a [`BulletinAllowanceKey`], never
//! caller-supplied call data.

use parity_scale_codec::{Compact, Encode};
use subxt::client::{ClientAtBlock, OfflineClientAtBlockT};
use subxt::config::DefaultExtrinsicParamsBuilder;
use subxt::config::substrate::SubstrateConfig;
use subxt::ext::scale_encode::{self, EncodeAsFields, FieldIter, TypeResolver};
use subxt::ext::scale_type_resolver::{Primitive, visitor};
use subxt::tx::{StaticPayload, SubmittableTransaction};
use subxt::utils::H256;
use truapi_platform::BulletinAllowanceKey;

use crate::host_logic::extrinsic::Sr25519Signer;

pub(crate) const STORE_PALLET_NAME: &str = "TransactionStorage";
pub(crate) const STORE_CALL_NAME: &str = "store";

/// Mortality window for store transactions. Must stay <= 4096 so the era
/// phase quantization is a no-op and the anchor block is the era birth block
/// (larger periods make the encoded anchor hash wrong and the signature
/// fails as BadProof).
const MORTAL_PERIOD_BLOCKS: u64 = 64;

/// Preimage key: blake2b-256 of the raw preimage bytes.
pub(crate) fn preimage_key(value: &[u8]) -> [u8; 32] {
    sp_crypto_hashing::blake2_256(value)
}

/// Block the transaction's mortality is anchored at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MortalityAnchor {
    /// Anchor block number (era birth block).
    pub(crate) number: u64,
    /// Anchor block hash (the `CheckMortality` implicit).
    pub(crate) hash: [u8; 32],
}

/// Build and sign a `TransactionStorage.store { data }` transaction with the
/// Bulletin allowance signer against the client's block. The anchor must be
/// the block the client is at, so the mortal era and the dry-run block agree.
pub(crate) fn build_signed_store_transaction<C: OfflineClientAtBlockT<SubstrateConfig>>(
    client: &ClientAtBlock<SubstrateConfig, C>,
    anchor: &MortalityAnchor,
    signer: &Sr25519Signer,
    nonce: u64,
    data: &[u8],
) -> Result<SubmittableTransaction<SubstrateConfig, C>, String> {
    if !client
        .metadata_ref()
        .extrinsic()
        .supported_versions()
        .contains(&4)
    {
        return Err(format!(
            "bulletin runtime no longer supports v4 extrinsics (supported: {:?})",
            client.metadata_ref().extrinsic().supported_versions()
        ));
    }

    let payload = StaticPayload::new(STORE_PALLET_NAME, STORE_CALL_NAME, StoreCallData(data));
    let params = DefaultExtrinsicParamsBuilder::<SubstrateConfig>::new()
        .nonce(nonce)
        .mortal_from_unchecked(MORTAL_PERIOD_BLOCKS, anchor.number, H256(anchor.hash))
        .build();
    client
        .tx()
        .create_v4_signable_offline(&payload, params)
        .map_err(|err| format!("store transaction assembly failed: {err}"))?
        .sign(signer)
        .map_err(|err| format!("store transaction signing failed: {err}"))
}

/// The only [`BulletinAllowanceKey`] -> signer conversion in the crate. The
/// returned signer is a transient per-call value; callers must not store it.
pub(crate) fn allowance_signer(allowance: &BulletinAllowanceKey) -> Result<Sr25519Signer, String> {
    Sr25519Signer::from_secret_bytes(allowance.as_secret_bytes())
        .map_err(|reason| format!("invalid bulletin allowance key: {reason}"))
}

/// `store { data: Vec<u8> }` call arguments with a byte-level fast path.
///
/// scale-encode has no `u8` specialization, so encoding the preimage as a
/// `Vec<u8>` value would pay a registry resolution and visitor dispatch per
/// byte, twice per transaction. This implementation verifies once that the
/// `data` field resolves to a sequence of `u8` (hard error otherwise, so
/// metadata lies about the argument type are rejected) and then memcpys.
struct StoreCallData<'a>(&'a [u8]);

impl EncodeAsFields for StoreCallData<'_> {
    fn encode_as_fields_to<R: TypeResolver>(
        &self,
        fields: &mut dyn FieldIter<'_, R::TypeId>,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), scale_encode::Error> {
        let field = fields
            .next()
            .ok_or_else(|| scale_encode::Error::custom_str("store call has no data field"))?;
        if fields.next().is_some() {
            return Err(scale_encode::Error::custom_str(
                "store call has more than one field",
            ));
        }
        require_u8_sequence(types, field.id)?;

        Compact(self.0.len() as u32).encode_to(out);
        out.extend_from_slice(self.0);
        Ok(())
    }
}

/// Hard-error unless `type_id` resolves to a sequence whose element type is
/// the `u8` primitive.
fn require_u8_sequence<R: TypeResolver>(
    types: &R,
    type_id: R::TypeId,
) -> Result<(), scale_encode::Error> {
    let sequence_visitor = visitor::new((), |(), _kind| None::<R::TypeId>)
        .visit_sequence(|(), _path, inner| Some(inner));
    let inner = types
        .resolve_type(type_id, sequence_visitor)
        .map_err(|err| scale_encode::Error::custom_string(err.to_string()))?
        .ok_or_else(|| {
            scale_encode::Error::custom_str("store data field is not a byte sequence")
        })?;

    let u8_visitor = visitor::new((), |(), _kind| false)
        .visit_primitive(|(), primitive| matches!(primitive, Primitive::U8));
    let is_u8 = types
        .resolve_type(inner, u8_visitor)
        .map_err(|err| scale_encode::Error::custom_string(err.to_string()))?;
    if !is_u8 {
        return Err(scale_encode::Error::custom_str(
            "store data field is not a sequence of u8",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::extrinsic::tests::{OfflineChainState, bulletin_chain_state, split_v4};
    use crate::host_logic::product_account::SR25519_SIGNING_CONTEXT;
    use parity_scale_codec::Decode;
    use schnorrkel::{PublicKey, Signature};
    use subxt::ext::frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed, v14};
    use subxt::metadata::{ArcMetadata, Metadata};

    fn allowance_fixture() -> BulletinAllowanceKey {
        let secret = hex::decode(
            "0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
             573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
        )
        .unwrap();
        BulletinAllowanceKey::from_secret_bytes(secret).unwrap()
    }

    fn signer_fixture() -> Sr25519Signer {
        allowance_signer(&allowance_fixture()).unwrap()
    }

    fn anchor_fixture() -> MortalityAnchor {
        MortalityAnchor {
            number: 4200,
            hash: [0xaa; 32],
        }
    }

    /// Decode the fixture metadata down to its mutable v14 representation.
    fn bulletin_metadata_v14() -> v14::RuntimeMetadataV14 {
        let prefixed = RuntimeMetadataPrefixed::decode(
            &mut &crate::host_logic::extrinsic::tests::BULLETIN_METADATA_BYTES[..],
        )
        .unwrap();
        match prefixed.1 {
            RuntimeMetadata::V14(metadata) => metadata,
            other => panic!("expected v14 fixture metadata, got {other:?}"),
        }
    }

    fn metadata_from_v14(metadata: v14::RuntimeMetadataV14) -> ArcMetadata {
        let prefixed =
            RuntimeMetadataPrefixed(u32::from_le_bytes(*b"meta"), RuntimeMetadata::V14(metadata));
        ArcMetadata::from(Metadata::try_from(prefixed).unwrap())
    }

    fn state_with_metadata(metadata: ArcMetadata) -> OfflineChainState {
        OfflineChainState {
            metadata,
            ..bulletin_chain_state()
        }
    }

    fn extension_by_identifier(
        metadata: &v14::RuntimeMetadataV14,
        identifier: &str,
    ) -> v14::SignedExtensionMetadata<scale_info::form::PortableForm> {
        metadata
            .extrinsic
            .signed_extensions
            .iter()
            .find(|extension| extension.identifier == identifier)
            .unwrap_or_else(|| panic!("fixture metadata lacks the {identifier} extension"))
            .clone()
    }

    #[test]
    fn preimage_key_is_blake2b_256() {
        assert_eq!(
            hex::encode(preimage_key(b"")),
            "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8"
        );
    }

    #[test]
    fn builds_and_signs_store_extrinsic_against_fixture() {
        let state = bulletin_chain_state();
        let data = b"hello bulletin".to_vec();
        let client = state.client_at(anchor_fixture().number).unwrap();
        let signed =
            build_signed_store_transaction(&client, &anchor_fixture(), &signer_fixture(), 7, &data)
                .unwrap();

        let (account, signature, tail) = split_v4(signed.encoded());
        assert_eq!(account, signer_fixture().public_key());
        let payload = StaticPayload::new(STORE_PALLET_NAME, STORE_CALL_NAME, StoreCallData(&data));
        let call_data = client.tx().call_data(&payload).unwrap();
        assert!(tail.ends_with(&call_data));

        // The signature must verify over the reconstructed signer payload.
        let params = DefaultExtrinsicParamsBuilder::<SubstrateConfig>::new()
            .nonce(7)
            .mortal_from_unchecked(
                MORTAL_PERIOD_BLOCKS,
                anchor_fixture().number,
                H256(anchor_fixture().hash),
            )
            .build();
        let payload = StaticPayload::new(STORE_PALLET_NAME, STORE_CALL_NAME, StoreCallData(&data));
        let signer_payload = client
            .tx()
            .create_v4_signable_offline(&payload, params)
            .unwrap()
            .signer_payload()
            .unwrap();
        let public = PublicKey::from_bytes(&account).unwrap();
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    &signer_payload,
                    &Signature::from_bytes(&signature).unwrap()
                )
                .is_ok()
        );
    }

    #[test]
    fn genesis_hash_binds_the_signature() {
        let data = b"pinned to one chain".to_vec();
        let client = bulletin_chain_state()
            .client_at(anchor_fixture().number)
            .unwrap();
        let signed =
            build_signed_store_transaction(&client, &anchor_fixture(), &signer_fixture(), 0, &data)
                .unwrap();
        let (account, signature, _) = split_v4(signed.encoded());

        let mutated_state = OfflineChainState {
            genesis_hash: [0xcc; 32],
            ..bulletin_chain_state()
        };
        let client = mutated_state.client_at(anchor_fixture().number).unwrap();
        let params = DefaultExtrinsicParamsBuilder::<SubstrateConfig>::new()
            .mortal_from_unchecked(
                MORTAL_PERIOD_BLOCKS,
                anchor_fixture().number,
                H256(anchor_fixture().hash),
            )
            .build();
        let payload = StaticPayload::new(STORE_PALLET_NAME, STORE_CALL_NAME, StoreCallData(&data));
        let mutated_payload = client
            .tx()
            .create_v4_signable_offline(&payload, params)
            .unwrap()
            .signer_payload()
            .unwrap();

        let public = PublicKey::from_bytes(&account).unwrap();
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    &mutated_payload,
                    &Signature::from_bytes(&signature).unwrap()
                )
                .is_err()
        );
    }

    #[test]
    fn rejects_mutated_store_argument_type() {
        // Point the store call's `data` field at a non-u8-sequence type: the
        // CheckSpecVersion additional (u32) borrowed from the extension list.
        let mut metadata = bulletin_metadata_v14();
        let u32_type = extension_by_identifier(&metadata, "CheckSpecVersion").additional_signed;
        let calls_type_id = metadata
            .pallets
            .iter()
            .find(|pallet| pallet.name == "TransactionStorage")
            .unwrap()
            .calls
            .as_ref()
            .unwrap()
            .ty
            .id;
        let calls_type = metadata
            .types
            .types
            .iter_mut()
            .find(|ty| ty.id == calls_type_id)
            .unwrap();
        let scale_info::TypeDef::Variant(variants) = &mut calls_type.ty.type_def else {
            panic!("calls type is not a variant");
        };
        let store = variants
            .variants
            .iter_mut()
            .find(|variant| variant.name == "store")
            .unwrap();
        store.fields[0].ty = u32_type;

        let state = state_with_metadata(metadata_from_v14(metadata));
        let client = state.client_at(anchor_fixture().number).unwrap();
        let error = build_signed_store_transaction(
            &client,
            &anchor_fixture(),
            &signer_fixture(),
            0,
            &[1, 2, 3],
        )
        .unwrap_err();
        assert!(error.contains("not a"), "{error}");
    }

    #[test]
    fn unknown_extension_with_non_empty_implicit_errors() {
        let mut metadata = bulletin_metadata_v14();
        let mut fake = extension_by_identifier(&metadata, "CheckSpecVersion");
        fake.identifier = "FakeImplicitExt".to_string();
        metadata.extrinsic.signed_extensions.push(fake);

        let state = state_with_metadata(metadata_from_v14(metadata));
        let client = state.client_at(anchor_fixture().number).unwrap();
        let error =
            build_signed_store_transaction(&client, &anchor_fixture(), &signer_fixture(), 0, &[1])
                .unwrap_err();
        assert!(error.contains("FakeImplicitExt"), "{error}");
    }

    #[test]
    fn unknown_extension_with_non_empty_value_errors() {
        let mut metadata = bulletin_metadata_v14();
        let mut fake = extension_by_identifier(&metadata, "CheckNonce");
        fake.identifier = "FakeValueExt".to_string();
        metadata.extrinsic.signed_extensions.push(fake);

        let state = state_with_metadata(metadata_from_v14(metadata));
        let client = state.client_at(anchor_fixture().number).unwrap();
        let error =
            build_signed_store_transaction(&client, &anchor_fixture(), &signer_fixture(), 0, &[1])
                .unwrap_err();
        assert!(error.contains("FakeValueExt"), "{error}");
    }

    #[test]
    fn unknown_extension_with_option_value_encodes_none() {
        // Accepted gap: an unknown extension whose extra is a bare `Option`
        // silently encodes `None` instead of erroring.
        let mut metadata = bulletin_metadata_v14();
        let option_type = extension_by_identifier(&metadata, "CheckMetadataHash").additional_signed;
        let empty_type = extension_by_identifier(&metadata, "CheckSpecVersion").ty;
        let mut fake = extension_by_identifier(&metadata, "CheckSpecVersion");
        fake.identifier = "FakeOptionExt".to_string();
        fake.ty = option_type;
        fake.additional_signed = empty_type;
        metadata.extrinsic.signed_extensions.push(fake);

        let state = state_with_metadata(metadata_from_v14(metadata));
        let baseline_client = bulletin_chain_state()
            .client_at(anchor_fixture().number)
            .unwrap();
        let baseline = build_signed_store_transaction(
            &baseline_client,
            &anchor_fixture(),
            &signer_fixture(),
            0,
            &[1],
        )
        .unwrap();
        let client = state.client_at(anchor_fixture().number).unwrap();
        let with_fake =
            build_signed_store_transaction(&client, &anchor_fixture(), &signer_fixture(), 0, &[1])
                .unwrap();
        assert_eq!(
            with_fake.encoded().len(),
            baseline.encoded().len() + 1,
            "the Option-typed extra should contribute exactly one None byte"
        );
    }

    #[test]
    fn builds_large_preimage_without_pathological_cost() {
        // The store call data must not encode per byte (scale-encode has no u8
        // fast path); the StoreCallData memcpy keeps an 8 MiB preimage cheap.
        // A generous bound catches an O(n^2)/per-byte-visitor regression
        // without being flaky under CI load.
        let data = vec![0x5au8; 8 * 1024 * 1024];
        let client = bulletin_chain_state()
            .client_at(anchor_fixture().number)
            .unwrap();
        let start = std::time::Instant::now();
        let signed =
            build_signed_store_transaction(&client, &anchor_fixture(), &signer_fixture(), 0, &data)
                .unwrap();
        let elapsed = start.elapsed();
        assert!(signed.encoded().len() > data.len());
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "building an 8 MiB store extrinsic took {elapsed:?}"
        );
    }

    #[test]
    fn rejects_secret_of_wrong_shape() {
        let error =
            allowance_signer(&BulletinAllowanceKey::from_secret_bytes(vec![0xff; 64]).unwrap())
                .unwrap_err();
        assert!(error.contains("invalid bulletin allowance key"), "{error}");
    }
}
