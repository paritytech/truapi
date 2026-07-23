//! Host-backed JSON-RPC helpers for statement-store allowance registration.

use core::time::Duration;

use futures::{FutureExt, pin_mut};
use serde_json::{Value, json};
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

    /// `state_getStorage(key)` at the current best block -> raw value bytes,
    /// or `None` if absent.
    pub async fn get_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        self.get_storage_maybe_at(key, None).await
    }

    /// `state_getStorage(key, at)` pinned to block `at` -> raw value bytes,
    /// or `None` if absent.
    pub async fn get_storage_at(&self, key: &[u8], at: &str) -> Result<Option<Vec<u8>>, String> {
        self.get_storage_maybe_at(key, Some(at)).await
    }

    async fn get_storage_maybe_at(
        &self,
        key: &[u8],
        at: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        let key_hex = format!("0x{}", hex::encode(key));
        let params = match at {
            Some(at) => rpc_params![key_hex, at],
            None => rpc_params![key_hex],
        };
        match self
            .inner
            .request::<Value>("state_getStorage", params)
            .await
            .map_err(rpc_error_message)?
        {
            Value::String(hex_value) => Ok(Some(decode_hex(&hex_value)?)),
            _ => Ok(None),
        }
    }

    /// `chain_getFinalizedHead` -> hash of the latest finalized block.
    pub async fn finalized_head(&self) -> Result<String, String> {
        let value = self.call("chain_getFinalizedHead", json!([])).await?;
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "chain_getFinalizedHead returned non-string".to_string())
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

#[derive(Debug, PartialEq)]
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
    for key in [
        "invalid",
        "dropped",
        "usurped",
        "retracted",
        "finalityTimeout",
    ] {
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
pub(crate) mod testing {
    //! Scripted JSON-RPC transport for exercising request shapes in tests.

    use std::sync::{Arc, Mutex};

    use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClientT};

    /// Records every request as `(method, params)` and replays canned JSON
    /// results in order; subscriptions replay the scripted notification items.
    #[derive(Clone, Default)]
    pub(crate) struct ScriptedRpc(Arc<Inner>);

    #[derive(Default)]
    struct Inner {
        calls: Mutex<Vec<(String, String)>>,
        responses: Mutex<Vec<String>>,
        subscription_items: Mutex<Vec<String>>,
    }

    impl ScriptedRpc {
        /// A script answering requests with `responses`, in order.
        pub(crate) fn new<'a>(responses: impl IntoIterator<Item = &'a str>) -> Self {
            let scripted = Self::default();
            *scripted.0.responses.lock().unwrap() =
                responses.into_iter().map(str::to_owned).collect();
            scripted
        }

        /// Queue the notification items for the next subscription.
        pub(crate) fn script_subscription<'a>(&self, items: impl IntoIterator<Item = &'a str>) {
            *self.0.subscription_items.lock().unwrap() =
                items.into_iter().map(str::to_owned).collect();
        }

        /// The `(method, params)` pairs seen so far.
        pub(crate) fn calls(&self) -> Vec<(String, String)> {
            self.0.calls.lock().unwrap().clone()
        }
    }

    fn params_json(params: Option<Box<RawValue>>) -> String {
        params.map_or_else(|| "[]".to_string(), |p| p.get().to_owned())
    }

    impl RpcClientT for ScriptedRpc {
        fn request_raw<'a>(
            &'a self,
            method: &'a str,
            params: Option<Box<RawValue>>,
        ) -> RawRpcFuture<'a, Box<RawValue>> {
            self.0
                .calls
                .lock()
                .unwrap()
                .push((method.to_owned(), params_json(params)));
            let mut responses = self.0.responses.lock().unwrap();
            assert!(!responses.is_empty(), "unscripted request `{method}`");
            let response = responses.remove(0);
            Box::pin(async move {
                Ok(RawValue::from_string(response).expect("scripted response is valid JSON"))
            })
        }

        fn subscribe_raw<'a>(
            &'a self,
            sub: &'a str,
            params: Option<Box<RawValue>>,
            _unsub: &'a str,
        ) -> RawRpcFuture<'a, RawRpcSubscription> {
            self.0
                .calls
                .lock()
                .unwrap()
                .push((sub.to_owned(), params_json(params)));
            let items: Vec<_> = core::mem::take(&mut *self.0.subscription_items.lock().unwrap())
                .into_iter()
                .map(|item| Ok(RawValue::from_string(item).expect("scripted item is valid JSON")))
                .collect();
            Box::pin(async move {
                Ok(RawRpcSubscription {
                    stream: Box::pin(futures::stream::iter(items)),
                    id: Some("scripted".to_string()),
                })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::testing::ScriptedRpc;
    use super::{ExtrinsicStatus, HostRpcClient, RpcClient, extrinsic_status};

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

    #[test]
    fn terminal_pool_statuses_reject_the_submission() {
        let statuses: Vec<ExtrinsicStatus> = [
            "invalid",
            "dropped",
            "usurped",
            "retracted",
            "finalityTimeout",
        ]
        .into_iter()
        .map(|key| extrinsic_status(&json!({key: "0x1234"})))
        .collect();

        assert_eq!(
            statuses,
            vec![
                ExtrinsicStatus::Rejected("invalid".to_string()),
                ExtrinsicStatus::Rejected("dropped".to_string()),
                ExtrinsicStatus::Rejected("usurped".to_string()),
                ExtrinsicStatus::Rejected("retracted".to_string()),
                ExtrinsicStatus::Rejected("finalityTimeout".to_string()),
            ],
        );
    }

    #[test]
    fn get_storage_at_pins_the_read_to_a_block() {
        let scripted = ScriptedRpc::new([r#""0x0102""#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let value = futures::executor::block_on(rpc.get_storage_at(b"key", "0xat")).unwrap();

        assert_eq!(value, Some(vec![0x01, 0x02]));
        assert_eq!(
            scripted.calls(),
            vec![(
                "state_getStorage".to_string(),
                r#"["0x6b6579","0xat"]"#.to_string(),
            )],
        );
    }

    #[test]
    fn get_storage_reads_at_the_current_block() {
        let scripted = ScriptedRpc::new(["null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let value = futures::executor::block_on(rpc.get_storage(b"key")).unwrap();

        assert_eq!(value, None);
        assert_eq!(
            scripted.calls(),
            vec![(
                "state_getStorage".to_string(),
                r#"["0x6b6579"]"#.to_string()
            )],
        );
    }

    #[test]
    fn finalized_head_returns_the_hash() {
        let scripted = ScriptedRpc::new([r#""0xfeed""#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let head = futures::executor::block_on(rpc.finalized_head()).unwrap();

        assert_eq!(head, "0xfeed");
        assert_eq!(
            scripted.calls(),
            vec![("chain_getFinalizedHead".to_string(), "[]".to_string())],
        );
    }
}
