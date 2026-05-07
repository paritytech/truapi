use parity_scale_codec::{Decode, Encode};

/// A runtime API identified by name and version.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeApi {
    /// Runtime API name.
    pub name: String,
    /// Runtime API version.
    pub version: u32,
}

/// Runtime specification metadata.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeType {
    /// Valid runtime with spec.
    Valid(RuntimeSpec),
    /// Invalid runtime with error.
    Invalid { error: String },
}

/// Type of storage query to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum StorageQueryType {
    Value,
    Hash,
    ClosestDescendantMerkleValue,
    DescendantsValues,
    DescendantsHashes,
}

/// A single storage query.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StorageQueryItem {
    /// Storage key to query.
    pub key: Vec<u8>,
    /// What to return.
    pub query_type: StorageQueryType,
}

/// Result of a storage query.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StorageResultItem {
    /// The queried key.
    pub key: Vec<u8>,
    /// Value, if requested.
    pub value: Option<Vec<u8>>,
    /// Hash, if requested.
    pub hash: Option<Vec<u8>>,
    /// Merkle value, if requested.
    pub closest_descendant_merkle_value: Option<Vec<u8>>,
}

/// Result of starting a chain operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum OperationStartedResult {
    /// Operation started successfully.
    Started {
        /// The assigned operation identifier.
        operation_id: String,
    },
    /// Too many concurrent operations.
    LimitReached,
}

/// Events received when following the chain head.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteChainHeadFollowItem {
    /// Initial state with finalized blocks.
    Initialized {
        finalized_block_hashes: Vec<Vec<u8>>,
        finalized_block_runtime: Option<RuntimeType>,
    },
    /// A new block was produced.
    NewBlock {
        block_hash: Vec<u8>,
        parent_block_hash: Vec<u8>,
        new_runtime: Option<RuntimeType>,
    },
    /// Best block changed.
    BestBlockChanged { best_block_hash: Vec<u8> },
    /// Blocks were finalized.
    Finalized {
        finalized_block_hashes: Vec<Vec<u8>>,
        pruned_block_hashes: Vec<Vec<u8>>,
    },
    /// Body fetch completed.
    OperationBodyDone {
        operation_id: String,
        value: Vec<Vec<u8>>,
    },
    /// Runtime call completed.
    OperationCallDone {
        operation_id: String,
        output: Vec<u8>,
    },
    /// Storage results batch.
    OperationStorageItems {
        operation_id: String,
        items: Vec<StorageResultItem>,
    },
    /// Storage query completed.
    OperationStorageDone { operation_id: String },
    /// Operation paused, needs [`crate::api::ChainInteraction::remote_chain_head_continue`].
    OperationWaitingForContinue { operation_id: String },
    /// Block became inaccessible.
    OperationInaccessible { operation_id: String },
    /// Operation failed.
    OperationError { operation_id: String, error: String },
    /// Subscription terminated by server.
    Stop,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_follow`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadFollowRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Whether to include runtime information in events.
    pub with_runtime: bool,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_header`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_body`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_storage`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStorageRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
    /// Storage items to query.
    pub items: Vec<StorageQueryItem>,
    /// Optional child trie.
    pub child_trie: Option<Vec<u8>>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_call`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadCallRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
    /// Runtime API function name.
    pub function: String,
    /// SCALE-encoded call parameters.
    pub call_parameters: Vec<u8>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_unpin`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadUnpinRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hashes to unpin.
    pub hashes: Vec<Vec<u8>>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_continue`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadContinueRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: String,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_stop_operation`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStopOperationRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: String,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_transaction_broadcast`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Signed transaction bytes.
    pub transaction: Vec<u8>,
}

/// Parameters for [`crate::api::ChainInteraction::remote_chain_transaction_stop`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionStopRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Operation identifier of the broadcast to stop.
    pub operation_id: String,
}

/// Response containing a block header, if available.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderResponse {
    /// SCALE-encoded block header.
    pub header: Option<Vec<u8>>,
}

/// Response indicating a block body operation was started.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

/// Response indicating a storage query operation was started.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStorageResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

/// Response indicating a runtime call operation was started.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadCallResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

/// Request to fetch a chain genesis hash.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashRequest {
    /// Chain genesis hash requested by the product.
    pub genesis_hash: Vec<u8>,
}

/// Response containing a chain genesis hash.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashResponse {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// Request to fetch a chain display name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// Response containing a chain display name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameResponse {
    /// Chain display name.
    pub chain_name: String,
}

/// Request to fetch chain properties.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// Response containing JSON-encoded chain properties.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesResponse {
    /// JSON-encoded properties.
    pub properties: String,
}

/// Response containing a transaction broadcast operation identifier.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastResponse {
    /// Broadcast operation identifier, if available.
    pub operation_id: Option<String>,
}
