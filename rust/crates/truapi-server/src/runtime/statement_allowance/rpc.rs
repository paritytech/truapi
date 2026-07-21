//! Host-backed JSON-RPC helpers for statement-store allowance registration.

use core::time::Duration;

use futures::{FutureExt, pin_mut};
use serde_json::Value;
use subxt_rpcs::RpcClient as HostRpcClient;
use subxt_rpcs::client::{RpcClient as NativeRpcClient, RpcParams, rpc_params};

/// Timeout for an allowance registration extrinsic to reach a block.
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Thin adapter matching the allowance allocator's minimal RPC surface.
#[derive(Clone)]
pub struct RpcClient {
    inner: HostRpcClient,
}

impl RpcClient {
    /// Open a native JSON-RPC connection to `url`.
    pub async fn connect(url: &str) -> Result<Self, String> {
        let inner = NativeRpcClient::from_insecure_url(url)
            .await
            .map_err(|err| format!("connect {url}: {err}"))?;
        Ok(Self { inner })
    }

    /// Wrap a platform-backed Subxt RPC client.
    pub fn new(inner: HostRpcClient) -> Self {
        Self { inner }
    }

    /// Call `method` with JSON-array `params`, returning the result value.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.inner
            .request(method, value_to_params(params)?)
            .await
            .map_err(rpc_error_message)
    }

    /// `state_getStorage(key)` -> raw value bytes, or `None` if absent.
    pub async fn get_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let key_hex = format!("0x{}", hex::encode(key));
        match self
            .inner
            .request::<Value>("state_getStorage", rpc_params![key_hex])
            .await
            .map_err(rpc_error_message)?
        {
            Value::String(hex_value) => Ok(Some(decode_hex(&hex_value)?)),
            _ => Ok(None),
        }
    }

    /// Submit an extrinsic and wait for `inBlock` or `finalized`; returns the block hash.
    pub async fn submit_and_watch(&self, extrinsic: &[u8]) -> Result<String, String> {
        let extrinsic_hex = format!("0x{}", hex::encode(extrinsic));
        let mut subscription = self
            .inner
            .subscribe::<Value>(
                "author_submitAndWatchExtrinsic",
                rpc_params![extrinsic_hex],
                "author_unwatchExtrinsic",
            )
            .await
            .map_err(rpc_error_message)?;
        let timeout = futures_timer::Delay::new(SUBMIT_TIMEOUT).fuse();
        pin_mut!(timeout);

        loop {
            let next = subscription.next().fuse();
            pin_mut!(next);
            let status = futures::select! {
                item = next => item.ok_or_else(|| {
                    "author_submitAndWatchExtrinsic subscription ended".to_string()
                })?.map_err(rpc_error_message)?,
                () = timeout => return Err(
                    "timed out waiting for author_submitAndWatchExtrinsic inclusion".to_string()
                ),
            };
            tracing::debug!(?status, "allowance extrinsic status");
            match extrinsic_status(&status) {
                ExtrinsicStatus::Included(hash) => return Ok(hash),
                ExtrinsicStatus::Rejected(reason) => return Err(format!("extrinsic {reason}")),
                ExtrinsicStatus::Pending => {}
            }
        }
    }
}

enum ExtrinsicStatus {
    Included(String),
    Rejected(String),
    Pending,
}

fn extrinsic_status(status: &Value) -> ExtrinsicStatus {
    for key in ["finalized", "inBlock"] {
        if let Some(hash) = status.get(key).and_then(Value::as_str) {
            return ExtrinsicStatus::Included(hash.to_string());
        }
    }
    for key in ["invalid", "dropped", "usurped", "finalityTimeout"] {
        if status.get(key).is_some() {
            return ExtrinsicStatus::Rejected(key.to_string());
        }
    }
    ExtrinsicStatus::Pending
}

fn value_to_params(value: Value) -> Result<RpcParams, String> {
    let Value::Array(values) = value else {
        return Err("RPC params must be a JSON array".to_string());
    };
    let mut params = RpcParams::new();
    for value in values {
        params.push(value).map_err(rpc_error_message)?;
    }
    Ok(params)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|err| format!("decode hex storage value: {err}"))
}

fn rpc_error_message(error: subxt_rpcs::Error) -> String {
    match error {
        subxt_rpcs::Error::User(error) => error.message,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ExtrinsicStatus, extrinsic_status};

    #[test]
    fn in_block_status_completes_submission() {
        let status = extrinsic_status(&json!({"inBlock": "0x1234"}));

        assert!(matches!(status, ExtrinsicStatus::Included(hash) if hash == "0x1234"));
    }

    #[test]
    fn finalized_status_completes_submission() {
        let status = extrinsic_status(&json!({"finalized": "0xabcd"}));

        assert!(matches!(status, ExtrinsicStatus::Included(hash) if hash == "0xabcd"));
    }
}
