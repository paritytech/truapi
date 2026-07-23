//! Signed-extension encoding for the unsigned General (v5) `AsResources`
//! extrinsic, driven by live chain metadata.
//!
//! The extension **order** and per-extension type ids come from the runtime
//! metadata (`state_getMetadata`, V14/V15/V16); the per-extension `extra` /
//! `additional_signed` bytes come from a name-keyed encoder mirroring
//! signing-bot `src/core/create-transaction.ts` `encodeSignedExtensions`, with a
//! generic default for the personhood extensions (all `Option`/void).
//!
//! Two concatenations are derived from the same encoded list:
//! - the ring-VRF proof message (`build_proof_message`) over the extensions
//!   strictly *after* `AsResources` (host-spec inherited implication), and
//! - the full extrinsic body's `Σ extra` (see `extrinsic.rs`), over *all*
//!   extensions with `AsResources` carrying `Some(AsResourcesInfo)`.

use std::collections::HashMap;

use frame_metadata::RuntimeMetadata;
use frame_metadata::RuntimeMetadataPrefixed;
use parity_scale_codec::{Compact, Decode, Encode};
use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive};

/// Signed-extension identifier that carries the `AsResources` authorization.
pub const AS_RESOURCES: &str = "AsResources";

/// Chain state needed to fill the standard signed extensions.
#[derive(Debug, Clone, Copy)]
pub struct ChainState {
    /// Runtime `specVersion` (CheckSpecVersion implicit).
    pub spec_version: u32,
    /// Runtime `transactionVersion` (CheckTxVersion implicit).
    pub transaction_version: u32,
    /// Genesis block hash (CheckGenesis / CheckMortality implicit).
    pub genesis_hash: [u8; 32],
    /// Account nonce (CheckNonce extra); ignored by the unsigned path.
    pub nonce: u32,
}

/// A signed extension's identifier plus the type ids of its `extra` and
/// `additional_signed` fields, in metadata order.
struct ExtensionDef {
    identifier: String,
    extra_type: u32,
    additional_signed_type: u32,
}

/// A signed extension encoded to its `extra` and `additional_signed` bytes.
pub struct EncodedExtension {
    /// SCALE-encoded `extra` (goes into the extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded `additional_signed` (the implicit, part of the signed data).
    pub additional_signed: Vec<u8>,
}

/// Decoded metadata: the ordered signed-extension defs, the type registry, and
/// each storage entry's value type id (`(pallet, entry) -> type id`).
pub struct Metadata {
    extensions: Vec<ExtensionDef>,
    registry: PortableRegistry,
    storage_values: HashMap<(String, String), u32>,
    constants: HashMap<(String, String), Vec<u8>>,
}

/// Collect extensions, type registry, storage value types, and pallet constants
/// from decoded metadata; `$set` is the version's `StorageEntryType`.
macro_rules! collect_metadata {
    ($m:expr, $set:path) => {{
        let extensions = $m
            .extrinsic
            .signed_extensions
            .iter()
            .map(|e| ExtensionDef {
                identifier: e.identifier.clone(),
                extra_type: e.ty.id,
                additional_signed_type: e.additional_signed.id,
            })
            .collect();
        let mut storage_values = HashMap::new();
        let mut constants = HashMap::new();
        for pallet in &$m.pallets {
            for constant in &pallet.constants {
                constants.insert(
                    (pallet.name.clone(), constant.name.clone()),
                    constant.value.clone(),
                );
            }
            let Some(storage) = &pallet.storage else {
                continue;
            };
            for entry in &storage.entries {
                use $set as EntryType;
                let value_type = match &entry.ty {
                    EntryType::Plain(ty) => ty.id,
                    EntryType::Map { value, .. } => value.id,
                };
                storage_values.insert((pallet.name.clone(), entry.name.clone()), value_type);
            }
        }
        (extensions, $m.types, storage_values, constants)
    }};
}

macro_rules! collect_metadata_v16 {
    ($m:expr) => {{
        let extension_indexes = $m
            .extrinsic
            .transaction_extensions_by_version
            .get(&5)
            .map(|indexes| {
                indexes
                    .iter()
                    .map(|Compact(index)| *index as usize)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| (0..$m.extrinsic.transaction_extensions.len()).collect());
        let extensions = extension_indexes
            .into_iter()
            .filter_map(|index| $m.extrinsic.transaction_extensions.get(index))
            .map(|e| ExtensionDef {
                identifier: e.identifier.clone(),
                extra_type: e.ty.id,
                additional_signed_type: e.implicit.id,
            })
            .collect();
        let mut storage_values = HashMap::new();
        let mut constants = HashMap::new();
        for pallet in &$m.pallets {
            for constant in &pallet.constants {
                constants.insert(
                    (pallet.name.clone(), constant.name.clone()),
                    constant.value.clone(),
                );
            }
            let Some(storage) = &pallet.storage else {
                continue;
            };
            for entry in &storage.entries {
                use frame_metadata::v16::StorageEntryType as EntryType;
                let value_type = match &entry.ty {
                    EntryType::Plain(ty) => ty.id,
                    EntryType::Map { value, .. } => value.id,
                };
                storage_values.insert((pallet.name.clone(), entry.name.clone()), value_type);
            }
        }
        (extensions, $m.types, storage_values, constants)
    }};
}

impl Metadata {
    /// Decode `state_getMetadata` bytes (a `RuntimeMetadataPrefixed`, V14 or
    /// V15) into the ordered signed-extension defs, type registry, and storage
    /// value types.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let prefixed = RuntimeMetadataPrefixed::decode(&mut &bytes[..])
            .map_err(|err| format!("metadata decode failed: {err}"))?;
        let (extensions, registry, storage_values, constants) = match prefixed.1 {
            RuntimeMetadata::V14(m) => collect_metadata!(m, frame_metadata::v14::StorageEntryType),
            RuntimeMetadata::V15(m) => collect_metadata!(m, frame_metadata::v15::StorageEntryType),
            RuntimeMetadata::V16(m) => collect_metadata_v16!(m),
            other => return Err(format!("unsupported metadata version {}", other.version())),
        };
        Ok(Self {
            extensions,
            registry,
            storage_values,
            constants,
        })
    }

    /// The type registry, for dynamic decoding of storage values.
    pub fn registry(&self) -> &PortableRegistry {
        &self.registry
    }

    /// The value type id of storage entry `pallet::entry`, if present.
    pub fn storage_value_type(&self, pallet: &str, entry: &str) -> Option<u32> {
        self.storage_values
            .get(&(pallet.to_string(), entry.to_string()))
            .copied()
    }

    /// The SCALE-encoded value bytes of pallet constant `pallet::name`.
    pub fn constant(&self, pallet: &str, name: &str) -> Option<&[u8]> {
        self.constants
            .get(&(pallet.to_string(), name.to_string()))
            .map(Vec::as_slice)
    }

    /// Encode every signed extension in metadata order.
    pub fn encode_signed_extensions(&self, state: &ChainState) -> Vec<EncodedExtension> {
        self.extensions
            .iter()
            .map(|ext| {
                let (extra, additional_signed) = self.encode_one(ext, state);
                EncodedExtension {
                    extra,
                    additional_signed,
                }
            })
            .collect()
    }

    /// The signed-extension identifiers, in metadata order.
    #[cfg(test)]
    pub fn extension_ids(&self) -> Vec<&str> {
        self.extensions
            .iter()
            .map(|e| e.identifier.as_str())
            .collect()
    }

    /// Encode a single extension's `(extra, additional_signed)`, mirroring the
    /// signing-bot switch; unknown personhood extensions fall back to the
    /// metadata type default (`Option` -> None, void -> empty).
    fn encode_one(&self, ext: &ExtensionDef, state: &ChainState) -> (Vec<u8>, Vec<u8>) {
        match ext.identifier.as_str() {
            "CheckNonce" => (Compact(state.nonce).encode(), Vec::new()),
            "CheckSpecVersion" => (Vec::new(), state.spec_version.to_le_bytes().to_vec()),
            "CheckTxVersion" => (Vec::new(), state.transaction_version.to_le_bytes().to_vec()),
            "CheckGenesis" => (Vec::new(), state.genesis_hash.to_vec()),
            // extra = Era::Immortal (0x00); implicit = genesis hash.
            "CheckMortality" => (vec![0x00], state.genesis_hash.to_vec()),
            // extra = first variant `Disabled` (void) = 0x00.
            "VerifyMultiSignature" => (vec![0x00], Vec::new()),
            // extra = { tip: compact(0), asset_id: None } = 0x00 0x00.
            "ChargeAssetTxPayment" => (vec![0x00, 0x00], Vec::new()),
            // extra = bool false = 0x00.
            "RestrictOrigins" => (vec![0x00], Vec::new()),
            _ => (
                self.encode_default(ext.extra_type),
                self.encode_default(ext.additional_signed_type),
            ),
        }
    }

    /// Encode the "disabled" default value for a metadata type: `Option` -> None
    /// (`0x00`), void/empty tuple -> empty, enums -> first variant, primitives
    /// -> zero. Matches signing-bot `defaultValueForType`.
    fn encode_default(&self, type_id: u32) -> Vec<u8> {
        let Some(ty) = self.registry.resolve(type_id) else {
            return Vec::new();
        };
        match &ty.type_def {
            TypeDef::Composite(c) => c
                .fields
                .iter()
                .flat_map(|f| self.encode_default(f.ty.id))
                .collect(),
            TypeDef::Tuple(t) => t
                .fields
                .iter()
                .flat_map(|f| self.encode_default(f.id))
                .collect(),
            TypeDef::Variant(v) => {
                // Option<T> encodes None as 0x00.
                if ty.path.segments.last().map(String::as_str) == Some("Option") {
                    return vec![0x00];
                }
                match v.variants.iter().min_by_key(|var| var.index) {
                    None => Vec::new(),
                    Some(first) => {
                        let mut out = vec![first.index];
                        for field in &first.fields {
                            out.extend(self.encode_default(field.ty.id));
                        }
                        out
                    }
                }
            }
            TypeDef::Array(a) => {
                let elem = self.encode_default(a.type_param.id);
                elem.repeat(a.len as usize)
            }
            // Sequences / strings / bit-sequences encode an empty run as compact(0).
            TypeDef::Sequence(_) | TypeDef::BitSequence(_) => vec![0x00],
            TypeDef::Compact(_) => vec![0x00],
            TypeDef::Primitive(p) => match p {
                TypeDefPrimitive::Bool | TypeDefPrimitive::U8 | TypeDefPrimitive::I8 => vec![0],
                TypeDefPrimitive::Char | TypeDefPrimitive::U32 | TypeDefPrimitive::I32 => {
                    vec![0; 4]
                }
                TypeDefPrimitive::U16 | TypeDefPrimitive::I16 => vec![0; 2],
                TypeDefPrimitive::U64 | TypeDefPrimitive::I64 => vec![0; 8],
                TypeDefPrimitive::U128 | TypeDefPrimitive::I128 => vec![0; 16],
                TypeDefPrimitive::U256 | TypeDefPrimitive::I256 => vec![0; 32],
                // Length-prefixed string: empty = compact(0).
                TypeDefPrimitive::Str => vec![0x00],
            },
        }
    }

    /// Index of `AsResources` in the extension list, if present.
    pub fn as_resources_index(&self) -> Option<usize> {
        self.extensions
            .iter()
            .position(|e| e.identifier == AS_RESOURCES)
    }
}

/// Build the ring-VRF proof message for an `AsResources`-authorized call:
/// `blake2b256(0x00 ‖ call ‖ Σ tail.extra ‖ Σ tail.additional_signed)`, where
/// the tail is the extensions ordered strictly after `AsResources`. The leading
/// `0x00` is the General-transaction extension-version byte.
pub fn build_proof_message(
    metadata: &Metadata,
    call_data: &[u8],
    state: &ChainState,
) -> Result<[u8; 32], String> {
    let all = metadata.encode_signed_extensions(state);
    let tail_start = metadata
        .as_resources_index()
        .map(|i| i + 1)
        .ok_or_else(|| format!("{AS_RESOURCES} extension not found in metadata"))?;
    let tail = &all[tail_start..];

    let mut payload = Vec::with_capacity(1 + call_data.len());
    payload.push(0x00);
    payload.extend_from_slice(call_data);
    for ext in tail {
        payload.extend_from_slice(&ext.extra);
    }
    for ext in tail {
        payload.extend_from_slice(&ext.additional_signed);
    }
    Ok(blake2b256(&payload))
}

/// BLAKE2b-256 of `message`.
pub fn blake2b256(message: &[u8]) -> [u8; 32] {
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

    /// Fixture metadata captured from paseo-next-v2 (raw `RuntimeMetadataPrefixed`).
    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");

    /// The known-answer chain state frozen alongside the fixture.
    fn fixture_state() -> ChainState {
        ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
        }
    }

    /// `Resources.set_statement_store_account(period=7, seq=0, target=0)`.
    fn fixture_call() -> Vec<u8> {
        let mut call = vec![0x3f, 0x0a];
        call.extend_from_slice(&7u32.to_le_bytes());
        call.extend_from_slice(&0u32.to_le_bytes());
        call.extend_from_slice(&[0u8; 32]);
        call
    }

    #[test]
    fn proof_message_matches_frozen_known_answer() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let msg = build_proof_message(&metadata, &fixture_call(), &fixture_state()).unwrap();
        assert_eq!(
            hex::encode(msg),
            "1d2e6d8d8f421b0857097c6076115507432d66fea47ebe0c3be282a369f6743c",
        );
    }

    #[test]
    fn as_resources_tail_is_indices_10_through_20() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let idx = metadata.as_resources_index().unwrap();
        // AsResources sits at index 9; the proof tail is everything after it.
        assert_eq!(idx, 9);
        let ids = metadata.extension_ids();
        assert_eq!(
            ids[idx + 1..].to_vec(),
            vec![
                "AuthorizeCall",
                "RestrictOrigins",
                "CheckNonZeroSender",
                "CheckSpecVersion",
                "CheckTxVersion",
                "CheckGenesis",
                "CheckMortality",
                "CheckNonce",
                "CheckWeight",
                "ChargeAssetTxPayment",
                "StorageWeightReclaim",
            ],
        );
    }

    #[test]
    fn dropping_the_version_byte_changes_the_hash() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let state = fixture_state();
        let call = fixture_call();
        let all = metadata.encode_signed_extensions(&state);
        let tail = &all[metadata.as_resources_index().unwrap() + 1..];
        let mut without = call.clone();
        for e in tail {
            without.extend_from_slice(&e.extra);
        }
        for e in tail {
            without.extend_from_slice(&e.additional_signed);
        }
        assert_ne!(
            build_proof_message(&metadata, &call, &state).unwrap(),
            blake2b256(&without),
        );
    }
}
