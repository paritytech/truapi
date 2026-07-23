use parity_scale_codec::{Decode, Encode};

/// One entry of a runtime's supported API list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeApi {
    /// Runtime API name.
    pub name: String,
    /// Runtime API version.
    pub version: u32,
}

/// Runtime version information for a block's runtime.
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

/// Runtime attached to follow events, either a decoded spec or a decode error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeType {
    /// Runtime spec decoded successfully.
    Valid(RuntimeSpec),
    /// The runtime could not be decoded.
    Invalid {
        /// Decode error message.
        error: String,
    },
}

/// What a chain-head storage query returns for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum StorageQueryType {
    /// Return the value under the key.
    Value,
    /// Return the hash of the value under the key.
    Hash,
    /// Return the Merkle value of the closest descendant of the key.
    ClosestDescendantMerkleValue,
    /// Return the values of the key and all its descendants.
    DescendantsValues,
    /// Return the value hashes of the key and all its descendants.
    DescendantsHashes,
}

/// A single key query within a chain-head storage request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StorageQueryItem {
    /// Storage key to query.
    pub key: Vec<u8>,
    /// What to return.
    pub query_type: StorageQueryType,
}

/// Result for one queried storage key.
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

/// Outcome of starting a chain-head operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum OperationStartedResult {
    /// The operation was accepted; results arrive as follow events.
    Started {
        /// The assigned operation identifier.
        operation_id: String,
    },
    /// Too many operations are in progress; retry after some complete.
    LimitReached,
}

/// Event emitted on a chain-head follow subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteChainHeadFollowItem {
    /// First event of the subscription, describing the current finalized blocks.
    Initialized {
        /// Hashes of the finalized blocks, from lowest to highest height.
        finalized_block_hashes: Vec<Vec<u8>>,
        /// Runtime of the latest finalized block, if requested.
        finalized_block_runtime: Option<RuntimeType>,
    },
    /// A new non-finalized block was announced.
    NewBlock {
        /// Hash of the new block.
        block_hash: Vec<u8>,
        /// Hash of the parent block.
        parent_block_hash: Vec<u8>,
        /// Runtime of the block if it differs from its parent, when requested.
        new_runtime: Option<RuntimeType>,
    },
    /// The best block has changed.
    BestBlockChanged {
        /// Hash of the new best block.
        best_block_hash: Vec<u8>,
    },
    /// One or more blocks were finalized.
    Finalized {
        /// Newly finalized block hashes, from lowest to highest height.
        finalized_block_hashes: Vec<Vec<u8>>,
        /// Hashes of blocks pruned off the finalized chain.
        pruned_block_hashes: Vec<Vec<u8>>,
    },
    /// A body operation completed.
    OperationBodyDone {
        /// Operation identifier.
        operation_id: String,
        /// SCALE-encoded extrinsics of the block body.
        value: Vec<Vec<u8>>,
    },
    /// A runtime call operation completed.
    OperationCallDone {
        /// Operation identifier.
        operation_id: String,
        /// SCALE-encoded return value of the runtime call.
        output: Vec<u8>,
    },
    /// A storage operation produced a batch of results.
    OperationStorageItems {
        /// Operation identifier.
        operation_id: String,
        /// Storage results in this batch.
        items: Vec<StorageResultItem>,
    },
    /// A storage operation finished emitting results.
    OperationStorageDone {
        /// Operation identifier.
        operation_id: String,
    },
    /// A storage operation is paused until the product requests continuation.
    OperationWaitingForContinue {
        /// Operation identifier.
        operation_id: String,
    },
    /// The operation failed because the required data was not accessible; it can be retried.
    OperationInaccessible {
        /// Operation identifier.
        operation_id: String,
    },
    /// The operation failed with an error.
    OperationError {
        /// Operation identifier.
        operation_id: String,
        /// Human-readable error message.
        error: String,
    },
    /// The subscription was stopped by the host and is no longer valid.
    Stop,
}

/// Request to start a chain-head follow subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadFollowRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Whether to include runtime information in events.
    pub with_runtime: bool,
}

/// Request to fetch the header of a pinned block.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
}

/// Request to fetch the body of a pinned block.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: Vec<u8>,
}

/// Request to query storage at a pinned block.
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

/// Request to invoke a runtime call at a pinned block.
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

/// Request to release pinned blocks.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadUnpinRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hashes to unpin.
    pub hashes: Vec<Vec<u8>>,
}

/// Request to continue a paused chain-head operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadContinueRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: String,
}

/// Request to stop an in-progress chain-head operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStopOperationRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: String,
}

/// Request to broadcast a signed transaction.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Signed transaction bytes.
    pub transaction: Vec<u8>,
}

/// Request to stop broadcasting a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionStopRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Operation identifier of the broadcast to stop.
    pub operation_id: String,
}

/// Response containing the requested block header.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadHeaderResponse {
    /// SCALE-encoded block header.
    pub header: Option<Vec<u8>>,
}

/// Response to a body request; results arrive as follow events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadBodyResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

/// Response to a storage request; results arrive as follow events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadStorageResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

/// Response to a runtime call request; the output arrives as a follow event.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainHeadCallResponse {
    /// Started operation result.
    pub operation: OperationStartedResult,
}

/// Request for the canonical genesis hash of a chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashRequest {
    /// Chain genesis hash requested by the product.
    pub genesis_hash: Vec<u8>,
}

/// Response containing the canonical genesis hash.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecGenesisHashResponse {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// Request for the display name of a chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// Response containing the chain display name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecChainNameResponse {
    /// Chain display name.
    pub chain_name: String,
}

/// Request for the JSON-encoded properties of a chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// Response containing the chain properties.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainSpecPropertiesResponse {
    /// JSON-encoded properties.
    pub properties: String,
}

/// Response to a transaction broadcast request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChainTransactionBroadcastResponse {
    /// Broadcast operation identifier, if available.
    pub operation_id: Option<String>,
}
