//! Ring location resolution and Bandersnatch ring-VRF primitives.
//!
//! The resolver mirrors Nova's RFC-0004 implementation: it validates the
//! requested Members pallet from runtime metadata, pins every storage read to
//! one finalized block, selects the full-person key before the lite-person key,
//! and returns the ring members, exponent, and revision from that snapshot.

use std::sync::Arc;

use async_trait::async_trait;
use subxt::dynamic;
use subxt::ext::scale_decode::DecodeAsType;
use truapi::v01::{ProductProofContext, RingLocation, RingLocationJunction};
use verifiable::GenerateVerifiable;
use verifiable::ring::RingDomainSize;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;
use zeroize::Zeroizing;

use crate::chain_runtime::ChainRuntime;
use crate::host_logic::sso::messages::RingVrfError;

const MEMBERS_PALLET: &str = "Members";
const FULL_PERSON_COLLECTION: [u8; 32] = *b"pop:polkadot.network/people     ";
const LITE_PERSON_COLLECTION: [u8; 32] = *b"pop:polkadot.network/people-lite";
const FULL_PERSON_ENTROPY_KEY: &[u8] = b"candidate";

type RingMember = <BandersnatchVrfVerifiable as GenerateVerifiable>::Member;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PersonKey {
    Full,
    Lite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MemberCandidate {
    pub(super) key: PersonKey,
    pub(super) member: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResolvedRing {
    pub(super) selected: MemberCandidate,
    pub(super) ring_index: u32,
    pub(super) ring_revision: u32,
    pub(super) domain_size: RingDomainSize,
    pub(super) members: Vec<[u8; 32]>,
}

#[async_trait]
pub(super) trait RingResolver: Send + Sync {
    /// Validate the chain and Members pallet, returning the requested
    /// collection (or the RFC-0004 full-person fallback).
    async fn validate(&self, location: &RingLocation) -> Result<[u8; 32], RingVrfError>;

    /// Resolve a current, single-block ring snapshot and select the first
    /// candidate with an active membership.
    async fn resolve(
        &self,
        location: &RingLocation,
        candidates: &[MemberCandidate],
    ) -> Result<ResolvedRing, RingVrfError>;
}

pub(super) struct ChainRingResolver {
    chain: ChainRuntime,
}

impl ChainRingResolver {
    pub(super) fn new(chain: ChainRuntime) -> Arc<Self> {
        Arc::new(Self { chain })
    }

    async fn at_ring(
        &self,
        location: &RingLocation,
    ) -> Result<
        subxt::client::OnlineClientAtBlock<subxt::config::substrate::SubstrateConfig>,
        RingVrfError,
    > {
        let client = self
            .chain
            .online_client(&location.chain_id)
            .await
            .map_err(unknown)?;
        let at_block = client.at_current_block().await.map_err(unknown)?;
        let Some(pallet) = at_block.metadata_ref().pallet_by_name(MEMBERS_PALLET) else {
            return Err(RingVrfError::RingNotFound);
        };
        if let Some(expected) = pallet_instance(location)
            && pallet.call_index() != expected
        {
            return Err(RingVrfError::RingNotFound);
        }
        Ok(at_block)
    }
}

#[async_trait]
impl RingResolver for ChainRingResolver {
    async fn validate(&self, location: &RingLocation) -> Result<[u8; 32], RingVrfError> {
        self.at_ring(location).await?;
        collection_id(location)
    }

    async fn resolve(
        &self,
        location: &RingLocation,
        candidates: &[MemberCandidate],
    ) -> Result<ResolvedRing, RingVrfError> {
        let collection = collection_id(location)?;
        let at_block = self.at_ring(location).await?;
        let storage = at_block.storage();

        let mut selected = None;
        for candidate in candidates {
            let address =
                dynamic::storage::<([u8; 32], [u8; 32]), RingPosition>(MEMBERS_PALLET, "Members");
            let Some(position) = storage
                .try_fetch(address, (collection, candidate.member))
                .await
                .map_err(unknown)?
            else {
                continue;
            };
            let RingPosition::Included {
                ring_index,
                ring_position,
                ..
            } = position.decode().map_err(unknown)?
            else {
                continue;
            };

            let status_address =
                dynamic::storage::<([u8; 32], u32), RingStatus>(MEMBERS_PALLET, "RingKeysStatus");
            let status = storage
                .fetch(status_address, (collection, ring_index))
                .await
                .map_err(unknown)?
                .decode()
                .map_err(unknown)?;
            if status.included > ring_position {
                selected = Some((*candidate, ring_index, status.included));
                break;
            }
        }

        let Some((selected, ring_index, included_count)) = selected else {
            return Err(RingVrfError::NotMember);
        };

        let collection_address =
            dynamic::storage::<([u8; 32],), CollectionInfo>(MEMBERS_PALLET, "Collections");
        let Some(collection_info) = storage
            .try_fetch(collection_address, (collection,))
            .await
            .map_err(unknown)?
        else {
            return Err(RingVrfError::RingNotFound);
        };
        let collection_info = collection_info.decode().map_err(unknown)?;

        let ring_keys_address =
            dynamic::storage::<([u8; 32], u32, u32), BoundedMembers>(MEMBERS_PALLET, "RingKeys");
        let mut pages = storage
            .iter(ring_keys_address, (collection, ring_index))
            .await
            .map_err(unknown)?;
        let mut members_by_page = Vec::new();
        while let Some(entry) = pages.next().await {
            let entry = entry.map_err(unknown)?;
            let page_index = entry
                .key()
                .map_err(unknown)?
                .part(2)
                .ok_or_else(|| RingVrfError::Unknown {
                    reason: "Members.RingKeys returned a key without a page index".to_string(),
                })?
                .decode_as::<u32>()
                .map_err(unknown)?
                .ok_or_else(|| RingVrfError::Unknown {
                    reason: "Members.RingKeys page index is not recoverable".to_string(),
                })?;
            let members = entry.value().decode().map_err(unknown)?.0;
            members_by_page.push((page_index, members));
        }
        members_by_page.sort_unstable_by_key(|(page, _)| *page);
        let mut members: Vec<_> = members_by_page
            .into_iter()
            .flat_map(|(_, members)| members)
            .collect();
        let included_count = usize::try_from(included_count).map_err(unknown)?;
        if members.len() < included_count {
            return Err(RingVrfError::Unknown {
                reason: format!(
                    "Members.RingKeys contains {} keys but RingKeysStatus includes {included_count}",
                    members.len()
                ),
            });
        }
        members.truncate(included_count);
        if !members.contains(&selected.member) {
            return Err(RingVrfError::NotMember);
        }

        let root_address = dynamic::storage::<([u8; 32], u32), RingRoot>(MEMBERS_PALLET, "Root");
        let Some(root) = storage
            .try_fetch(root_address, (collection, ring_index))
            .await
            .map_err(unknown)?
        else {
            return Err(RingVrfError::RingNotFound);
        };
        let ring_revision = root.decode().map_err(unknown)?.revision;

        Ok(ResolvedRing {
            selected,
            ring_index,
            ring_revision,
            domain_size: collection_info.ring_size.domain_size(),
            members,
        })
    }
}

pub(super) fn context_bytes(context: &ProductProofContext) -> [u8; 32] {
    let mut input = Vec::with_capacity(9 + context.product_id.len() + context.suffix.len());
    input.extend_from_slice(b"product/");
    input.extend_from_slice(context.product_id.as_bytes());
    input.push(b'/');
    input.extend_from_slice(&context.suffix);
    blake2b_256(&input, None)
}

pub(super) fn person_entropy(root_entropy: &[u8], key: PersonKey) -> Zeroizing<[u8; 32]> {
    let key = match key {
        PersonKey::Full => Some(FULL_PERSON_ENTROPY_KEY),
        PersonKey::Lite => None,
    };
    Zeroizing::new(blake2b_256(root_entropy, key))
}

pub(super) fn member_from_entropy(entropy: &[u8; 32]) -> Result<[u8; 32], RingVrfError> {
    use parity_scale_codec::Encode;

    let secret = BandersnatchVrfVerifiable::new_secret(*entropy);
    BandersnatchVrfVerifiable::member_from_secret(&secret)
        .encode()
        .try_into()
        .map_err(|member: Vec<u8>| RingVrfError::Unknown {
            reason: format!(
                "Bandersnatch member encoded to {} bytes instead of 32",
                member.len()
            ),
        })
}

pub(super) fn alias_from_entropy(
    entropy: &[u8; 32],
    context: &[u8],
) -> Result<[u8; 32], RingVrfError> {
    let secret = BandersnatchVrfVerifiable::new_secret(*entropy);
    BandersnatchVrfVerifiable::alias_in_context(&secret, context).map_err(unknown)
}

pub(super) fn create_proof(
    entropy: &[u8; 32],
    resolved: &ResolvedRing,
    context: &[u8],
    message: &[u8],
) -> Result<(Vec<u8>, [u8; 32]), RingVrfError> {
    use parity_scale_codec::Decode;

    let mut selected_bytes = &resolved.selected.member[..];
    let selected = RingMember::decode(&mut selected_bytes).map_err(unknown)?;
    let members = resolved
        .members
        .iter()
        .map(|member| {
            let mut bytes = &member[..];
            RingMember::decode(&mut bytes).map_err(unknown)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let secret = BandersnatchVrfVerifiable::new_secret(*entropy);
    let prover =
        BandersnatchVrfVerifiable::open(resolved.domain_size, &selected, members.into_iter())
            .map_err(|_| RingVrfError::NotMember)?;
    let (proof, alias) =
        BandersnatchVrfVerifiable::create(prover, &secret, context, message).map_err(unknown)?;
    Ok((proof.to_vec(), alias))
}

pub(super) fn key_for_collection(collection: &[u8; 32]) -> PersonKey {
    if collection == &LITE_PERSON_COLLECTION {
        PersonKey::Lite
    } else {
        PersonKey::Full
    }
}

fn collection_id(location: &RingLocation) -> Result<[u8; 32], RingVrfError> {
    location
        .junctions
        .iter()
        .find_map(|junction| match junction {
            RingLocationJunction::CollectionId(value) => Some(value),
            RingLocationJunction::PalletInstance(_) => None,
        })
        .map_or(Ok(FULL_PERSON_COLLECTION), |value| {
            value
                .as_slice()
                .try_into()
                .map_err(|_| RingVrfError::RingNotFound)
        })
}

fn pallet_instance(location: &RingLocation) -> Option<u8> {
    location
        .junctions
        .iter()
        .find_map(|junction| match junction {
            RingLocationJunction::PalletInstance(index) => Some(*index),
            RingLocationJunction::CollectionId(_) => None,
        })
}

fn blake2b_256(input: &[u8], key: Option<&[u8]>) -> [u8; 32] {
    let mut params = blake2b_simd::Params::new();
    params.hash_length(32);
    if let Some(key) = key {
        params.key(key);
    }
    let hash = params.hash(input);
    let mut output = [0u8; 32];
    output.copy_from_slice(hash.as_bytes());
    output
}

fn unknown(error: impl std::fmt::Debug) -> RingVrfError {
    RingVrfError::Unknown {
        reason: format!("{error:?}"),
    }
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
enum RingPosition {
    Onboarding {},
    Included { ring_index: u32, ring_position: u32 },
    Suspended,
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct RingStatus {
    included: u32,
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct CollectionInfo {
    ring_size: RingExponent,
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
enum RingExponent {
    R2e9,
    R2e10,
    R2e14,
}

impl RingExponent {
    fn domain_size(self) -> RingDomainSize {
        match self {
            Self::R2e9 => RingDomainSize::Domain11,
            Self::R2e10 => RingDomainSize::Domain12,
            Self::R2e14 => RingDomainSize::Domain16,
        }
    }
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct BoundedMembers(Vec<[u8; 32]>);

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct RingRoot {
    revision: u32,
}

#[cfg(test)]
mod tests {
    use parity_scale_codec::Encode;
    use scale_info::TypeInfo;

    use super::*;

    fn decode_as<A, B>(source: A) -> B
    where
        A: Encode + TypeInfo + 'static,
        B: DecodeAsType,
    {
        let mut registry = scale_info::Registry::new();
        let type_id = registry.register_type(&scale_info::meta_type::<A>()).id;
        let types: scale_info::PortableRegistry = registry.into();
        B::decode_as_type(&mut &*source.encode(), type_id, &types).expect("dynamic decode succeeds")
    }

    #[test]
    fn context_matches_rfc_0004_vector() {
        let context = ProductProofContext {
            product_id: "example.dot".to_string(),
            suffix: b"login".to_vec(),
        };
        assert_eq!(
            hex::encode(context_bytes(&context)),
            "be397823154bdcc0f4d86938af932cd4d5c49d0793e0138663ccdb3d8e0062eb"
        );
    }

    #[test]
    fn context_matches_ios_host_vector() {
        let context = ProductProofContext {
            product_id: "voting.dot".to_string(),
            suffix: vec![0, 1, 2, 3],
        };
        assert_eq!(
            hex::encode(context_bytes(&context)),
            "03fba4e4f9ce1b2eb228e79b8aabef71213cfc53bec6dcae9d24a075a2d5a89e"
        );
    }

    #[test]
    fn collection_selects_corresponding_person_key() {
        assert_eq!(key_for_collection(&FULL_PERSON_COLLECTION), PersonKey::Full);
        assert_eq!(key_for_collection(&LITE_PERSON_COLLECTION), PersonKey::Lite);
        assert_eq!(key_for_collection(&[0xff; 32]), PersonKey::Full);
    }

    #[test]
    fn missing_collection_defaults_to_full_personhood() {
        let location = RingLocation {
            chain_id: [0; 32],
            junctions: vec![RingLocationJunction::PalletInstance(42)],
        };
        assert_eq!(collection_id(&location), Ok(FULL_PERSON_COLLECTION));
    }

    #[test]
    fn storage_projections_decode_runtime_shapes() {
        #[derive(Encode, TypeInfo)]
        enum SourceRingPosition {
            Onboarding {
                queue_page: u32,
                queued_at: u64,
            },
            Included {
                ring_index: u32,
                ring_page: u32,
                ring_position: u32,
            },
            Suspended,
        }
        #[derive(Encode, TypeInfo)]
        enum SourceRingExponent {
            R2e9,
            R2e10,
            R2e14,
        }
        #[derive(Encode, TypeInfo)]
        struct SourceCollectionInfo {
            owner: u8,
            mode: u8,
            ring_size: SourceRingExponent,
            self_inclusion_delay: Option<u64>,
        }
        #[derive(Encode, TypeInfo)]
        struct SourceBoundedMembers(Vec<[u8; 32]>);
        #[derive(Encode, TypeInfo)]
        struct SourceRingRoot {
            root: [u8; 4],
            revision: u32,
            intermediate: [u8; 8],
        }

        assert_eq!(
            decode_as::<_, RingPosition>(SourceRingPosition::Included {
                ring_index: 7,
                ring_page: 3,
                ring_position: 19,
            }),
            RingPosition::Included {
                ring_index: 7,
                ring_position: 19,
            }
        );
        assert_eq!(
            decode_as::<_, CollectionInfo>(SourceCollectionInfo {
                owner: 1,
                mode: 0,
                ring_size: SourceRingExponent::R2e10,
                self_inclusion_delay: Some(3_600),
            }),
            CollectionInfo {
                ring_size: RingExponent::R2e10,
            }
        );
        assert_eq!(
            decode_as::<_, BoundedMembers>(SourceBoundedMembers(vec![[0x11; 32], [0x22; 32]])),
            BoundedMembers(vec![[0x11; 32], [0x22; 32]])
        );
        assert_eq!(
            decode_as::<_, RingRoot>(SourceRingRoot {
                root: [0x33; 4],
                revision: 12,
                intermediate: [0x44; 8],
            }),
            RingRoot { revision: 12 }
        );

        // Keep every runtime variant in the type registry so this test also
        // checks their names remain aligned with the partial decoder.
        let _ = SourceRingPosition::Onboarding {
            queue_page: 0,
            queued_at: 0,
        };
        let _ = SourceRingPosition::Suspended;
        let _ = SourceRingExponent::R2e9;
        let _ = SourceRingExponent::R2e14;
    }
}
