//! SSO remote messaging over the people-chain statement store: submits an
//! encrypted request statement to the paired wallet and waits for the
//! matching response, honoring timeouts and local/peer disconnect signals.

use core::mem;
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::PlatformRuntimeHost;
use super::statement_store_rpc;
use crate::host_logic::session::{SessionInfo, SsoSessionInfo};
use crate::host_logic::sso::messages::{
    RemoteMessage, RemoteMessageData, RemoteMessageV1, SsoRemoteResponse, SsoSessionStatement,
    build_outgoing_request_statement, decode_sso_session_statement,
};
use crate::host_logic::statement_store::{current_unix_secs, parse_new_statements_result};

use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use serde_json::Value;
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::RpcSubscription;
use tracing::{debug, instrument, warn};
use truapi::CallContext;

const DEFAULT_SSO_STATEMENT_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
const DEFAULT_SSO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);
/// Disconnect reason reported when the local session logs out mid-request.
pub(super) const SSO_LOCAL_DISCONNECT_REASON: &str = "SSO session disconnected";
/// Disconnect reason reported when the paired wallet announces a disconnect.
pub(super) const SSO_PEER_DISCONNECT_REASON: &str = "SSO peer disconnected";

/// Registry of oneshot waiters resolved when the SSO session disconnects.
#[derive(Default)]
pub(super) struct SessionDisconnects {
    inner: Mutex<SessionDisconnectsInner>,
}

#[derive(Default)]
struct SessionDisconnectsInner {
    next_id: u64,
    waiters: Vec<(u64, oneshot::Sender<String>)>,
}

impl SessionDisconnects {
    /// Register a waiter; returns its id and the disconnect-reason receiver.
    pub(super) fn subscribe(&self) -> (u64, oneshot::Receiver<String>) {
        let (tx, rx) = oneshot::channel();
        let mut inner = self
            .inner
            .lock()
            .expect("session disconnect mutex poisoned");
        inner.next_id = inner.next_id.wrapping_add(1);
        let id = inner.next_id;
        inner.waiters.push((id, tx));
        (id, rx)
    }

    fn unsubscribe(&self, id: u64) {
        self.inner
            .lock()
            .expect("session disconnect mutex poisoned")
            .waiters
            .retain(|(waiter_id, _)| *waiter_id != id);
    }

    /// Resolve every pending waiter with `reason`.
    pub(super) fn notify(&self, reason: &'static str) {
        let waiters = {
            let mut inner = self
                .inner
                .lock()
                .expect("session disconnect mutex poisoned");
            mem::take(&mut inner.waiters)
        };
        for (_, waiter) in waiters {
            let _ = waiter.send(reason.to_string());
        }
    }
}

impl PlatformRuntimeHost {
    /// Best-effort `Disconnected` notification to the SSO peer.
    #[instrument(skip_all, fields(runtime.method = "sso.disconnect.submit"))]
    pub(super) async fn submit_sso_disconnected(
        &self,
        session: &SessionInfo,
    ) -> Result<(), String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let message_id = "truapi:sso:disconnect".to_string();
        let message = RemoteMessage {
            message_id: message_id.clone(),
            data: RemoteMessageData::V1(RemoteMessageV1::Disconnected),
        };
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![message],
            fresh_statement_expiry(),
        )?;
        self.statement_store_rpc()
            .submit_fire_and_forget(statement, "SSO statement-store")
            .await
            .map_err(|err| format!("SSO statement submit failed: {err}"))?;
        Ok(())
    }

    /// Submit an SSO remote message and wait for the wallet response with
    /// the default timeout.
    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit", action = action))]
    pub(super) async fn submit_sso_remote_message(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
    ) -> Result<SsoRemoteResponse, String> {
        self.submit_sso_remote_message_with_timeout(
            cx,
            session,
            action,
            message,
            Some(DEFAULT_SSO_RESPONSE_TIMEOUT),
        )
        .await
    }

    /// Submit an SSO remote message and wait for the wallet response without
    /// a deadline (used for flows that block on user interaction).
    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit_without_timeout", action = action))]
    pub(super) async fn submit_sso_remote_message_without_timeout(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
    ) -> Result<SsoRemoteResponse, String> {
        self.submit_sso_remote_message_with_timeout(cx, session, action, message, None)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit_with_timeout", action = action))]
    async fn submit_sso_remote_message_with_timeout(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
        timeout: Option<Duration>,
    ) -> Result<SsoRemoteResponse, String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let message_id = sso_message_id(cx, action);
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![message],
            fresh_statement_expiry(),
        )?;
        let rpc_client = self
            .statement_store_rpc()
            .client("SSO statement-store")
            .await?;
        let own_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_own)
            .await
            .map_err(|err| format!("SSO own statement-store subscribe failed: {err}"))?;
        let peer_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_peer)
            .await
            .map_err(|err| format!("SSO peer statement-store subscribe failed: {err}"))?;
        let submit_client = rpc_client.clone();
        let submit = async move { statement_store_rpc::submit(&submit_client, statement).await }
            .map(|result| result.map_err(|err| format!("SSO statement submit failed: {err}")))
            .boxed();
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
        let (disconnect_waiter_id, disconnect) = self.session_disconnects.subscribe();
        let result = wait_for_sso_remote_response(
            statement_subscription_stream(own_subscription, "own"),
            statement_subscription_stream(peer_subscription, "peer"),
            submit,
            SsoRemoteResponseWait {
                session: sso,
                statement_request_id: &message_id,
                remote_message_id: &message_id,
                timeout,
                disconnect: Some(disconnect),
            },
        )
        .await;
        self.session_disconnects.unsubscribe(disconnect_waiter_id);
        match &result {
            Ok(_) => debug!(action, %message_id, "SSO remote response received"),
            Err(reason) => warn!(action, %message_id, %reason, "SSO remote message failed"),
        }
        if matches!(&result, Err(reason) if reason == SSO_PEER_DISCONNECT_REASON) {
            self.session_disconnects.notify(SSO_PEER_DISCONNECT_REASON);
            self.clear_disconnected_session().await;
        }
        result
    }
}

struct SsoRemoteResponseWait<'a> {
    session: &'a SsoSessionInfo,
    statement_request_id: &'a str,
    remote_message_id: &'a str,
    timeout: Option<Duration>,
    disconnect: Option<oneshot::Receiver<String>>,
}

type StatementPageStream = BoxStream<'static, Result<Value, String>>;
type StatementSubmitFuture = BoxFuture<'static, Result<(), String>>;

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait"))]
async fn wait_for_sso_remote_response(
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

async fn subscribe_statement_topic(
    rpc_client: &RpcClient,
    topic: [u8; 32],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    statement_store_rpc::subscribe_match_all(rpc_client, &[topic]).await
}

fn statement_subscription_stream(
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
pub(super) fn sso_message_id(cx: &CallContext, action: &str) -> String {
    if cx.request_id().is_empty() {
        format!("truapi:sso:{action}")
    } else {
        cx.request_id().to_string()
    }
}

fn fresh_statement_expiry() -> u64 {
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
