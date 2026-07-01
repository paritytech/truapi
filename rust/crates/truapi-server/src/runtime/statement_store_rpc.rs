//! Runtime helper for People-chain statement-store JSON-RPC.

use std::sync::Arc;

use serde_json::{Value, json};
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::{RpcSubscription, rpc_params};
use truapi_platform::{JsonRpcConnection, Platform};

use crate::host_logic::statement_store::{
    SUBMIT_STATEMENT_METHOD, SUBSCRIBE_STATEMENT_METHOD, TopicFilterKind,
    UNSUBSCRIBE_STATEMENT_METHOD, hex_topic,
};
use crate::host_rpc_client::HostRpcClient;
use crate::subscription::Spawner;

/// People-chain statement-store RPC client factory.
#[derive(Clone)]
pub(super) struct StatementStoreRpc {
    platform: Arc<dyn Platform>,
    people_chain_genesis_hash: [u8; 32],
    spawner: Spawner,
}

impl StatementStoreRpc {
    /// Build a helper backed by the platform-owned chain provider.
    pub(super) fn new(
        platform: Arc<dyn Platform>,
        people_chain_genesis_hash: [u8; 32],
        spawner: Spawner,
    ) -> Self {
        Self {
            platform,
            people_chain_genesis_hash,
            spawner,
        }
    }

    /// Open a statement-store RPC client over the host-provided People-chain
    /// connection.
    pub(super) async fn client(&self, label: &'static str) -> Result<RpcClient, String> {
        let connection = self.connect(label).await?;
        Ok(RpcClient::new(HostRpcClient::new(
            connection,
            self.spawner.clone(),
        )))
    }

    /// Submit a SCALE-encoded statement and wait for the JSON-RPC ack.
    pub(super) async fn submit(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        let rpc_client = self.client(label).await?;
        submit(&rpc_client, statement).await
    }

    /// Submit a SCALE-encoded statement without waiting for the JSON-RPC ack.
    pub(super) async fn submit_fire_and_forget(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        let connection = self.connect(label).await?;
        HostRpcClient::new(connection, self.spawner.clone())
            .send_fire_and_forget(SUBMIT_STATEMENT_METHOD, statement_submit_params(statement))
            .map_err(rpc_error_message)
    }

    async fn connect(&self, label: &'static str) -> Result<Arc<dyn JsonRpcConnection>, String> {
        self.platform
            .connect(self.people_chain_genesis_hash.to_vec())
            .await
            .map(Arc::from)
            .map_err(|err| format!("{label} connect failed: {err:?}"))
    }
}

/// Subscribe to statements matching the requested topic filter.
pub(super) async fn subscribe(
    rpc_client: &RpcClient,
    kind: TopicFilterKind,
    topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    rpc_client
        .subscribe::<Value>(
            SUBSCRIBE_STATEMENT_METHOD,
            rpc_params![filter(kind, topics)],
            UNSUBSCRIBE_STATEMENT_METHOD,
        )
        .await
}

/// Subscribe to statements matching every topic.
pub(super) async fn subscribe_match_all(
    rpc_client: &RpcClient,
    topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    subscribe(rpc_client, TopicFilterKind::MatchAll, topics).await
}

/// Submit a SCALE-encoded statement and wait for the JSON-RPC ack.
pub(super) async fn submit(rpc_client: &RpcClient, statement: Vec<u8>) -> Result<(), String> {
    rpc_client
        .request::<Value>(
            SUBMIT_STATEMENT_METHOD,
            rpc_params![statement_hex(&statement)],
        )
        .await
        .map(|_| ())
        .map_err(rpc_error_message)
}

/// Statement-store topic filter encoded as JSON-RPC params.
pub(super) fn filter(kind: TopicFilterKind, topics: &[[u8; 32]]) -> Value {
    let topics = topics.iter().map(hex_topic).collect::<Vec<_>>();
    match kind {
        TopicFilterKind::MatchAll => json!({ "matchAll": topics }),
        TopicFilterKind::MatchAny => json!({ "matchAny": topics }),
    }
}

/// Human-readable JSON-RPC error message, preserving user error text when
/// provided by the remote endpoint.
pub(super) fn rpc_error_message(error: subxt_rpcs::Error) -> String {
    match error {
        subxt_rpcs::Error::User(error) => error.message,
        other => other.to_string(),
    }
}

fn statement_submit_params(statement: Vec<u8>) -> Option<Box<serde_json::value::RawValue>> {
    rpc_params![statement_hex(&statement)].build()
}

fn statement_hex(statement: &[u8]) -> String {
    format!("0x{}", hex::encode(statement))
}
