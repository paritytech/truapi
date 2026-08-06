//! Native mirrors of the user-confirmation review tree.
//!
//! One `uniffi::Record`/`uniffi::Enum` mirror per type reachable from
//! [`truapi_platform::UserConfirmationReview`], converted core→host only.
//! Fixed-size byte arrays widen to `Vec<u8>` for the FFI surface.

use truapi::v01;
use truapi_platform::UserConfirmationReview;

/// Account selector within a product subtree (RFC 0022): plain index or raw
/// 32-byte derivation index.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DerivationIndex {
    /// Plain account index — the primary, enumerable form.
    Index(u32),
    /// Raw 32-byte derivation index (widened to `Vec<u8>` for FFI).
    Raw(Vec<u8>),
}

impl From<v01::DerivationIndex> for DerivationIndex {
    fn from(index: v01::DerivationIndex) -> Self {
        match index {
            v01::DerivationIndex::Left(index) => Self::Index(index),
            v01::DerivationIndex::Right(raw) => Self::Raw(raw.to_vec()),
        }
    }
}

impl TryFrom<DerivationIndex> for v01::DerivationIndex {
    type Error = String;

    fn try_from(index: DerivationIndex) -> Result<Self, Self::Error> {
        match index {
            DerivationIndex::Index(index) => Ok(Self::Left(index)),
            DerivationIndex::Raw(raw) => raw.try_into().map(Self::Right).map_err(|raw: Vec<u8>| {
                format!("raw derivation index must be 32 bytes, got {}", raw.len())
            }),
        }
    }
}

/// Product account identifier: dotNS domain plus derivation index.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProductAccountId {
    /// A dotNS domain name identifier (e.g. `"my-product.dot"`).
    pub dot_ns_identifier: String,
    /// Account selector within the product subtree.
    pub derivation_index: DerivationIndex,
}

impl From<v01::ProductAccountId> for ProductAccountId {
    fn from(account: v01::ProductAccountId) -> Self {
        let v01::ProductAccountId {
            dot_ns_identifier,
            derivation_index,
        } = account;
        Self {
            dot_ns_identifier,
            derivation_index: derivation_index.into(),
        }
    }
}

impl TryFrom<ProductAccountId> for v01::ProductAccountId {
    type Error = String;

    fn try_from(account: ProductAccountId) -> Result<Self, Self::Error> {
        Ok(Self {
            dot_ns_identifier: account.dot_ns_identifier,
            derivation_index: account.derivation_index.try_into()?,
        })
    }
}

/// Raw data to sign — binary bytes or a string message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RawPayload {
    /// Raw binary data to sign.
    Bytes {
        /// Raw binary payload bytes.
        bytes: Vec<u8>,
    },
    /// String message to sign.
    Payload {
        /// String payload to sign.
        payload: String,
    },
}

impl From<v01::RawPayload> for RawPayload {
    fn from(payload: v01::RawPayload) -> Self {
        match payload {
            v01::RawPayload::Bytes { bytes } => Self::Bytes { bytes },
            v01::RawPayload::Payload { payload } => Self::Payload { payload },
        }
    }
}

/// Full Substrate extrinsic signing payload with all fields needed for
/// signature generation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignPayloadData {
    /// Reference block hash.
    pub block_hash: Vec<u8>,
    /// Reference block number.
    pub block_number: Vec<u8>,
    /// Mortality era encoding.
    pub era: Vec<u8>,
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// SCALE-encoded call data.
    pub method: Vec<u8>,
    /// Account nonce.
    pub nonce: Vec<u8>,
    /// Runtime spec version.
    pub spec_version: Vec<u8>,
    /// Transaction tip.
    pub tip: Vec<u8>,
    /// Transaction format version.
    pub transaction_version: Vec<u8>,
    /// Extension identifiers.
    pub signed_extensions: Vec<String>,
    /// Extrinsic version.
    pub version: u32,
    /// For multi-asset tips.
    pub asset_id: Option<Vec<u8>>,
    /// CheckMetadataHash extension.
    pub metadata_hash: Option<Vec<u8>>,
    /// Metadata mode.
    pub mode: Option<u32>,
    /// Request signed transaction back.
    pub with_signed_transaction: Option<bool>,
}

impl From<v01::HostSignPayloadData> for SignPayloadData {
    fn from(data: v01::HostSignPayloadData) -> Self {
        let v01::HostSignPayloadData {
            block_hash,
            block_number,
            era,
            genesis_hash,
            method,
            nonce,
            spec_version,
            tip,
            transaction_version,
            signed_extensions,
            version,
            asset_id,
            metadata_hash,
            mode,
            with_signed_transaction,
        } = data;
        Self {
            block_hash,
            block_number,
            era,
            genesis_hash,
            method,
            nonce,
            spec_version,
            tip,
            transaction_version,
            signed_extensions,
            version,
            asset_id,
            metadata_hash,
            mode,
            with_signed_transaction,
        }
    }
}

/// Review for a sign-payload request.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SignPayloadReview {
    /// Product-account signing request.
    Product {
        /// Product account that will sign this payload.
        account: ProductAccountId,
        /// The extrinsic payload to sign.
        payload: SignPayloadData,
    },
    /// Legacy-account signing request.
    LegacyAccount {
        /// Signer address (SS58 or hex) of the legacy account.
        signer: String,
        /// The extrinsic payload to sign.
        payload: SignPayloadData,
    },
}

impl From<truapi_platform::SignPayloadReview> for SignPayloadReview {
    fn from(review: truapi_platform::SignPayloadReview) -> Self {
        match review {
            truapi_platform::SignPayloadReview::Product(request) => Self::Product {
                account: request.account.into(),
                payload: request.payload.into(),
            },
            truapi_platform::SignPayloadReview::LegacyAccount(request) => Self::LegacyAccount {
                signer: request.signer,
                payload: request.payload.into(),
            },
        }
    }
}

/// Review for a sign-raw request.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SignRawReview {
    /// Product-account raw signing request.
    Product {
        /// Product account that will sign this payload.
        account: ProductAccountId,
        /// The payload to sign.
        payload: RawPayload,
    },
    /// Legacy-account raw signing request.
    LegacyAccount {
        /// Signer address (SS58 or hex) of the legacy account.
        signer: String,
        /// The payload to sign.
        payload: RawPayload,
    },
}

impl From<truapi_platform::SignRawReview> for SignRawReview {
    fn from(review: truapi_platform::SignRawReview) -> Self {
        match review {
            truapi_platform::SignRawReview::Product(request) => Self::Product {
                account: request.account.into(),
                payload: request.payload.into(),
            },
            truapi_platform::SignRawReview::LegacyAccount(request) => Self::LegacyAccount {
                signer: request.signer,
                payload: request.payload.into(),
            },
        }
    }
}

/// Review for a Statement Store proof signature. The payload is the exact
/// unsigned statement, signed as-is (no `<Bytes>` envelope), so hosts must not
/// present it with the raw-signing convention.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StatementStoreProductSignReview {
    /// Product account that will sign the statement payload.
    pub account: ProductAccountId,
    /// Exact unsigned statement payload to be signed.
    pub payload: Vec<u8>,
}

impl From<truapi_platform::StatementStoreProductSignReview> for StatementStoreProductSignReview {
    fn from(review: truapi_platform::StatementStoreProductSignReview) -> Self {
        Self {
            account: review.account.into(),
            payload: review.payload,
        }
    }
}

/// One transaction extension supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TxPayloadExtension {
    /// Extension name (e.g. `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Vec<u8>,
}

impl From<v01::TxPayloadExtension> for TxPayloadExtension {
    fn from(extension: v01::TxPayloadExtension) -> Self {
        let v01::TxPayloadExtension {
            id,
            extra,
            additional_signed,
        } = extension;
        Self {
            id,
            extra,
            additional_signed,
        }
    }
}

/// Review for transaction creation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CreateTransactionReview {
    /// Product-account transaction request.
    Product {
        /// Product account that will sign the transaction.
        signer: ProductAccountId,
        /// Genesis hash of the chain where the transaction will execute.
        genesis_hash: Vec<u8>,
        /// SCALE-encoded Call data.
        call_data: Vec<u8>,
        /// Transaction extensions supplied by the caller.
        extensions: Vec<TxPayloadExtension>,
        /// 0 for Extrinsic V4, runtime-supported value for V5.
        tx_ext_version: u8,
    },
    /// Legacy-account transaction request.
    LegacyAccount {
        /// Raw 32-byte public key of the legacy account.
        signer: Vec<u8>,
        /// Genesis hash of the chain where the transaction will execute.
        genesis_hash: Vec<u8>,
        /// SCALE-encoded Call data.
        call_data: Vec<u8>,
        /// Transaction extensions supplied by the caller.
        extensions: Vec<TxPayloadExtension>,
        /// 0 for Extrinsic V4, runtime-supported value for V5.
        tx_ext_version: u8,
    },
}

impl From<truapi_platform::CreateTransactionReview> for CreateTransactionReview {
    fn from(review: truapi_platform::CreateTransactionReview) -> Self {
        match review {
            truapi_platform::CreateTransactionReview::Product(payload) => Self::Product {
                signer: payload.signer.into(),
                genesis_hash: payload.genesis_hash.to_vec(),
                call_data: payload.call_data,
                extensions: payload.extensions.into_iter().map(Into::into).collect(),
                tx_ext_version: payload.tx_ext_version,
            },
            truapi_platform::CreateTransactionReview::LegacyAccount(payload) => {
                Self::LegacyAccount {
                    signer: payload.signer.to_vec(),
                    genesis_hash: payload.genesis_hash.to_vec(),
                    call_data: payload.call_data,
                    extensions: payload.extensions.into_iter().map(Into::into).collect(),
                    tx_ext_version: payload.tx_ext_version,
                }
            }
        }
    }
}

/// A single step in a [`RingLocation`] path, addressing a ring within a chain.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RingLocationJunction {
    /// Pallet instance hosting the ring collection.
    PalletInstance(u8),
    /// Ring collection identifier within the pallet.
    CollectionId(Vec<u8>),
}

impl From<v01::RingLocationJunction> for RingLocationJunction {
    fn from(junction: v01::RingLocationJunction) -> Self {
        match junction {
            v01::RingLocationJunction::PalletInstance(instance) => Self::PalletInstance(instance),
            v01::RingLocationJunction::CollectionId(id) => Self::CollectionId(id),
        }
    }
}

impl From<RingLocationJunction> for v01::RingLocationJunction {
    fn from(junction: RingLocationJunction) -> Self {
        match junction {
            RingLocationJunction::PalletInstance(instance) => Self::PalletInstance(instance),
            RingLocationJunction::CollectionId(id) => Self::CollectionId(id),
        }
    }
}

/// Locates a ring for ring VRF operations.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RingLocation {
    /// Genesis hash of the chain hosting the ring.
    pub chain_id: Vec<u8>,
    /// Path addressing the ring within the chain.
    pub junctions: Vec<RingLocationJunction>,
}

impl From<v01::RingLocation> for RingLocation {
    fn from(location: v01::RingLocation) -> Self {
        Self {
            chain_id: location.chain_id.to_vec(),
            junctions: location.junctions.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<RingLocation> for v01::RingLocation {
    type Error = String;

    fn try_from(location: RingLocation) -> Result<Self, Self::Error> {
        let chain_id = location.chain_id.try_into().map_err(|chain_id: Vec<u8>| {
            format!("ring chain id must be 32 bytes, got {}", chain_id.len())
        })?;
        Ok(Self {
            chain_id,
            junctions: location.junctions.into_iter().map(Into::into).collect(),
        })
    }
}

/// A product-scoped proof context: a product and a context within it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProductProofContext {
    /// dotNS product identifier (e.g. `"my-product.dot"`) scoping the context.
    pub product_id: String,
    /// Selector distinguishing contexts within the product; expands to the
    /// same 32-byte derivation index as [`ProductAccountId::derivation_index`].
    pub suffix: DerivationIndex,
}

impl From<v01::ProductProofContext> for ProductProofContext {
    fn from(context: v01::ProductProofContext) -> Self {
        let v01::ProductProofContext { product_id, suffix } = context;
        Self {
            product_id,
            suffix: suffix.into(),
        }
    }
}

/// Review for contextual-alias derivation (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountAliasReview {
    /// Product requesting the alias.
    pub calling_product_id: String,
    /// Product-scoped context the alias is bound to.
    pub context: ProductProofContext,
    /// Ring the alias is derived against.
    pub ring_location: RingLocation,
}

impl From<truapi_platform::AccountAliasReview> for AccountAliasReview {
    fn from(review: truapi_platform::AccountAliasReview) -> Self {
        Self {
            calling_product_id: review.calling_product_id,
            context: review.context.into(),
            ring_location: review.ring_location.into(),
        }
    }
}

/// Review for ring-VRF proof creation (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CreateProofReview {
    /// Product requesting the proof.
    pub calling_product_id: String,
    /// Product-scoped context the proof's alias is bound to.
    pub context: ProductProofContext,
    /// Ring the proof is generated against.
    pub ring_location: RingLocation,
    /// Opaque message bound into the proof.
    pub message: Vec<u8>,
}

impl From<truapi_platform::CreateProofReview> for CreateProofReview {
    fn from(review: truapi_platform::CreateProofReview) -> Self {
        Self {
            calling_product_id: review.calling_product_id,
            context: review.context.into(),
            ring_location: review.ring_location.into(),
            message: review.message,
        }
    }
}

/// One `append_message` call replayed against the signing transcript.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VrfTranscriptItem {
    /// Merlin `append_message` label.
    pub label: Vec<u8>,
    /// Merlin `append_message` value.
    pub value: Vec<u8>,
}

impl From<v01::VrfTranscriptItem> for VrfTranscriptItem {
    fn from(item: v01::VrfTranscriptItem) -> Self {
        let v01::VrfTranscriptItem { label, value } = item;
        Self { label, value }
    }
}

/// Request to produce an sr25519 VRF signature from a product account over a
/// caller-supplied Merlin transcript.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignVrfRequest {
    /// Account whose key signs the VRF.
    pub account: ProductAccountId,
    /// Root domain-separation label: `Transcript::new(transcript_label)`.
    pub transcript_label: Vec<u8>,
    /// Transcript items replayed in order as `append_message(label, value)`.
    pub items: Vec<VrfTranscriptItem>,
}

impl From<v01::HostAccountSignVrfRequest> for SignVrfRequest {
    fn from(request: v01::HostAccountSignVrfRequest) -> Self {
        let v01::HostAccountSignVrfRequest {
            account,
            transcript_label,
            items,
        } = request;
        Self {
            account: account.into(),
            transcript_label,
            items: items.into_iter().map(Into::into).collect(),
        }
    }
}

/// Review for RFC-0023 VRF transcript signing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignVrfReview {
    /// Product making the request.
    pub calling_product_id: String,
    /// Product account and exact ordered transcript.
    pub request: SignVrfRequest,
}

impl From<truapi_platform::SignVrfReview> for SignVrfReview {
    fn from(review: truapi_platform::SignVrfReview) -> Self {
        Self {
            calling_product_id: review.calling_product_id,
            request: review.request.into(),
        }
    }
}

/// A resource the host can pre-allocate on behalf of the product (RFC 0010).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AllocatableResource {
    /// Statement Store slot allowance for the product's own allowance account.
    StatementStoreAllowance,
    /// Bulletin chain slot allowance for the product's own allowance account.
    BulletinAllowance,
    /// Pre-warmed PGAS balance for the smart-contract account at the given
    /// derivation index.
    SmartContractAllowance(DerivationIndex),
    /// Permission to sign on the product's behalf without per-call prompts.
    AutoSigning,
}

impl From<v01::AllocatableResource> for AllocatableResource {
    fn from(resource: v01::AllocatableResource) -> Self {
        match resource {
            v01::AllocatableResource::StatementStoreAllowance => Self::StatementStoreAllowance,
            v01::AllocatableResource::BulletinAllowance => Self::BulletinAllowance,
            v01::AllocatableResource::SmartContractAllowance(index) => {
                Self::SmartContractAllowance(index.into())
            }
            v01::AllocatableResource::AutoSigning => Self::AutoSigning,
        }
    }
}

/// Review for resource allocation. Names the beneficiary product so the user
/// knows which product receives the (signing-capable) allowance key they are
/// approving.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ResourceAllocationReview {
    /// Product the allocation is requested for.
    pub calling_product_id: String,
    /// Resources to allocate.
    pub resources: Vec<AllocatableResource>,
}

impl From<truapi_platform::ResourceAllocationReview> for ResourceAllocationReview {
    fn from(review: truapi_platform::ResourceAllocationReview) -> Self {
        Self {
            calling_product_id: review.calling_product_id,
            resources: review.resources.into_iter().map(Into::into).collect(),
        }
    }
}

/// Review for cross-product account access.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountAccessReview {
    /// Product currently handling the request.
    pub requesting_product_id: String,
    /// Product whose account is being requested.
    pub target_product_id: String,
}

impl From<truapi_platform::AccountAccessReview> for AccountAccessReview {
    fn from(review: truapi_platform::AccountAccessReview) -> Self {
        Self {
            requesting_product_id: review.requesting_product_id,
            target_product_id: review.target_product_id,
        }
    }
}

/// Review for identity disclosure.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IdentityDisclosureReview {
    /// Product currently handling the request.
    pub product_id: String,
}

impl From<truapi_platform::IdentityDisclosureReview> for IdentityDisclosureReview {
    fn from(review: truapi_platform::IdentityDisclosureReview) -> Self {
        Self {
            product_id: review.product_id,
        }
    }
}

/// Review for preimage submission.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PreimageSubmitReview {
    /// Size of the preimage in bytes.
    pub size: u64,
}

impl From<truapi_platform::PreimageSubmitReview> for PreimageSubmitReview {
    fn from(review: truapi_platform::PreimageSubmitReview) -> Self {
        Self { size: review.size }
    }
}

/// Native-friendly mirror of [`truapi_platform::UserConfirmationReview`].
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeUserConfirmationReview {
    /// Sign a SCALE payload with a product or legacy account.
    SignPayload(SignPayloadReview),
    /// Sign raw bytes with a product or legacy account.
    SignRaw(SignRawReview),
    /// Sign a Statement Store proof payload with a product account.
    StatementStoreProductSign(StatementStoreProductSignReview),
    /// Create a transaction with a product or legacy account.
    CreateTransaction(CreateTransactionReview),
    /// Allow a product to derive a contextual alias for a ring.
    AccountAlias(AccountAliasReview),
    /// Allow a product to create a ring-VRF proof for a ring.
    CreateProof(CreateProofReview),
    /// Allow a product to learn the user's primary identity.
    IdentityDisclosure(IdentityDisclosureReview),
    /// Allocate resources for the requesting product.
    ResourceAllocation(ResourceAllocationReview),
    /// Submit a preimage to the host-selected backend.
    PreimageSubmit(PreimageSubmitReview),
    /// Allow a product to access another product account.
    AccountAccess(AccountAccessReview),
    /// Sign an RFC-0023 VRF transcript with a product account.
    SignVrf(SignVrfReview),
}

impl From<UserConfirmationReview> for NativeUserConfirmationReview {
    fn from(review: UserConfirmationReview) -> Self {
        match review {
            UserConfirmationReview::SignPayload(review) => Self::SignPayload(review.into()),
            UserConfirmationReview::SignRaw(review) => Self::SignRaw(review.into()),
            UserConfirmationReview::StatementStoreProductSign(review) => {
                Self::StatementStoreProductSign(review.into())
            }
            UserConfirmationReview::CreateTransaction(review) => {
                Self::CreateTransaction(review.into())
            }
            UserConfirmationReview::AccountAlias(review) => Self::AccountAlias(review.into()),
            UserConfirmationReview::CreateProof(review) => Self::CreateProof(review.into()),
            UserConfirmationReview::IdentityDisclosure(review) => {
                Self::IdentityDisclosure(review.into())
            }
            UserConfirmationReview::ResourceAllocation(review) => {
                Self::ResourceAllocation(review.into())
            }
            UserConfirmationReview::PreimageSubmit(review) => Self::PreimageSubmit(review.into()),
            UserConfirmationReview::AccountAccess(review) => Self::AccountAccess(review.into()),
            UserConfirmationReview::SignVrf(review) => Self::SignVrf(review.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign_payload_data() -> v01::HostSignPayloadData {
        v01::HostSignPayloadData {
            block_hash: vec![1; 32],
            block_number: vec![2],
            era: vec![3],
            genesis_hash: vec![4; 32],
            method: vec![5, 6],
            nonce: vec![7],
            spec_version: vec![8],
            tip: vec![9],
            transaction_version: vec![10],
            signed_extensions: vec!["CheckSpecVersion".to_string()],
            version: 4,
            asset_id: Some(vec![11]),
            metadata_hash: Some(vec![12; 32]),
            mode: Some(1),
            with_signed_transaction: Some(true),
        }
    }

    fn product_account() -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: "app.dot".to_string(),
            derivation_index: v01::DerivationIndex::Left(7),
        }
    }

    #[test]
    fn sign_payload_review_converts_with_full_fidelity() {
        let review = UserConfirmationReview::SignPayload(
            truapi_platform::SignPayloadReview::Product(v01::HostSignPayloadRequest {
                account: product_account(),
                payload: sign_payload_data(),
            }),
        );

        let NativeUserConfirmationReview::SignPayload(SignPayloadReview::Product {
            account,
            payload,
        }) = review.into()
        else {
            panic!("expected product sign-payload review");
        };
        assert_eq!(account.dot_ns_identifier, "app.dot");
        assert_eq!(account.derivation_index, DerivationIndex::Index(7));
        assert_eq!(payload, SignPayloadData::from(sign_payload_data()));
    }

    #[test]
    fn create_transaction_review_widens_fixed_arrays() {
        let review = UserConfirmationReview::CreateTransaction(
            truapi_platform::CreateTransactionReview::LegacyAccount(v01::LegacyAccountTxPayload {
                signer: [13; 32],
                genesis_hash: [14; 32],
                call_data: vec![15],
                extensions: vec![v01::TxPayloadExtension {
                    id: "CheckNonce".to_string(),
                    extra: vec![16],
                    additional_signed: vec![],
                }],
                tx_ext_version: 0,
            }),
        );

        let NativeUserConfirmationReview::CreateTransaction(
            CreateTransactionReview::LegacyAccount {
                signer,
                genesis_hash,
                call_data,
                extensions,
                tx_ext_version,
            },
        ) = review.into()
        else {
            panic!("expected legacy-account transaction review");
        };
        assert_eq!(signer, vec![13; 32]);
        assert_eq!(genesis_hash, vec![14; 32]);
        assert_eq!(call_data, vec![15]);
        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].id, "CheckNonce");
        assert_eq!(tx_ext_version, 0);
    }

    type ReviewCase = (
        UserConfirmationReview,
        fn(&NativeUserConfirmationReview) -> bool,
    );

    #[test]
    fn every_review_variant_maps_to_its_native_discriminant() {
        let cases: Vec<ReviewCase> = vec![
            (
                UserConfirmationReview::SignRaw(truapi_platform::SignRawReview::LegacyAccount(
                    v01::HostSignRawWithLegacyAccountRequest {
                        signer: "5F...".to_string(),
                        payload: v01::RawPayload::Payload {
                            payload: "hello".to_string(),
                        },
                    },
                )),
                |review| matches!(review, NativeUserConfirmationReview::SignRaw(_)),
            ),
            (
                UserConfirmationReview::StatementStoreProductSign(
                    truapi_platform::StatementStoreProductSignReview {
                        account: product_account(),
                        payload: vec![1, 2, 3],
                    },
                ),
                |review| {
                    matches!(
                        review,
                        NativeUserConfirmationReview::StatementStoreProductSign(_)
                    )
                },
            ),
            (
                UserConfirmationReview::AccountAlias(truapi_platform::AccountAliasReview {
                    calling_product_id: "app.dot".to_string(),
                    context: v01::ProductProofContext {
                        product_id: "app.dot".to_string(),
                        suffix: v01::DerivationIndex::Left(1),
                    },
                    ring_location: v01::RingLocation {
                        chain_id: [1; 32],
                        junctions: vec![
                            v01::RingLocationJunction::PalletInstance(2),
                            v01::RingLocationJunction::CollectionId(vec![3]),
                        ],
                    },
                }),
                |review| matches!(review, NativeUserConfirmationReview::AccountAlias(_)),
            ),
            (
                UserConfirmationReview::CreateProof(truapi_platform::CreateProofReview {
                    calling_product_id: "app.dot".to_string(),
                    context: v01::ProductProofContext {
                        product_id: "app.dot".to_string(),
                        suffix: v01::DerivationIndex::Right([2; 32]),
                    },
                    ring_location: v01::RingLocation {
                        chain_id: [1; 32],
                        junctions: vec![],
                    },
                    message: vec![9],
                }),
                |review| matches!(review, NativeUserConfirmationReview::CreateProof(_)),
            ),
            (
                UserConfirmationReview::IdentityDisclosure(
                    truapi_platform::IdentityDisclosureReview {
                        product_id: "app.dot".to_string(),
                    },
                ),
                |review| matches!(review, NativeUserConfirmationReview::IdentityDisclosure(_)),
            ),
            (
                UserConfirmationReview::ResourceAllocation(
                    truapi_platform::ResourceAllocationReview {
                        calling_product_id: "app.dot".to_string(),
                        resources: vec![
                            v01::AllocatableResource::StatementStoreAllowance,
                            v01::AllocatableResource::SmartContractAllowance(
                                v01::DerivationIndex::Left(4),
                            ),
                        ],
                    },
                ),
                |review| matches!(review, NativeUserConfirmationReview::ResourceAllocation(_)),
            ),
            (
                UserConfirmationReview::PreimageSubmit(truapi_platform::PreimageSubmitReview {
                    size: 42,
                }),
                |review| {
                    matches!(
                        review,
                        NativeUserConfirmationReview::PreimageSubmit(PreimageSubmitReview {
                            size: 42
                        })
                    )
                },
            ),
            (
                UserConfirmationReview::AccountAccess(truapi_platform::AccountAccessReview {
                    requesting_product_id: "a.dot".to_string(),
                    target_product_id: "b.dot".to_string(),
                }),
                |review| matches!(review, NativeUserConfirmationReview::AccountAccess(_)),
            ),
            (
                UserConfirmationReview::SignVrf(truapi_platform::SignVrfReview {
                    calling_product_id: "app.dot".to_string(),
                    request: v01::HostAccountSignVrfRequest {
                        account: product_account(),
                        transcript_label: b"vrf-label".to_vec(),
                        items: vec![v01::VrfTranscriptItem {
                            label: b"item".to_vec(),
                            value: vec![1, 2, 3],
                        }],
                    },
                }),
                |review| matches!(review, NativeUserConfirmationReview::SignVrf(_)),
            ),
        ];

        for (review, is_expected) in cases {
            let native = NativeUserConfirmationReview::from(review);
            assert!(is_expected(&native), "unexpected mapping: {native:?}");
        }
    }
}
