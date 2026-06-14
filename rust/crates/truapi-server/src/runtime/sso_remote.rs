//! SSO remote messaging over the people-chain statement store: submits an
//! encrypted request statement to the paired wallet and waits for the
//! matching response, honoring timeouts and local/peer disconnect signals.

use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::PlatformRuntimeHost;
use crate::host_logic::session::{SessionInfo, SsoSessionInfo};
use crate::host_logic::sso_messages::{
    RemoteMessage, RemoteMessageData, RemoteMessageV1, SsoRemoteResponse, SsoSessionStatement,
    build_outgoing_request_statement, decode_sso_session_statement,
};
use crate::host_logic::statement_store::{
    current_unix_secs, parse_new_statements, parse_submit_ack, parse_subscribe_ack,
    submit_statement_request, subscribe_match_all_request, unsubscribe_request,
};

use futures::channel::oneshot;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use tracing::{debug, instrument, warn};
use truapi::CallContext;
use truapi_platform::{ChainProvider as PlatformChainProvider, JsonRpcConnection, Platform};

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
            std::mem::take(&mut inner.waiters)
        };
        for (_, waiter) in waiters {
            let _ = waiter.send(reason.to_string());
        }
    }
}

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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
        let connection = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| format!("SSO statement-store connect failed: {err:?}"))?;
        connection.send(submit_statement_request(
            &format!("truapi:sso-submit:{message_id}"),
            &statement,
        ));
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
        let connection = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| format!("SSO statement-store connect failed: {err:?}"))?;
        let own_subscription_request_id = format!("truapi:sso-sub-own:{message_id}");
        let peer_subscription_request_id = format!("truapi:sso-sub-peer:{message_id}");
        let submit_request_id = format!("truapi:sso-submit:{message_id}");
        connection.send(subscribe_match_all_request(
            &own_subscription_request_id,
            &[sso.session_id_own],
        ));
        connection.send(subscribe_match_all_request(
            &peer_subscription_request_id,
            &[sso.session_id_peer],
        ));
        connection.send(submit_statement_request(&submit_request_id, &statement));
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
        let responses = connection.responses();
        let subscription_guard = SsoRemoteSubscriptionGuard::new(
            connection,
            own_subscription_request_id.clone(),
            peer_subscription_request_id.clone(),
        );
        let (disconnect_waiter_id, disconnect) = self.session_disconnects.subscribe();
        let result = wait_for_sso_remote_response(
            responses,
            SsoRemoteResponseWait {
                session: sso,
                own_subscription_request_id: &own_subscription_request_id,
                peer_subscription_request_id: &peer_subscription_request_id,
                submit_request_id: &submit_request_id,
                statement_request_id: &message_id,
                remote_message_id: &message_id,
                timeout,
                disconnect: Some(disconnect),
                own_remote_subscription_id: subscription_guard.own_remote_subscription_id(),
                peer_remote_subscription_id: subscription_guard.peer_remote_subscription_id(),
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
    own_subscription_request_id: &'a str,
    peer_subscription_request_id: &'a str,
    submit_request_id: &'a str,
    statement_request_id: &'a str,
    remote_message_id: &'a str,
    timeout: Option<Duration>,
    disconnect: Option<oneshot::Receiver<String>>,
    own_remote_subscription_id: SharedRemoteSubscriptionId,
    peer_remote_subscription_id: SharedRemoteSubscriptionId,
}

struct SsoRemoteResponseTarget<'a> {
    session: &'a SsoSessionInfo,
    own_subscription_request_id: &'a str,
    peer_subscription_request_id: &'a str,
    submit_request_id: &'a str,
    statement_request_id: &'a str,
    remote_message_id: &'a str,
    own_remote_subscription_slot: SharedRemoteSubscriptionId,
    peer_remote_subscription_slot: SharedRemoteSubscriptionId,
}

/// Shared slot a response waiter fills with the remote subscription id so
/// the owning guard can unsubscribe on drop.
pub(crate) type SharedRemoteSubscriptionId = Arc<Mutex<Option<String>>>;

struct SsoRemoteSubscriptionGuard {
    connection: Box<dyn JsonRpcConnection>,
    own_unsubscribe_request_id: String,
    peer_unsubscribe_request_id: String,
    own_remote_subscription_id: SharedRemoteSubscriptionId,
    peer_remote_subscription_id: SharedRemoteSubscriptionId,
}

impl SsoRemoteSubscriptionGuard {
    fn new(
        connection: Box<dyn JsonRpcConnection>,
        own_subscription_request_id: String,
        peer_subscription_request_id: String,
    ) -> Self {
        Self {
            connection,
            own_unsubscribe_request_id: format!("{own_subscription_request_id}:unsubscribe"),
            peer_unsubscribe_request_id: format!("{peer_subscription_request_id}:unsubscribe"),
            own_remote_subscription_id: Arc::new(Mutex::new(None)),
            peer_remote_subscription_id: Arc::new(Mutex::new(None)),
        }
    }

    fn own_remote_subscription_id(&self) -> SharedRemoteSubscriptionId {
        self.own_remote_subscription_id.clone()
    }

    fn peer_remote_subscription_id(&self) -> SharedRemoteSubscriptionId {
        self.peer_remote_subscription_id.clone()
    }
}

impl Drop for SsoRemoteSubscriptionGuard {
    fn drop(&mut self) {
        if let Some(remote_subscription_id) = self
            .own_remote_subscription_id
            .lock()
            .expect("SSO own subscription id mutex poisoned")
            .as_ref()
        {
            self.connection.send(unsubscribe_request(
                &self.own_unsubscribe_request_id,
                remote_subscription_id,
            ));
        }
        if let Some(remote_subscription_id) = self
            .peer_remote_subscription_id
            .lock()
            .expect("SSO peer subscription id mutex poisoned")
            .as_ref()
        {
            self.connection.send(unsubscribe_request(
                &self.peer_unsubscribe_request_id,
                remote_subscription_id,
            ));
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait"))]
async fn wait_for_sso_remote_response(
    responses: BoxStream<'static, String>,
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
        responses,
        SsoRemoteResponseTarget {
            session: wait.session,
            own_subscription_request_id: wait.own_subscription_request_id,
            peer_subscription_request_id: wait.peer_subscription_request_id,
            submit_request_id: wait.submit_request_id,
            statement_request_id: wait.statement_request_id,
            remote_message_id: wait.remote_message_id,
            own_remote_subscription_slot: wait.own_remote_subscription_id.clone(),
            peer_remote_subscription_slot: wait.peer_remote_subscription_id.clone(),
        },
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
    mut responses: BoxStream<'static, String>,
    target: SsoRemoteResponseTarget<'_>,
) -> Result<SsoRemoteResponse, String> {
    let mut own_remote_subscription_id: Option<String> = None;
    let mut peer_remote_subscription_id: Option<String> = None;
    let mut request_accepted = false;
    let mut pending_remote_response = None;

    while let Some(frame) = responses.next().await {
        if own_remote_subscription_id.is_none()
            && let Some(id) = parse_subscribe_ack(&frame, target.own_subscription_request_id)
                .map_err(|err| err.to_string())?
        {
            *target
                .own_remote_subscription_slot
                .lock()
                .expect("SSO own subscription id mutex poisoned") = Some(id.clone());
            own_remote_subscription_id = Some(id);
            continue;
        }
        if peer_remote_subscription_id.is_none()
            && let Some(id) = parse_subscribe_ack(&frame, target.peer_subscription_request_id)
                .map_err(|err| err.to_string())?
        {
            *target
                .peer_remote_subscription_slot
                .lock()
                .expect("SSO peer subscription id mutex poisoned") = Some(id.clone());
            peer_remote_subscription_id = Some(id);
            continue;
        }

        let submit_ack = parse_submit_ack(&frame, target.submit_request_id)
            .map_err(|err| format!("SSO statement submit failed: {err}"))?;
        if submit_ack.is_some() {
            continue;
        }

        let Some(page) = parse_new_statements(&frame).map_err(|err| err.to_string())? else {
            continue;
        };
        if !subscription_id_matches(
            &page.remote_subscription_id,
            own_remote_subscription_id.as_deref(),
            peer_remote_subscription_id.as_deref(),
        ) {
            continue;
        }

        for statement in page.statements {
            match decode_sso_session_statement(
                target.session,
                &statement,
                target.statement_request_id,
                target.remote_message_id,
            )? {
                Some(SsoSessionStatement::RequestAccepted) => {
                    request_accepted = true;
                    if let Some(response) = pending_remote_response.take() {
                        return Ok(response);
                    }
                }
                Some(SsoSessionStatement::RemoteResponse(response)) => {
                    if request_accepted {
                        return Ok(response);
                    }
                    pending_remote_response = Some(response);
                }
                Some(SsoSessionStatement::Disconnected) => {
                    return Err(SSO_PEER_DISCONNECT_REASON.to_string());
                }
                None => {}
            }
        }
    }

    Err(format!(
        "SSO response stream ended before response for {}",
        target.remote_message_id
    ))
}

fn format_timeout_duration(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn subscription_id_matches(
    remote_subscription_id: &str,
    own_remote_subscription_id: Option<&str>,
    peer_remote_subscription_id: Option<&str>,
) -> bool {
    own_remote_subscription_id == Some(remote_subscription_id)
        || peer_remote_subscription_id == Some(remote_subscription_id)
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
    use crate::test_support::{remote_subscription_slot, sso_session_info};
    use futures::stream;

    #[test]
    fn sso_remote_response_waiter_times_out() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_millis(1)),
                disconnect: None,
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(err, "SSO response timed out after 1ms for request-1");
    }

    #[test]
    fn sso_remote_response_waiter_reports_submit_rejections() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::iter(vec![
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": "submit",
                    "error": {
                        "code": -32000,
                        "message": "no allowance"
                    },
                })
                .to_string(),
            ])
            .boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_secs(60)),
                disconnect: None,
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(
            err,
            "SSO statement submit failed: malformed statement-store frame: no allowance"
        );
    }

    #[test]
    fn sso_remote_response_waiter_stops_on_local_disconnect_signal() {
        let session = sso_session_info();
        let (tx, rx) = oneshot::channel();
        tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_secs(60)),
                disconnect: Some(rx),
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
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
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: None,
                disconnect: Some(rx),
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(err, SSO_LOCAL_DISCONNECT_REASON);
    }
}
