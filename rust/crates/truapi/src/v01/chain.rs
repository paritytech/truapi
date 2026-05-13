use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeApi {
    /// Runtime API name.
    pub name: String,
    /// Runtime API version.
    pub version: u32,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeType {
    Valid(RuntimeSpec),
    Invalid { error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum StorageQueryType {
    Value,
    Hash,
    ClosestDescendantMerkleValue,
    DescendantsValues,
    DescendantsHashes,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StorageQueryItem {
    /// Storage key to query.
    pub key: Vec<u8>,
    /// What to return.
    pub query_type: StorageQueryType,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum OperationStartedResult {
    Started {
        /// The assigned operation identifier.
        operation_id: String,
    },
    LimitReached,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteChainHeadFollowItem {
    Initialized {
        finalized_block_hashes: Vec<Vec<u8>>,
        finalized_block_runtime: Option<RuntimeType>,
    },
    NewBlock {
        block_hash: Vec<u8>,
        parent_block_hash: Vec<u8>,
        new_runtime: Option<RuntimeType>,
    },
    BestBlockChanged {
        best_block_hash: Vec<u8>,
    },
    Finalized {
        finalized_block_hashes: Vec<Vec<u8>>,
        pruned_block_hashes: Vec<Vec<u8>>,
    },
    OperationBodyDone {
        operation_id: String,
        value: Vec<Vec<u8>>,
    },
    OperationCallDone {
        operation_id: String,
        output: Vec<u8>,
    },
    OperationStorageItems {
        operation_id: String,
        items: Vec<StorageResultItem>,
    },
    OperationStorageDone {
        operation_id: String,
    },
    OperationWaitingForContinue {
        operation_id: String,
    },
    OperationInaccessible {
        operation_id: String,
    },
    OperationError {
        operation_id: String,
        error: String,
    },
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadFollowRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Whether to include runtime information in events.
    pub with_runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadUnpinRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hashes to unpin.
    pub hashes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadContinueRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStopOperationRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Signed transaction bytes.
    pub transaction: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionStopRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Operation identifier of the broadcast to stop.
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderResponse {
    /// SCALE-encoded block header.
    pub header: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStorageResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadCallResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashRequest {
    /// Chain genesis hash requested by the product.
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashResponse {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameResponse {
    /// Chain display name.
    pub chain_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesResponse {
    /// JSON-encoded properties.
    pub properties: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastResponse {
    /// Broadcast operation identifier, if available.
    pub operation_id: Option<String>,
}
