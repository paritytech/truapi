use parity_scale_codec::{Decode, Encode};

use super::ProductAccountId;

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

/// Parameters for [`crate::api::ChainInteraction::remote_chain_head_follow_subscribe`].
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

/// Request to send a JSON-RPC message to a chain identified by its genesis hash.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSendRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// JSON-RPC message body.
    pub message: String,
}

/// Request to subscribe to inbound JSON-RPC messages for a chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeRequest {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
}

/// An inbound JSON-RPC message from the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJsonrpcMessageSubscribeItem {
    /// JSON-RPC message body.
    pub message: String,
}

/// Full Substrate extrinsic signing payload with all fields needed for signature
/// generation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadRequest {
    /// Product account that will sign this payload.
    pub account: ProductAccountId,
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

/// Raw data to sign -- either binary bytes or a string message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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

/// A raw signing request pairing an account with the payload to sign.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignRawRequest {
    /// Product account that will sign this payload.
    pub account: ProductAccountId,
    /// The payload to sign.
    pub payload: RawPayload,
}

/// Result of a signing operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadResponse {
    /// The cryptographic signature.
    pub signature: Vec<u8>,
    /// Full signed transaction, if requested.
    pub signed_transaction: Option<Vec<u8>>,
}

/// Signing operation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostSignPayloadError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected signing.
    Rejected,
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Sign raw bytes with a non-product (legacy) account. The signer field
/// identifies which legacy account to use.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignRawWithLegacyAccountRequest {
    /// Signer address (SS58 or hex) of the legacy account.
    pub signer: String,
    /// The data to sign.
    pub payload: RawPayload,
}

/// Sign a Substrate extrinsic payload with a non-product (legacy) account.
/// Contains the same fields as [`HostSignPayloadRequest`] minus `address`
/// (replaced by `signer`).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadWithLegacyAccountRequest {
    /// Signer address (SS58 or hex) of the legacy account.
    pub signer: String,
    /// The extrinsic payload to sign.
    pub payload: HostSignPayloadRequest,
}

/// A signed extension for a transaction payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadExtensionV1 {
    /// Extension name (e.g., `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Vec<u8>,
}

/// Context information for transaction construction.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadContextV1 {
    /// `RuntimeMetadataPrefixed` blob (SCALE).
    pub metadata: Vec<u8>,
    /// Native token symbol.
    pub token_symbol: String,
    /// Native token decimals.
    pub token_decimals: u32,
    /// Highest known block number.
    pub best_block_height: u32,
}

/// Version 1 transaction payload with all data needed to construct a signed
/// extrinsic.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadV1 {
    /// Signer hint (address/name), `None` = host picks.
    pub signer: Option<String>,
    /// SCALE-encoded Call data.
    pub call_data: Vec<u8>,
    /// Signed extensions.
    pub extensions: Vec<TxPayloadExtensionV1>,
    /// 0 for Extrinsic V4, any for V5.
    pub tx_ext_version: u8,
    /// Transaction context.
    pub context: TxPayloadContextV1,
}

/// Versioned transaction payload envelope.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum VersionedTxPayload {
    /// Version 1 payload.
    V1(TxPayloadV1),
}

/// Request to create a transaction for a product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionRequest {
    /// Product account that will sign the transaction.
    pub product_account_id: ProductAccountId,
    /// Versioned transaction payload.
    pub payload: VersionedTxPayload,
}

/// Transaction creation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCreateTransactionError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected.
    Rejected,
    /// Unsupported payload version or extension.
    NotSupported {
        /// Unsupported payload or extension reason.
        reason: String,
    },
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Response containing a created transaction.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionResponse {
    /// SCALE-encoded signed transaction.
    pub transaction: Vec<u8>,
}

/// Request to create a transaction with a non-product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionWithLegacyAccountRequest {
    /// Versioned transaction payload to sign.
    pub payload: VersionedTxPayload,
}

/// Response containing a transaction created with a non-product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionWithLegacyAccountResponse {
    /// SCALE-encoded signed transaction.
    pub transaction: Vec<u8>,
}
