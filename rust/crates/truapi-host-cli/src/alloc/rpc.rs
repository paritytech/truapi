//! JSON-RPC helpers for statement-store allowance registration.
//!
//! Keep the CLI diagnostic path on the same `subxt-rpcs` transport surface as
//! the runtime code instead of hand-rolling request ids, subscriptions, and
//! websocket framing here.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use subxt_rpcs::client::{RpcClient as SubxtRpcClient, RpcParams, rpc_params};
use tokio::time::timeout;

/// Timeout for an allowance registration extrinsic to finalize.
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(120);

/// A People-chain JSON-RPC connection.
pub struct RpcClient {
    inner: SubxtRpcClient,
}

impl RpcClient {
    /// Open a JSON-RPC connection to `url`.
    pub async fn connect(url: &str) -> Result<Self> {
        let inner = SubxtRpcClient::from_insecure_url(url)
            .await
            .with_context(|| format!("connect {url}"))?;
        Ok(Self { inner })
    }

    /// Call `method` with `params`, returning the result value.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.inner
            .request(method, value_to_params(params)?)
            .await
            .with_context(|| format!("rpc {method}"))
    }

    /// `state_getStorage(key)` -> raw value bytes, or `None` if absent.
    pub async fn get_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let key_hex = format!("0x{}", hex::encode(key));
        match self
            .inner
            .request::<Value>("state_getStorage", rpc_params![key_hex])
            .await
            .context("rpc state_getStorage")?
        {
            Value::String(hex_value) => Ok(Some(decode_hex(&hex_value)?)),
            _ => Ok(None),
        }
    }

    /// Submit an extrinsic and wait for `finalized`; returns the block hash.
    pub async fn submit_and_watch(&self, extrinsic: &[u8]) -> Result<String> {
        let extrinsic_hex = format!("0x{}", hex::encode(extrinsic));
        let mut subscription = self
            .inner
            .subscribe::<Value>(
                "author_submitAndWatchExtrinsic",
                rpc_params![extrinsic_hex],
                "author_unwatchExtrinsic",
            )
            .await
            .context("rpc author_submitAndWatchExtrinsic")?;
        let started = Instant::now();

        loop {
            let remaining = SUBMIT_TIMEOUT
                .checked_sub(started.elapsed())
                .ok_or_else(|| anyhow!("timed out waiting for author_submitAndWatchExtrinsic"))?;
            let status = timeout(remaining, subscription.next())
                .await
                .map_err(|_| anyhow!("timed out waiting for author_submitAndWatchExtrinsic"))?
                .ok_or_else(|| anyhow!("author_submitAndWatchExtrinsic subscription ended"))?
                .context("author_submitAndWatchExtrinsic update")?;
            match extrinsic_status(&status) {
                ExtrinsicStatus::Finalized(hash) => return Ok(hash),
                ExtrinsicStatus::Rejected(reason) => bail!("extrinsic {reason}"),
                ExtrinsicStatus::Pending => {}
            }
        }
    }
}

enum ExtrinsicStatus {
    Finalized(String),
    Rejected(String),
    Pending,
}

fn extrinsic_status(status: &Value) -> ExtrinsicStatus {
    if let Some(hash) = status.get("finalized").and_then(Value::as_str) {
        return ExtrinsicStatus::Finalized(hash.to_string());
    }
    for key in ["invalid", "dropped", "usurped", "finalityTimeout"] {
        if status.get(key).is_some() {
            return ExtrinsicStatus::Rejected(key.to_string());
        }
    }
    ExtrinsicStatus::Pending
}

fn value_to_params(value: Value) -> Result<RpcParams> {
    let Value::Array(values) = value else {
        bail!("RPC params must be a JSON array");
    };
    let mut params = RpcParams::new();
    for value in values {
        params.push(value).context("serialize RPC params")?;
    }
    Ok(params)
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).context("decode hex storage value")
}
