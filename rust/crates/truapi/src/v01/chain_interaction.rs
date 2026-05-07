use parity_scale_codec::{Decode, Encode};

use super::{GenesisHash, Hex};

/// Block hash identifier.
pub type BlockHash = Hex;

/// Operation identifier for async chain operations.
pub type OperationId = String;

/// A runtime API identified by name and version.
pub type RuntimeApi = (String, u32);

/// Runtime specification metadata.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct RuntimeSpec {
    /// Specification name.
    pub spec_name: String,
    /// Implementation name.
    pub impl_name: String,
    /// Spec version number.
    pub spec_version: u32,
    /// Implementation version.
    pub impl_version: u32,
    /// Transaction format version.
    pub transaction_version: Option<u32>,
    /// Supported runtime APIs.
    pub apis: Vec<RuntimeApi>,
}

/// Runtime validity check result.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RuntimeType {
    /// Valid runtime with spec.
    Valid(RuntimeSpec),
    /// Invalid runtime with error.
    Invalid { error: String },
}

/// Type of storage query to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum StorageQueryType {
    Value,
    Hash,
    ClosestDescendantMerkleValue,
    DescendantsValues,
    DescendantsHashes,
}

/// A single storage query.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct StorageQueryItem {
    /// Storage key to query.
    pub key: Hex,
    /// What to return.
    pub query_type: StorageQueryType,
}

/// Result of a storage query.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct StorageResultItem {
    /// The queried key.
    pub key: Hex,
    /// Value, if requested.
    pub value: Option<Hex>,
    /// Hash, if requested.
    pub hash: Option<Hex>,
    /// Merkle value, if requested.
    pub closest_descendant_merkle_value: Option<Hex>,
}

/// Result of starting a chain operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum OperationStartedResult {
    /// Operation started successfully.
    Started {
        /// The assigned operation identifier.
        operation_id: OperationId,
    },
    /// Too many concurrent operations.
    LimitReached,
}

/// Events received when following the chain head.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChainHeadEvent {
    /// Initial state with finalized blocks.
    Initialized {
        finalized_block_hashes: Vec<BlockHash>,
        finalized_block_runtime: Option<RuntimeType>,
    },
    /// A new block was produced.
    NewBlock {
        block_hash: BlockHash,
        parent_block_hash: BlockHash,
        new_runtime: Option<RuntimeType>,
    },
    /// Best block changed.
    BestBlockChanged { best_block_hash: BlockHash },
    /// Blocks were finalized.
    Finalized {
        finalized_block_hashes: Vec<BlockHash>,
        pruned_block_hashes: Vec<BlockHash>,
    },
    /// Body fetch completed.
    OperationBodyDone {
        operation_id: OperationId,
        value: Vec<Hex>,
    },
    /// Runtime call completed.
    OperationCallDone {
        operation_id: OperationId,
        output: Hex,
    },
    /// Storage results batch.
    OperationStorageItems {
        operation_id: OperationId,
        items: Vec<StorageResultItem>,
    },
    /// Storage query completed.
    OperationStorageDone { operation_id: OperationId },
    /// Operation paused, needs [`crate::api::ChainInteraction::remote_chain_head_continue`].
    OperationWaitingForContinue { operation_id: OperationId },
    /// Block became inaccessible.
    OperationInaccessible { operation_id: OperationId },
    /// Operation failed.
    OperationError {
        operation_id: OperationId,
        error: String,
    },
    /// Subscription terminated by server.
    Stop,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_follow`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainHeadFollowRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Whether to include runtime information in events.
    pub with_runtime: bool,
}

/// Parameters for chain head methods that operate within a follow subscription
/// on a specific block.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainHeadBlockRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: BlockHash,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_storage`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainHeadStorageRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: BlockHash,
    /// Storage items to query.
    pub items: Vec<StorageQueryItem>,
    /// Optional child trie.
    pub child_trie: Option<Hex>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_call`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainHeadCallRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: BlockHash,
    /// Runtime API function name.
    pub function: String,
    /// SCALE-encoded call parameters.
    pub call_parameters: Hex,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_unpin`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainHeadUnpinRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hashes to unpin.
    pub hashes: Vec<BlockHash>,
}

/// Parameters for chain head operations that reference a specific operation within
/// a follow subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainHeadOperationRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: OperationId,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_transaction_broadcast`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainTransactionBroadcastRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Signed transaction bytes.
    pub transaction: Hex,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_transaction_stop`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ChainTransactionStopRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Operation identifier of the broadcast to stop.
    pub operation_id: OperationId,
}

pub type RemoteChainHeadFollowRequest = ChainHeadFollowRequest;
pub type RemoteChainHeadFollowItem = ChainHeadEvent;
pub type RemoteChainHeadHeaderRequest = ChainHeadBlockRequest;
pub type RemoteChainHeadHeaderResponse = Option<Hex>;
pub type RemoteChainHeadHeaderError = super::GenericError;
pub type RemoteChainHeadBodyRequest = ChainHeadBlockRequest;
pub type RemoteChainHeadBodyResponse = OperationStartedResult;
pub type RemoteChainHeadBodyError = super::GenericError;
pub type RemoteChainHeadStorageRequest = ChainHeadStorageRequest;
pub type RemoteChainHeadStorageResponse = OperationStartedResult;
pub type RemoteChainHeadStorageError = super::GenericError;
pub type RemoteChainHeadCallRequest = ChainHeadCallRequest;
pub type RemoteChainHeadCallResponse = OperationStartedResult;
pub type RemoteChainHeadCallError = super::GenericError;
pub type RemoteChainHeadUnpinRequest = ChainHeadUnpinRequest;
pub type RemoteChainHeadUnpinError = super::GenericError;
pub type RemoteChainHeadContinueRequest = ChainHeadOperationRequest;
pub type RemoteChainHeadContinueError = super::GenericError;
pub type RemoteChainHeadStopOperationRequest = ChainHeadOperationRequest;
pub type RemoteChainHeadStopOperationError = super::GenericError;
pub type RemoteChainSpecGenesisHashRequest = GenesisHash;
pub type RemoteChainSpecGenesisHashResponse = Hex;
pub type RemoteChainSpecGenesisHashError = super::GenericError;
pub type RemoteChainSpecChainNameRequest = GenesisHash;
pub type RemoteChainSpecChainNameResponse = String;
pub type RemoteChainSpecChainNameError = super::GenericError;
pub type RemoteChainSpecPropertiesRequest = GenesisHash;
pub type RemoteChainSpecPropertiesResponse = String;
pub type RemoteChainSpecPropertiesError = super::GenericError;
pub type RemoteChainTransactionBroadcastRequest = ChainTransactionBroadcastRequest;
pub type RemoteChainTransactionBroadcastResponse = Option<String>;
pub type RemoteChainTransactionBroadcastError = super::GenericError;
pub type RemoteChainTransactionStopRequest = ChainTransactionStopRequest;
pub type RemoteChainTransactionStopError = super::GenericError;
