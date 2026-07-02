//! `subxt-rpcs` client adapter for host-provided JSON-RPC pipes.
//!
//! The platform owns the physical chain connection. This module owns only the
//! generic JSON-RPC mechanics needed to expose that pipe as a
//! [`subxt_rpcs::RpcClientT`]: request correlation, subscription routing, and
//! best-effort unsubscribe on subscription drop.

#![allow(dead_code)]

use core::fmt;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, pin_mut};
use futures::{Stream, StreamExt};
use serde::Serialize;
use serde_json::value::RawValue;
use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RpcClientT};
use subxt_rpcs::{Error as RpcError, UserError};
use tracing::instrument;
use truapi_platform::JsonRpcConnection;

use crate::subscription::Spawner;

const MAX_BUFFERED_SUBSCRIPTIONS: usize = 64;
const MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION: usize = 256;

/// JSON-RPC client backed by a host-owned [`JsonRpcConnection`].
pub(crate) struct HostRpcClient {
    inner: Arc<HostRpcClientInner>,
}

struct HostRpcClientInner {
    connection: Arc<dyn JsonRpcConnection>,
    request_ids: AtomicU64,
    user_handles: AtomicUsize,
    closed: AtomicBool,
    stop_response_loop: Mutex<Option<oneshot::Sender<()>>>,
    pending: Mutex<HashMap<String, PendingRequest>>,
    subscriptions: Mutex<HashMap<String, SubscriptionSink>>,
    buffered_subscription_items: Mutex<HashMap<String, Vec<Box<RawValue>>>>,
}

struct HostRpcClientLease {
    inner: Arc<HostRpcClientInner>,
}

struct PendingRequest {
    tx: oneshot::Sender<Result<Box<RawValue>, RpcError>>,
}

#[derive(Clone)]
struct SubscriptionSink {
    tx: mpsc::UnboundedSender<Result<Box<RawValue>, RpcError>>,
}

#[derive(Debug)]
struct HostRpcClientError(String);

impl fmt::Display for HostRpcClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HostRpcClientError {}

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    id: &'a str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a RawValue>,
}

impl HostRpcClient {
    /// Wrap `connection` and start the response pump on `spawner`.
    pub(crate) fn new(connection: Arc<dyn JsonRpcConnection>, spawner: Spawner) -> Self {
        let (stop_response_tx, stop_response_rx) = oneshot::channel();
        let client = Self {
            inner: Arc::new(HostRpcClientInner {
                connection,
                request_ids: AtomicU64::new(1),
                user_handles: AtomicUsize::new(1),
                closed: AtomicBool::new(false),
                stop_response_loop: Mutex::new(Some(stop_response_tx)),
                pending: Mutex::new(HashMap::new()),
                subscriptions: Mutex::new(HashMap::new()),
                buffered_subscription_items: Mutex::new(HashMap::new()),
            }),
        };
        client.spawn_response_loop(spawner, stop_response_rx);
        client
    }

    /// Whether the underlying response stream has ended or failed.
    pub(crate) fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Relaxed)
    }

    /// Send a JSON-RPC request without waiting for its response.
    ///
    /// Used by best-effort notifications where the caller must not block on
    /// the remote endpoint acknowledging the request.
    pub(crate) fn send_fire_and_forget(
        &self,
        method: &str,
        params: Option<Box<RawValue>>,
    ) -> Result<(), RpcError> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(client_error("json-rpc connection is closed"));
        }
        let id = self.inner.next_request_id();
        self.inner.send_request(&id, method, params.as_deref())
    }

    fn spawn_response_loop(&self, spawner: Spawner, stop_rx: oneshot::Receiver<()>) {
        let inner = self.inner.clone();
        let fut = async move {
            let mut responses = inner.connection.responses();
            let stop = stop_rx.fuse();
            pin_mut!(stop);
            loop {
                futures::select! {
                    _ = stop => return,
                    frame = responses.next().fuse() => match frame {
                        Some(frame) => {
                            if let Err(error) = inner.handle_frame(&frame) {
                                inner.close_with_error(error);
                                return;
                            }
                        }
                        None => {
                            inner.close_with_error(client_error("json-rpc response stream ended"));
                            return;
                        }
                    }
                }
            }
        };
        (spawner)(fut.boxed());
    }
}

impl Clone for HostRpcClient {
    fn clone(&self) -> Self {
        self.inner.retain_user_handle();
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for HostRpcClient {
    fn drop(&mut self) {
        self.inner.release_user_handle();
    }
}

impl HostRpcClientInner {
    fn retain_user_handle(&self) {
        self.user_handles.fetch_add(1, Ordering::Relaxed);
    }

    fn acquire_lease(self: &Arc<Self>) -> HostRpcClientLease {
        self.retain_user_handle();
        HostRpcClientLease {
            inner: self.clone(),
        }
    }

    fn release_user_handle(&self) {
        let previous = self.user_handles.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "host rpc client handle count underflow");
        if previous == 1 {
            self.close_with_error(client_error("json-rpc client dropped"));
        }
    }

    fn next_request_id(&self) -> String {
        format!(
            "truapi:{}",
            self.request_ids.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn send_request(
        &self,
        id: &str,
        method: &str,
        params: Option<&RawValue>,
    ) -> Result<(), RpcError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let encoded = serde_json::to_string(&request).map_err(RpcError::Serialization)?;
        self.connection.send(encoded);
        Ok(())
    }

    async fn request(
        &self,
        method: &str,
        params: Option<Box<RawValue>>,
    ) -> Result<Box<RawValue>, RpcError> {
        let id = self.next_request_id();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if self.closed.load(Ordering::Relaxed) {
                return Err(client_error("json-rpc connection is closed"));
            }
            pending.insert(id.clone(), PendingRequest { tx });
        }

        if let Err(error) = self.send_request(&id, method, params.as_deref()) {
            self.pending.lock().unwrap().remove(&id);
            return Err(error);
        }

        rx.await
            .map_err(|_| client_error("json-rpc request was cancelled"))?
    }

    async fn subscribe(
        self: Arc<Self>,
        method: &str,
        params: Option<Box<RawValue>>,
        unsubscribe_method: &str,
        lease: HostRpcClientLease,
    ) -> Result<RawRpcSubscription, RpcError> {
        let raw_id = self.request(method, params).await?;
        let subscription_id = subscription_id_from_raw(raw_id.as_ref())?;
        let (tx, rx) = mpsc::unbounded();
        {
            let mut subscriptions = self.subscriptions.lock().unwrap();
            if self.closed.load(Ordering::Relaxed) {
                return Err(client_error("json-rpc connection is closed"));
            }
            subscriptions.insert(subscription_id.clone(), SubscriptionSink { tx: tx.clone() });
        }

        let buffered = self
            .buffered_subscription_items
            .lock()
            .unwrap()
            .remove(&subscription_id)
            .unwrap_or_default();
        for item in buffered {
            let _ = tx.unbounded_send(Ok(item));
        }

        let stream = SubscriptionStream {
            inner: rx,
            client: self,
            _lease: lease,
            subscription_id: subscription_id.clone(),
            unsubscribe_method: unsubscribe_method.to_string(),
            closed: false,
        };
        Ok(RawRpcSubscription {
            stream: Box::pin(stream),
            id: Some(subscription_id),
        })
    }

    fn unsubscribe(&self, subscription_id: &str, unsubscribe_method: &str) {
        self.subscriptions.lock().unwrap().remove(subscription_id);
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        let id = self.next_request_id();
        let params = RawValue::from_string(format!(
            "[{}]",
            serde_json::to_string(subscription_id).unwrap_or_else(|_| "\"\"".to_string())
        ));
        if let Ok(params) = params {
            let _ = self.send_request(&id, unsubscribe_method, Some(params.as_ref()));
        }
    }

    #[instrument(skip_all, fields(runtime.method = "host_rpc_client.handle_frame"))]
    fn handle_frame(&self, frame: &str) -> Result<(), RpcError> {
        let value: serde_json::Value =
            serde_json::from_str(frame).map_err(RpcError::Deserialization)?;

        if value.get("method").is_some() && value.get("params").is_some() {
            self.handle_notification(&value)?;
            return Ok(());
        }

        let Some(request_id) = value.get("id").and_then(json_id) else {
            return Ok(());
        };
        let Some(pending) = self.pending.lock().unwrap().remove(&request_id) else {
            return Ok(());
        };

        if let Some(result) = value.get("result") {
            let raw = raw_value_from_json(result)?;
            let _ = pending.tx.send(Ok(raw));
            return Ok(());
        }

        if let Some(error) = value.get("error") {
            let _ = pending.tx.send(Err(user_error_from_json(error)));
            return Ok(());
        }

        let _ = pending.tx.send(Err(client_error(
            "json-rpc response missing result and error",
        )));
        Ok(())
    }

    fn handle_notification(&self, value: &serde_json::Value) -> Result<(), RpcError> {
        let Some(params) = value.get("params") else {
            return Ok(());
        };
        let Some(subscription_id) = params.get("subscription").and_then(json_id) else {
            return Ok(());
        };
        let Some(result) = params.get("result") else {
            return Ok(());
        };
        let raw = raw_value_from_json(result)?;
        let sink = self
            .subscriptions
            .lock()
            .unwrap()
            .get(&subscription_id)
            .cloned();
        match sink {
            Some(sink) => {
                let _ = sink.tx.unbounded_send(Ok(raw));
            }
            None => self.buffer_subscription_item(subscription_id, raw),
        }
        Ok(())
    }

    fn buffer_subscription_item(&self, subscription_id: String, item: Box<RawValue>) {
        let mut buffered = self.buffered_subscription_items.lock().unwrap();
        let known = buffered.contains_key(&subscription_id);
        if !known && buffered.len() >= MAX_BUFFERED_SUBSCRIPTIONS {
            return;
        }
        let items = buffered.entry(subscription_id).or_default();
        if items.len() >= MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION {
            return;
        }
        items.push(item);
    }

    fn close_with_error(&self, error: RpcError) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(stop) = self.stop_response_loop.lock().unwrap().take() {
            let _ = stop.send(());
        }
        self.connection.close();

        let pending = {
            let mut pending = self.pending.lock().unwrap();
            mem::take(&mut *pending)
        };
        for (_, pending) in pending {
            let _ = pending.tx.send(Err(client_error(format!(
                "json-rpc connection closed: {error}"
            ))));
        }

        let subscriptions = mem::take(&mut *self.subscriptions.lock().unwrap());
        for (_, sink) in subscriptions {
            let _ = sink.tx.unbounded_send(Err(client_error(format!(
                "json-rpc connection closed: {error}"
            ))));
        }
        self.buffered_subscription_items.lock().unwrap().clear();
    }
}

impl Drop for HostRpcClientLease {
    fn drop(&mut self) {
        self.inner.release_user_handle();
    }
}

impl RpcClientT for HostRpcClient {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<RawValue>>,
    ) -> RawRpcFuture<'a, Box<RawValue>> {
        Box::pin(async move { self.inner.request(method, params).await })
    }

    fn subscribe_raw<'a>(
        &'a self,
        sub: &'a str,
        params: Option<Box<RawValue>>,
        unsub: &'a str,
    ) -> RawRpcFuture<'a, RawRpcSubscription> {
        let lease = self.inner.acquire_lease();
        Box::pin(async move {
            self.inner
                .clone()
                .subscribe(sub, params, unsub, lease)
                .await
        })
    }
}

struct SubscriptionStream {
    inner: mpsc::UnboundedReceiver<Result<Box<RawValue>, RpcError>>,
    client: Arc<HostRpcClientInner>,
    _lease: HostRpcClientLease,
    subscription_id: String,
    unsubscribe_method: String,
    closed: bool,
}

impl Drop for SubscriptionStream {
    fn drop(&mut self) {
        if !self.closed {
            self.closed = true;
            self.client
                .unsubscribe(&self.subscription_id, &self.unsubscribe_method);
        }
    }
}

impl Stream for SubscriptionStream {
    type Item = Result<Box<RawValue>, RpcError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(None) => {
                this.closed = true;
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

fn raw_value_from_json(value: &serde_json::Value) -> Result<Box<RawValue>, RpcError> {
    RawValue::from_string(value.to_string()).map_err(RpcError::Deserialization)
}

fn subscription_id_from_raw(raw: &RawValue) -> Result<String, RpcError> {
    let value: serde_json::Value =
        serde_json::from_str(raw.get()).map_err(RpcError::Deserialization)?;
    json_id(&value).ok_or_else(|| client_error("json-rpc subscription id is not a string"))
}

fn json_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn user_error_from_json(value: &serde_json::Value) -> RpcError {
    match serde_json::from_value::<UserError>(value.clone()) {
        Ok(error) => RpcError::User(error),
        Err(error) => RpcError::Deserialization(error),
    }
}

fn client_error(reason: impl Into<String>) -> RpcError {
    RpcError::Client(Box::new(HostRpcClientError(reason.into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    use futures::executor::block_on;
    use futures::stream::BoxStream;
    use serde_json::{Value, json};
    use subxt_rpcs::RpcClient;
    use subxt_rpcs::client::rpc_params;

    use crate::subscription::thread_per_subscription_spawner;

    struct TrackingConnection {
        sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
        receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
        close_count: AtomicUsize,
    }

    impl TrackingConnection {
        fn new() -> Arc<Self> {
            let (tx, rx) = mpsc::unbounded();
            Arc::new(Self {
                sender: Mutex::new(Some(tx)),
                receiver: Mutex::new(Some(rx)),
                close_count: AtomicUsize::new(0),
            })
        }

        fn close_count(&self) -> usize {
            self.close_count.load(Ordering::SeqCst)
        }
    }

    impl JsonRpcConnection for TrackingConnection {
        fn send(&self, request: String) {
            let Ok(value) = serde_json::from_str::<Value>(&request) else {
                return;
            };
            let Some(id) = value.get("id").cloned() else {
                return;
            };
            if value.get("method").and_then(Value::as_str) == Some("sub") {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": "sub-1",
                });
                if let Some(sender) = self.sender.lock().unwrap().as_ref() {
                    let _ = sender.unbounded_send(response.to_string());
                }
            }
        }

        fn responses(&self) -> BoxStream<'static, String> {
            self.receiver
                .lock()
                .unwrap()
                .take()
                .expect("responses called twice")
                .boxed()
        }

        fn close(&self) {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            self.sender.lock().unwrap().take();
        }
    }

    #[test]
    fn dropping_one_shot_client_closes_connection_lease() {
        let connection = TrackingConnection::new();
        let spawner: Spawner = Arc::new(|_| {});

        {
            let client = HostRpcClient::new(connection.clone(), spawner);
            client
                .send_fire_and_forget("statement_submit", None)
                .unwrap();
        }

        assert_eq!(connection.close_count(), 1);
    }

    #[test]
    fn subscription_stream_holds_connection_lease_until_dropped() {
        let connection = TrackingConnection::new();
        let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
        let rpc_client = RpcClient::new(client.clone());

        let subscription = block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
            .expect("subscription should start");

        drop(rpc_client);
        drop(client);
        assert_eq!(connection.close_count(), 0);

        drop(subscription);
        assert_eq!(connection.close_count(), 1);
    }
}
