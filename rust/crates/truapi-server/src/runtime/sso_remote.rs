//! SSO remote messaging over the people-chain statement store: submits an
//! encrypted request statement to the paired signing host and waits for the
//! matching response, honoring timeouts and local/peer disconnect signals.

use core::mem;
use std::fmt::Display;
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::statement_store_rpc;
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::messages::{
    SsoRemoteResponse, SsoSessionStatement, decode_sso_session_statement,
};
use crate::host_logic::statement_store::{current_unix_secs, parse_new_statements_result};

use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use serde_json::Value;
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::RpcSubscription;
use tracing::instrument;
use truapi::CallContext;

/// Host-spec B.3.3 recommends seven-day statement expiry for session traffic:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L143-L145>
const DEFAULT_SSO_STATEMENT_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
/// Disconnect reason reported when the local session logs out mid-request.
pub(super) const SSO_LOCAL_DISCONNECT_REASON: &str = "SSO session disconnected";
/// Disconnect reason reported when the paired signing host announces a disconnect.
pub(super) const SSO_PEER_DISCONNECT_REASON: &str = "SSO peer disconnected";

/// Registry of oneshot waiters resolved when the SSO session disconnects.
#[derive(Default)]
pub(super) struct SessionDisconnects {
    inner: Mutex<SessionDisconnectsInner>,
}

#[derive(Default)]
struct SessionDisconnectsInner {
    next_id: u64,
    waiters: Vec<(u64, SsoSessionKey, oneshot::Sender<String>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SsoSessionKey {
    own: [u8; 32],
    peer: [u8; 32],
}

impl SsoSessionKey {
    pub(super) fn from_session(session: &SsoSessionInfo) -> Self {
        Self {
            own: session.session_id_own,
            peer: session.session_id_peer,
        }
    }
}

pub(super) struct SessionDisconnectGuard {
    disconnects: std::sync::Arc<SessionDisconnects>,
    id: u64,
}

impl Drop for SessionDisconnectGuard {
    fn drop(&mut self) {
        self.disconnects.unsubscribe(self.id);
    }
}

impl SessionDisconnects {
    /// Register a waiter; returns its id and the disconnect-reason receiver.
    pub(super) fn subscribe(
        self: &std::sync::Arc<Self>,
        session: &SsoSessionInfo,
    ) -> (SessionDisconnectGuard, oneshot::Receiver<String>) {
        let (tx, rx) = oneshot::channel();
        let mut inner = self
            .inner
            .lock()
            .expect("session disconnect mutex poisoned");
        inner.next_id = inner.next_id.wrapping_add(1);
        let id = inner.next_id;
        inner
            .waiters
            .push((id, SsoSessionKey::from_session(session), tx));
        (
            SessionDisconnectGuard {
                disconnects: self.clone(),
                id,
            },
            rx,
        )
    }

    fn unsubscribe(&self, id: u64) {
        self.inner
            .lock()
            .expect("session disconnect mutex poisoned")
            .waiters
            .retain(|(waiter_id, _, _)| *waiter_id != id);
    }

    /// Resolve pending waiters for one SSO session with `reason`.
    pub(super) fn notify(&self, session: &SsoSessionInfo, reason: &'static str) {
        self.notify_key(SsoSessionKey::from_session(session), reason);
    }

    pub(super) fn notify_key(&self, key: SsoSessionKey, reason: &'static str) {
        let waiters = {
            let mut inner = self
                .inner
                .lock()
                .expect("session disconnect mutex poisoned");
            let mut matching = Vec::new();
            let mut pending = Vec::with_capacity(inner.waiters.len());
            for waiter in mem::take(&mut inner.waiters) {
                if waiter.1 == key {
                    matching.push(waiter);
                } else {
                    pending.push(waiter);
                }
            }
            inner.waiters = pending;
            matching
        };
        for (_, _, waiter) in waiters {
            let _ = waiter.send(reason.to_string());
        }
    }
}

pub(super) struct SsoRemoteResponseWait<'a> {
    pub(super) session: &'a SsoSessionInfo,
    pub(super) statement_request_id: &'a str,
    pub(super) remote_message_id: &'a str,
    pub(super) timeout: Option<Duration>,
    pub(super) disconnect: Option<oneshot::Receiver<String>>,
}

pub(super) type StatementPageStream = BoxStream<'static, Result<Value, String>>;
pub(super) type StatementSubmitFuture = BoxFuture<'static, Result<(), String>>;

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait"))]
pub(super) async fn wait_for_sso_remote_response(
    own_statements: StatementPageStream,
    peer_statements: StatementPageStream,
    submit: StatementSubmitFuture,
    wait: SsoRemoteResponseWait<'_>,
) -> Result<SsoRemoteResponse, String> {
    let timeout_reason = wait.timeout.map(|timeout| {
        format!(
            "SSO response timed out after {} for {}",
            format_timeout_duration(timeout),
            wait.remote_message_id
        )
    });
    let response = wait_for_sso_remote_response_inner(
        own_statements,
        peer_statements,
        submit,
        wait.session,
        wait.statement_request_id,
        wait.remote_message_id,
    )
    .fuse();
    let timeout = async move {
        match (wait.timeout, timeout_reason) {
            (Some(timeout), Some(reason)) => {
                futures_timer::Delay::new(timeout).await;
                reason
            }
            _ => futures::future::pending::<String>().await,
        }
    }
    .fuse();
    let disconnect = async move {
        match wait.disconnect {
            Some(rx) => rx
                .await
                .unwrap_or_else(|_| SSO_LOCAL_DISCONNECT_REASON.to_string()),
            None => futures::future::pending::<String>().await,
        }
    }
    .fuse();
    pin_mut!(response, timeout, disconnect);
    futures::select! {
        result = response => result,
        reason = timeout => Err(reason),
        reason = disconnect => Err(reason),
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait_inner"))]
async fn wait_for_sso_remote_response_inner(
    own_statements: StatementPageStream,
    peer_statements: StatementPageStream,
    submit: StatementSubmitFuture,
    session: &SsoSessionInfo,
    statement_request_id: &str,
    remote_message_id: &str,
) -> Result<SsoRemoteResponse, String> {
    let mut own_statements = own_statements.fuse();
    let mut peer_statements = peer_statements.fuse();
    let mut submit = submit.fuse();
    let mut own_done = false;
    let mut peer_done = false;
    let mut request_accepted = false;
    let mut pending_remote_response = None;

    loop {
        if own_done && peer_done {
            return Err(format!(
                "SSO response stream ended before response for {}",
                remote_message_id
            ));
        }
        futures::select! {
            item = own_statements.next() => {
                match item {
                    Some(Ok(value)) => {
                        if let Some(response) = handle_sso_remote_statement_page(
                            session,
                            &value,
                            statement_request_id,
                            remote_message_id,
                            &mut request_accepted,
                            &mut pending_remote_response,
                        )? {
                            return Ok(response);
                        }
                    }
                    Some(Err(reason)) => return Err(reason),
                    None => own_done = true,
                }
            }
            item = peer_statements.next() => {
                match item {
                    Some(Ok(value)) => {
                        if let Some(response) = handle_sso_remote_statement_page(
                            session,
                            &value,
                            statement_request_id,
                            remote_message_id,
                            &mut request_accepted,
                            &mut pending_remote_response,
                        )? {
                            return Ok(response);
                        }
                    }
                    Some(Err(reason)) => return Err(reason),
                    None => peer_done = true,
                }
            }
            submit_result = submit => {
                submit_result?;
            }
        }
    }
}

fn handle_sso_remote_statement_page(
    session: &SsoSessionInfo,
    value: &Value,
    statement_request_id: &str,
    remote_message_id: &str,
    request_accepted: &mut bool,
    pending_remote_response: &mut Option<SsoRemoteResponse>,
) -> Result<Option<SsoRemoteResponse>, String> {
    let page = parse_new_statements_result("sso-remote".to_string(), value)
        .map_err(|err| err.to_string())?;
    for statement in page.statements {
        match decode_sso_session_statement(
            session,
            &statement,
            statement_request_id,
            remote_message_id,
        )? {
            Some(SsoSessionStatement::RequestAccepted) => {
                *request_accepted = true;
                if let Some(response) = pending_remote_response.take() {
                    return Ok(Some(response));
                }
            }
            Some(SsoSessionStatement::RemoteResponse(response)) => {
                if *request_accepted {
                    return Ok(Some(response));
                }
                *pending_remote_response = Some(response);
            }
            Some(SsoSessionStatement::Disconnected) => {
                return Err(SSO_PEER_DISCONNECT_REASON.to_string());
            }
            None => {}
        }
    }
    Ok(None)
}

pub(super) async fn subscribe_statement_topic(
    rpc_client: &RpcClient,
    topic: [u8; 32],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    statement_store_rpc::subscribe_match_all(rpc_client, &[topic]).await
}

pub(super) fn statement_subscription_stream(
    subscription: RpcSubscription<Value>,
    label: &'static str,
) -> StatementPageStream {
    subscription
        .map(move |item| item.map_err(|err| format!("SSO {label} subscription failed: {err}")))
        .boxed()
}

fn format_timeout_duration(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

/// Stable message id for an SSO request: the wire request id when present,
/// otherwise a fixed per-action fallback.
pub(super) fn sso_message_id(cx: &CallContext, action: impl Display) -> String {
    if cx.request_id().is_empty() {
        format!("truapi:sso:{action}")
    } else {
        cx.request_id().to_string()
    }
}

pub(super) fn fresh_statement_expiry() -> u64 {
    let timestamp = current_unix_secs().saturating_add(DEFAULT_SSO_STATEMENT_EXPIRY_SECS);
    timestamp << 32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sso_session_info;
    use futures::stream;

    #[test]
    fn sso_remote_response_waiter_times_out() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            stream::pending().boxed(),
            futures::future::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_millis(1)),
                disconnect: None,
            },
        ))
        .unwrap_err();

        assert_eq!(err, "SSO response timed out after 1ms for request-1");
    }

    #[test]
    fn sso_remote_response_waiter_reports_submit_rejections() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            stream::pending().boxed(),
            futures::future::ready(Err("SSO statement submit failed: no allowance".to_string()))
                .boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_secs(60)),
                disconnect: None,
            },
        ))
        .unwrap_err();

        assert_eq!(err, "SSO statement submit failed: no allowance");
    }

    #[test]
    fn sso_remote_response_waiter_stops_on_local_disconnect_signal() {
        let session = sso_session_info();
        let (tx, rx) = oneshot::channel();
        tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            stream::pending().boxed(),
            futures::future::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_secs(60)),
                disconnect: Some(rx),
            },
        ))
        .unwrap_err();

        assert_eq!(err, SSO_LOCAL_DISCONNECT_REASON);
    }

    #[test]
    fn sso_remote_response_waiter_without_timeout_stops_on_local_disconnect_signal() {
        let session = sso_session_info();
        let (tx, rx) = oneshot::channel();
        tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            stream::pending().boxed(),
            futures::future::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: None,
                disconnect: Some(rx),
            },
        ))
        .unwrap_err();

        assert_eq!(err, SSO_LOCAL_DISCONNECT_REASON);
    }
}
