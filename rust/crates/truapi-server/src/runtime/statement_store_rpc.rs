//! Runtime helper for People-chain statement-store JSON-RPC.
//!
//! Statement traffic opens short-lived host RPC connections for its own
//! subscription lifetimes instead of riding the shared
//! [`crate::chain_runtime::ChainRuntime`] chainHead runtime. If a host shares
//! same-topic statement subscriptions on one upstream connection, the host
//! broker must fan out and ref-count same-upstream-token notifications.

use std::sync::Arc;

use core::time::Duration;
use serde_json::{Value, json};

use subxt_rpcs::RpcClient;
use subxt_rpcs::client::{RpcSubscription, rpc_params};
use tracing::warn;
use truapi_platform::{JsonRpcConnection, Platform};

use crate::host_logic::statement_store::{
    SUBMIT_STATEMENT_METHOD, SUBSCRIBE_STATEMENT_METHOD, TopicFilterKind,
    UNSUBSCRIBE_STATEMENT_METHOD, hex_topic,
};
use crate::host_rpc_client::HostRpcClient;
use crate::subscription::Spawner;

const SSO_NO_ALLOWANCE_RETRY_ATTEMPTS: usize = 5;
const SSO_NO_ALLOWANCE_RETRY_DELAY: Duration = Duration::from_secs(1);

/// People-chain statement-store RPC client factory.
#[derive(Clone)]
pub(crate) struct StatementStoreRpc {
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

    /// Submit an SSO statement, tolerating the short propagation window after
    /// an allowance registration is included but not yet visible to the
    /// Statement Store RPC backend.
    pub(super) async fn submit_sso(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        let rpc_client = self.client(label).await?;
        submit_sso(&rpc_client, statement, label).await
    }

    /// Submit a SCALE-encoded statement without waiting for the JSON-RPC ack.
    pub(super) async fn submit_fire_and_forget(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        let connection = self.connect(label).await?;
        HostRpcClient::new(connection, self.spawner.clone())
            .send_fire_and_forget(
                SUBMIT_STATEMENT_METHOD,
                rpc_params![format!("0x{}", hex::encode(&statement))].build(),
            )
            .map_err(rpc_error_message)
    }

    async fn connect(&self, label: &'static str) -> Result<Arc<dyn JsonRpcConnection>, String> {
        self.platform
            .connect(self.people_chain_genesis_hash)
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

/// Submit a SCALE-encoded statement and confirm the store accepted it.
///
/// `statement_submit` returns an RPC error only for internal failures; a
/// rejected or invalid statement (e.g. `NoAllowance`, `BadProof`) comes back as
/// `Ok(SubmitResult)`. Treat only `new`/`known` as success, so allowance/proof
/// rejections surface instead of being silently dropped.
pub(super) async fn submit(rpc_client: &RpcClient, statement: Vec<u8>) -> Result<(), String> {
    let result = rpc_client
        .request::<Value>(
            SUBMIT_STATEMENT_METHOD,
            rpc_params![format!("0x{}", hex::encode(&statement))],
        )
        .await
        .map_err(rpc_error_message)?;
    match result.get("status").and_then(Value::as_str) {
        Some("new") | Some("known") => Ok(()),
        _ => Err(format!("statement_submit not accepted: {result}")),
    }
}

pub(super) async fn submit_sso(
    rpc_client: &RpcClient,
    statement: Vec<u8>,
    label: &'static str,
) -> Result<(), String> {
    for attempt in 1..=SSO_NO_ALLOWANCE_RETRY_ATTEMPTS {
        match submit(rpc_client, statement.clone()).await {
            Ok(()) => return Ok(()),
            Err(reason)
                if is_transient_no_allowance(&reason)
                    && attempt < SSO_NO_ALLOWANCE_RETRY_ATTEMPTS =>
            {
                warn!(
                    label,
                    attempt,
                    max_attempts = SSO_NO_ALLOWANCE_RETRY_ATTEMPTS,
                    "SSO allowance not visible yet; retrying statement submission"
                );
                futures_timer::Delay::new(SSO_NO_ALLOWANCE_RETRY_DELAY).await;
            }
            Err(reason) => return Err(reason),
        }
    }
    unreachable!("the bounded SSO submit loop always returns")
}

fn is_transient_no_allowance(reason: &str) -> bool {
    reason.contains("noAllowance")
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

#[cfg(test)]
mod tests {
    use super::is_transient_no_allowance;

    #[test]
    fn identifies_no_allowance_submit_rejections_for_retry() {
        assert!(is_transient_no_allowance(
            r#"statement_submit not accepted: {"reason":"noAllowance","status":"rejected"}"#
        ));
        assert!(!is_transient_no_allowance(
            r#"statement_submit not accepted: {"reason":"badProof","status":"rejected"}"#
        ));
    }
}
