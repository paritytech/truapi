use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeApi {
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeSpec {
    pub spec_name: String,
    pub impl_name: String,
    pub spec_version: u32,
    pub impl_version: u32,
    pub transaction_version: Option<u32>,
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
    pub key: Vec<u8>,
    pub query_type: StorageQueryType,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StorageResultItem {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
    pub hash: Option<Vec<u8>>,
    pub closest_descendant_merkle_value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum OperationStartedResult {
    Started { operation_id: String },
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
    pub genesis_hash: Vec<u8>,
    pub with_runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStorageRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub hash: Vec<u8>,
    pub items: Vec<StorageQueryItem>,
    pub child_trie: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadCallRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub hash: Vec<u8>,
    pub function: String,
    pub call_parameters: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadUnpinRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub hashes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadContinueRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStopOperationRequest {
    pub genesis_hash: Vec<u8>,
    pub follow_subscription_id: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastRequest {
    pub genesis_hash: Vec<u8>,
    pub transaction: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionStopRequest {
    pub genesis_hash: Vec<u8>,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderResponse {
    pub header: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyResponse {
    pub operation: OperationStartedResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStorageResponse {
    pub operation: OperationStartedResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadCallResponse {
    pub operation: OperationStartedResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashRequest {
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashResponse {
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameRequest {
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameResponse {
    pub chain_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesRequest {
    pub genesis_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesResponse {
    pub properties: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastResponse {
    pub operation_id: Option<String>,
}
