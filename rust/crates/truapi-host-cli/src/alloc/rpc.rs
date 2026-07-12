//! Minimal JSON-RPC-over-WebSocket client for the People chain.
//!
//! Sequential request/response (one in flight at a time) plus a
//! submit-and-watch helper for `author_submitAndWatchExtrinsic`. Enough to read
//! metadata / storage / runtime version and submit the allowance extrinsic;
//! statement-store traffic keeps using the runtime's own transport.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Timeout for a single call's response.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout for an extrinsic to reach `inBlock`.
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(120);

/// A single WebSocket JSON-RPC connection.
pub struct RpcClient {
    ws: Mutex<Ws>,
    next_id: AtomicU64,
}

impl RpcClient {
    /// Open a WebSocket JSON-RPC connection to `url`.
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws, _) = connect_async(url)
            .await
            .with_context(|| format!("connect {url}"))?;
        Ok(Self {
            ws: Mutex::new(ws),
            next_id: AtomicU64::new(1),
        })
    }

    /// Call `method` with `params`, returning the `result` value.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut ws = self.ws.lock().await;
        ws.send(Message::Text(request.to_string()))
            .await
            .with_context(|| format!("send {method}"))?;
        loop {
            let value = next_json(&mut ws, CALL_TIMEOUT, method).await?;
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(err) = value.get("error") {
                    bail!("rpc error for {method}: {err}");
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// `state_getStorage(key)` -> raw value bytes, or `None` if absent.
    pub async fn get_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let key_hex = format!("0x{}", hex::encode(key));
        match self.call("state_getStorage", json!([key_hex])).await? {
            Value::String(hex_value) => Ok(Some(decode_hex(&hex_value)?)),
            _ => Ok(None),
        }
    }

    /// Submit an extrinsic and wait for `inBlock`/`finalized`; returns the block
    /// hash. Rejects on `invalid` / `dropped` / `usurped` / `finalityTimeout`.
    pub async fn submit_and_watch(&self, extrinsic: &[u8]) -> Result<String> {
        let extrinsic_hex = format!("0x{}", hex::encode(extrinsic));
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "author_submitAndWatchExtrinsic",
            "params": [extrinsic_hex],
        });
        let mut ws = self.ws.lock().await;
        ws.send(Message::Text(request.to_string()))
            .await
            .context("send author_submitAndWatchExtrinsic")?;

        // First the subscription id, then a stream of status notifications.
        let started = Instant::now();
        let mut subscription_id: Option<String> = None;
        loop {
            let remaining = SUBMIT_TIMEOUT
                .checked_sub(started.elapsed())
                .ok_or_else(|| anyhow!("timed out waiting for author_submitAndWatchExtrinsic"))?;
            let value = next_json(&mut ws, remaining, "author_submitAndWatchExtrinsic").await?;
            if subscription_id.is_none() {
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(err) = value.get("error") {
                        bail!("submit rejected: {err}");
                    }
                    subscription_id = value
                        .get("result")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if subscription_id.is_none() {
                        bail!("submit response missing subscription id: {value}");
                    }
                }
                continue;
            }
            let params = value.get("params");
            let matches = params
                .and_then(|p| p.get("subscription"))
                .and_then(Value::as_str)
                == subscription_id.as_deref();
            if !matches {
                continue;
            }
            if let Some(status) = params.and_then(|p| p.get("result")) {
                match extrinsic_status(status) {
                    ExtrinsicStatus::InBlock(hash) => return Ok(hash),
                    ExtrinsicStatus::Rejected(reason) => bail!("extrinsic {reason}"),
                    ExtrinsicStatus::Pending => {}
                }
            }
        }
    }
}

/// Terminal or pending state of a submitted extrinsic.
enum ExtrinsicStatus {
    InBlock(String),
    Rejected(String),
    Pending,
}

/// Classify an `author_extrinsicUpdate` status value.
///
/// Only `finalized` is terminal success: a freshly-set statement-store allowance
/// is not honored by the store until its `set_statement_store_account` extrinsic
/// is finalized, so returning at `inBlock` would race the handshake submit
/// (which then fails with `NoAllowance`).
fn extrinsic_status(status: &Value) -> ExtrinsicStatus {
    if let Some(hash) = status.get("finalized").and_then(Value::as_str) {
        return ExtrinsicStatus::InBlock(hash.to_string());
    }
    for key in ["invalid", "dropped", "usurped", "finalityTimeout"] {
        if status.get(key).is_some() {
            return ExtrinsicStatus::Rejected(key.to_string());
        }
    }
    ExtrinsicStatus::Pending
}

/// Read the next JSON frame, skipping non-text frames, within `deadline`.
async fn next_json(ws: &mut Ws, deadline: Duration, context: &str) -> Result<Value> {
    loop {
        let message = timeout(deadline, ws.next())
            .await
            .map_err(|_| anyhow!("timed out waiting for {context}"))?
            .ok_or_else(|| anyhow!("websocket closed waiting for {context}"))??;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Message::Close(_) => bail!("websocket closed waiting for {context}"),
            _ => continue,
        };
        return serde_json::from_str(&text).with_context(|| format!("parse {context} response"));
    }
}

/// Decode a `0x`-prefixed hex string to bytes.
fn decode_hex(value: &str) -> Result<Vec<u8>> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).context("decode hex storage value")
}
