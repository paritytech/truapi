//! In-memory statement-store relay for local end-to-end testing.
//!
//! Speaks the statement-store JSON-RPC intersection (host-spec N.5) over
//! WebSocket: `statement_submit`, `statement_subscribeStatement`,
//! `statement_unsubscribeStatement`. Statements are retained for the process
//! lifetime and replayed to new subscribers, so pairing works regardless of
//! which side connects first. Topic matching is on the statement's
//! `Topic1..4` fields; this is a test double, not a real statement store.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use truapi_server::host_logic::statement_store::decode_signed_statement;

/// Statement-store topic filter (host-spec N.5 / substrate `TopicFilter`).
#[derive(Clone)]
enum Filter {
    MatchAll(Vec<[u8; 32]>),
    MatchAny(Vec<[u8; 32]>),
}

impl Filter {
    fn matches(&self, topics: &[[u8; 32]]) -> bool {
        match self {
            Filter::MatchAll(wanted) => wanted.iter().all(|topic| topics.contains(topic)),
            Filter::MatchAny(wanted) => wanted.iter().any(|topic| topics.contains(topic)),
        }
    }
}

struct Subscription {
    id: String,
    filter: Filter,
    outbound: mpsc::UnboundedSender<Message>,
}

#[derive(Default)]
struct RelayState {
    statements: Vec<(Vec<u8>, Vec<[u8; 32]>)>,
    subscriptions: Vec<Subscription>,
}

/// Shared relay store: retained statements plus live subscriptions.
#[derive(Clone, Default)]
pub struct Relay {
    state: Arc<Mutex<RelayState>>,
    next_id: Arc<AtomicU64>,
}

impl Relay {
    /// Serve the relay on `addr` until the process exits. Returns the bound
    /// address (useful when `addr` uses port 0).
    pub async fn serve(addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("relay failed to bind {addr}"))?;
        let bound = listener.local_addr()?;
        info!(%bound, "statement-store relay listening");
        // Machine-readable readiness line for orchestrators.
        println!("RELAY_LISTENING ws://{bound}");
        let relay = Relay::default();
        loop {
            let (stream, peer) = listener.accept().await?;
            let relay = relay.clone();
            tokio::spawn(async move {
                if let Err(err) = relay.handle_connection(stream).await {
                    debug!(%peer, %err, "relay connection ended");
                }
            });
        }
    }

    async fn handle_connection(&self, stream: TcpStream) -> Result<()> {
        let ws = accept_async(stream).await?;
        let (mut write, mut read) = ws.split();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();

        let writer = tokio::spawn(async move {
            while let Some(message) = outbound_rx.recv().await {
                if write.send(message).await.is_err() {
                    break;
                }
            }
        });

        let mut connection_subscription_ids = Vec::new();
        while let Some(message) = read.next().await {
            let text = match message {
                Ok(Message::Text(text)) => text.to_string(),
                Ok(Message::Binary(bytes)) => match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => text,
                    Err(_) => continue,
                },
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            };
            if let Some(reply) = self
                .handle_request(&text, &outbound_tx, &mut connection_subscription_ids)
                .await
                && outbound_tx.send(Message::Text(reply)).is_err()
            {
                break;
            }
        }

        self.drop_subscriptions(&connection_subscription_ids).await;
        drop(outbound_tx);
        let _ = writer.await;
        Ok(())
    }

    /// Handle one JSON-RPC request, returning the reply frame to send back.
    async fn handle_request(
        &self,
        text: &str,
        outbound: &mpsc::UnboundedSender<Message>,
        connection_subscription_ids: &mut Vec<String>,
    ) -> Option<String> {
        let request: Value = serde_json::from_str(text).ok()?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method")?.as_str()?;
        let params = request.get("params");
        match method {
            "statement_submit" => Some(self.handle_submit(id, params).await),
            "statement_subscribeStatement" => {
                let (reply, sub_id) = self.handle_subscribe(id, params, outbound).await;
                if let Some(sub_id) = sub_id {
                    connection_subscription_ids.push(sub_id);
                }
                Some(reply)
            }
            "statement_unsubscribeStatement" => {
                if let Some(sub_id) = params
                    .and_then(|params| params.get(0))
                    .and_then(Value::as_str)
                {
                    self.drop_subscriptions(std::slice::from_ref(&sub_id.to_string()))
                        .await;
                    connection_subscription_ids.retain(|id| id != sub_id);
                }
                Some(result_frame(id, json!(true)))
            }
            other => {
                warn!(method = other, "relay received unsupported method");
                Some(error_frame(id, -32601, "method not supported"))
            }
        }
    }

    async fn handle_submit(&self, id: Value, params: Option<&Value>) -> String {
        let Some(statement) = params
            .and_then(|params| params.get(0))
            .and_then(Value::as_str)
            .and_then(decode_hex)
        else {
            return error_frame(id, -32602, "invalid statement_submit params");
        };
        let topics = statement_topics(&statement);
        let mut state = self.state.lock().await;
        state.statements.push((statement.clone(), topics.clone()));
        for subscription in &state.subscriptions {
            if subscription.filter.matches(&topics) {
                let _ = subscription
                    .outbound
                    .send(Message::Text(new_statements_frame(
                        &subscription.id,
                        &[&statement],
                    )));
            }
        }
        result_frame(id, json!("0xsubmitted"))
    }

    async fn handle_subscribe(
        &self,
        id: Value,
        params: Option<&Value>,
        outbound: &mpsc::UnboundedSender<Message>,
    ) -> (String, Option<String>) {
        let Some(filter) = params
            .and_then(|params| params.get(0))
            .and_then(parse_filter)
        else {
            return (
                error_frame(id, -32602, "invalid statement_subscribeStatement params"),
                None,
            );
        };
        let sub_id = format!("relay-sub-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let mut state = self.state.lock().await;
        let backlog: Vec<Vec<u8>> = state
            .statements
            .iter()
            .filter(|(_, topics)| filter.matches(topics))
            .map(|(statement, _)| statement.clone())
            .collect();
        state.subscriptions.push(Subscription {
            id: sub_id.clone(),
            filter,
            outbound: outbound.clone(),
        });
        drop(state);

        if !backlog.is_empty() {
            let refs: Vec<&[u8]> = backlog.iter().map(Vec::as_slice).collect();
            let _ = outbound.send(Message::Text(new_statements_frame(&sub_id, &refs)));
        }
        (result_frame(id, json!(sub_id)), Some(sub_id))
    }

    async fn drop_subscriptions(&self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let mut state = self.state.lock().await;
        state
            .subscriptions
            .retain(|subscription| !ids.contains(&subscription.id));
    }
}

fn parse_filter(value: &Value) -> Option<Filter> {
    if let Some(topics) = value.get("matchAll").and_then(Value::as_array) {
        return Some(Filter::MatchAll(parse_topics(topics)?));
    }
    if let Some(topics) = value.get("matchAny").and_then(Value::as_array) {
        return Some(Filter::MatchAny(parse_topics(topics)?));
    }
    if value.as_str() == Some("any") {
        return Some(Filter::MatchAny(Vec::new()));
    }
    None
}

fn parse_topics(values: &[Value]) -> Option<Vec<[u8; 32]>> {
    values
        .iter()
        .map(|value| {
            let bytes = value.as_str().and_then(decode_hex)?;
            <[u8; 32]>::try_from(bytes).ok()
        })
        .collect()
}

fn statement_topics(statement: &[u8]) -> Vec<[u8; 32]> {
    let mut topics: Vec<[u8; 32]> = decode_signed_statement(statement)
        .map(|signed| signed.topics)
        .unwrap_or_default();
    topics.sort();
    topics.dedup();
    topics
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).ok()
}

fn result_frame(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_frame(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

fn new_statements_frame(subscription_id: &str, statements: &[&[u8]]) -> String {
    let statements: Vec<String> = statements
        .iter()
        .map(|statement| format!("0x{}", hex::encode(statement)))
        .collect();
    json!({
        "jsonrpc": "2.0",
        "method": "statement_subscribeStatement",
        "params": {
            "subscription": subscription_id,
            "result": {
                "event": "newStatements",
                "data": { "statements": statements, "remaining": 0 },
            },
        },
    })
    .to_string()
}
