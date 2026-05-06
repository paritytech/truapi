//! Unified [`ChainInteraction`] trait.

use crate::v01::OperationStartedResult;
use crate::versioned::chain::{
    RemoteChainHeadBodyError, RemoteChainHeadBodyRequest, RemoteChainHeadBodyResponse,
    RemoteChainHeadCallError, RemoteChainHeadCallRequest, RemoteChainHeadCallResponse,
    RemoteChainHeadContinueError, RemoteChainHeadContinueRequest, RemoteChainHeadContinueResponse,
    RemoteChainHeadFollowItem, RemoteChainHeadFollowRequest, RemoteChainHeadHeaderError,
    RemoteChainHeadHeaderRequest, RemoteChainHeadHeaderResponse, RemoteChainHeadStopOperationError,
    RemoteChainHeadStopOperationRequest, RemoteChainHeadStopOperationResponse,
    RemoteChainHeadStorageError, RemoteChainHeadStorageRequest, RemoteChainHeadStorageResponse,
    RemoteChainHeadUnpinError, RemoteChainHeadUnpinRequest, RemoteChainHeadUnpinResponse,
    RemoteChainSpecChainNameError, RemoteChainSpecChainNameRequest,
    RemoteChainSpecChainNameResponse, RemoteChainSpecGenesisHashError,
    RemoteChainSpecGenesisHashRequest, RemoteChainSpecGenesisHashResponse,
    RemoteChainSpecPropertiesError, RemoteChainSpecPropertiesRequest,
    RemoteChainSpecPropertiesResponse, RemoteChainTransactionBroadcastError,
    RemoteChainTransactionBroadcastRequest, RemoteChainTransactionBroadcastResponse,
    RemoteChainTransactionStopError, RemoteChainTransactionStopRequest,
    RemoteChainTransactionStopResponse,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Chain head and transaction interactions.
///
/// Every method has a default body that flags the call as unavailable through
/// [`CallContext::fail_unavailable`] and returns a placeholder value. Hosts
/// override only the methods they can actually service against a chain
/// provider.
#[async_trait::async_trait]
pub trait ChainInteraction: Send + Sync {
    /// Follow the chain head and receive block events.
    #[wire(id = 76)]
    async fn remote_chain_head_follow(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadFollowRequest,
    ) -> Subscription<RemoteChainHeadFollowItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }

    /// Fetch a block header.
    #[wire(id = 80)]
    async fn remote_chain_head_header(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, RemoteChainHeadHeaderError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadHeaderResponse::V1(None))
    }

    /// Fetch a block body.
    #[wire(id = 82)]
    async fn remote_chain_head_body(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, RemoteChainHeadBodyError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadBodyResponse::V1(
            OperationStartedResult::LimitReached,
        ))
    }

    /// Query runtime storage at a specific block.
    #[wire(id = 84)]
    async fn remote_chain_head_storage(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, RemoteChainHeadStorageError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadStorageResponse::V1(
            OperationStartedResult::LimitReached,
        ))
    }

    /// Invoke a runtime call at a specific block.
    #[wire(id = 86)]
    async fn remote_chain_head_call(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, RemoteChainHeadCallError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadCallResponse::V1(
            OperationStartedResult::LimitReached,
        ))
    }

    /// Release pinned blocks.
    #[wire(id = 88)]
    async fn remote_chain_head_unpin(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadUnpinRequest,
    ) -> Result<RemoteChainHeadUnpinResponse, RemoteChainHeadUnpinError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadUnpinResponse::V1)
    }

    /// Continue a paused chain-head operation.
    #[wire(id = 90)]
    async fn remote_chain_head_continue(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadContinueRequest,
    ) -> Result<RemoteChainHeadContinueResponse, RemoteChainHeadContinueError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadContinueResponse::V1)
    }

    /// Stop a chain-head operation.
    #[wire(id = 92)]
    async fn remote_chain_head_stop_operation(
        &self,
        cx: &CallContext,
        _request: RemoteChainHeadStopOperationRequest,
    ) -> Result<RemoteChainHeadStopOperationResponse, RemoteChainHeadStopOperationError> {
        cx.fail_unavailable();
        Ok(RemoteChainHeadStopOperationResponse::V1)
    }

    /// Fetch the canonical genesis hash for a chain.
    #[wire(id = 94)]
    async fn remote_chain_spec_genesis_hash(
        &self,
        cx: &CallContext,
        _request: RemoteChainSpecGenesisHashRequest,
    ) -> Result<RemoteChainSpecGenesisHashResponse, RemoteChainSpecGenesisHashError> {
        cx.fail_unavailable();
        Ok(RemoteChainSpecGenesisHashResponse::V1(Vec::new()))
    }

    /// Fetch the display name of a chain.
    #[wire(id = 96)]
    async fn remote_chain_spec_chain_name(
        &self,
        cx: &CallContext,
        _request: RemoteChainSpecChainNameRequest,
    ) -> Result<RemoteChainSpecChainNameResponse, RemoteChainSpecChainNameError> {
        cx.fail_unavailable();
        Ok(RemoteChainSpecChainNameResponse::V1(String::new()))
    }

    /// Fetch the JSON-encoded properties of a chain.
    #[wire(id = 98)]
    async fn remote_chain_spec_properties(
        &self,
        cx: &CallContext,
        _request: RemoteChainSpecPropertiesRequest,
    ) -> Result<RemoteChainSpecPropertiesResponse, RemoteChainSpecPropertiesError> {
        cx.fail_unavailable();
        Ok(RemoteChainSpecPropertiesResponse::V1(String::new()))
    }

    /// Broadcast a signed transaction.
    #[wire(id = 100)]
    async fn remote_chain_transaction_broadcast(
        &self,
        cx: &CallContext,
        _request: RemoteChainTransactionBroadcastRequest,
    ) -> Result<RemoteChainTransactionBroadcastResponse, RemoteChainTransactionBroadcastError> {
        cx.fail_unavailable();
        Ok(RemoteChainTransactionBroadcastResponse::V1(None))
    }

    /// Stop a transaction broadcast.
    #[wire(id = 102)]
    async fn remote_chain_transaction_stop(
        &self,
        cx: &CallContext,
        _request: RemoteChainTransactionStopRequest,
    ) -> Result<RemoteChainTransactionStopResponse, RemoteChainTransactionStopError> {
        cx.fail_unavailable();
        Ok(RemoteChainTransactionStopResponse::V1)
    }
}
