//! SSO statement-store channel to the paired remote signing host.
//!
//! The channel half of [`PairingHost`]: message submission and response
//! correlation over the statement store, plus the peer-disconnect monitor.
//! Role policy (session lifecycle, login, revalidation) stays in the parent
//! module.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::super::authority::{
    AuthorityError, CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest,
    SignRawAuthorityRequest,
};
use super::super::sso_remote::{
    SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON, SsoRemoteResponseWait, SsoSessionKey,
    fresh_statement_expiry, sso_message_id, statement_subscription_stream,
    subscribe_statement_topic, wait_for_sso_remote_response,
};
use super::super::statement_store_rpc::{self, StatementStoreRpc};
use super::AuthorityRequestKind;
use super::PairingHost;
use crate::host_logic::session::{SessionInfo, SessionState, SsoSessionInfo};
use crate::host_logic::sso::messages::{
    OnExistingAllowancePolicy, RemoteMessage, RemoteMessageData, SsoAllocationOutcome,
    SsoRemoteResponse, SsoSessionStatement, alias_request_message,
    build_outgoing_request_statement, create_transaction_message, decode_sso_session_statement,
    resource_allocation_message, sign_payload_message, sign_raw_message, v1,
};
use crate::host_logic::statement_store::parse_new_statements_result;

use futures::FutureExt;
use futures::future::{AbortHandle, Abortable};
use tracing::{debug, instrument, warn};
use truapi::{CallContext, v01};

const DEFAULT_SSO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);
const UNEXPECTED_SSO_SIGNING_RESPONSE: &str = "Unexpected SSO response for signing request";
const UNEXPECTED_SSO_TRANSACTION_RESPONSE: &str = "Unexpected SSO response for transaction request";

#[derive(Clone, Copy, Debug, derive_more::Display)]
enum RemoteAction {
    #[display("{_0}")]
    Signing(AuthorityRequestKind),
    #[display("account-alias")]
    AccountAlias,
    #[display("resource-allocation")]
    ResourceAllocation,
}

/// Active peer-disconnect watcher for one SSO session; aborts on drop.
pub(super) struct SsoDisconnectMonitor {
    key: SsoSessionKey,
    abort: AbortHandle,
}

impl Drop for SsoDisconnectMonitor {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl PairingHost {
    async fn submit_sign_request(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: AuthorityRequestKind,
        message: RemoteMessage,
    ) -> Result<v01::HostSignPayloadResponse, String> {
        let response = self
            .submit_remote_message(
                cx,
                session,
                RemoteAction::Signing(action),
                message,
                Some(DEFAULT_SSO_RESPONSE_TIMEOUT),
            )
            .await?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(UNEXPECTED_SSO_SIGNING_RESPONSE.to_string());
        };
        response
            .payload
            .map(|payload| v01::HostSignPayloadResponse {
                signature: payload.signature,
                signed_transaction: payload.signed_transaction,
            })
    }

    fn stop_disconnect_monitor(&self) {
        self.disconnect_monitor
            .lock()
            .expect("SSO disconnect monitor mutex poisoned")
            .take();
    }

    pub(super) fn start_disconnect_monitor(&self, session: &SessionInfo) {
        let Some(sso) = session.sso.clone() else {
            self.stop_disconnect_monitor();
            return;
        };
        let key = SsoSessionKey::from_session(&sso);

        let (registration, spawner) = {
            let mut current = self
                .disconnect_monitor
                .lock()
                .expect("SSO disconnect monitor mutex poisoned");
            if current.as_ref().is_some_and(|active| active.key == key) {
                return;
            }
            let (abort, registration) = AbortHandle::new_pair();
            *current = Some(SsoDisconnectMonitor { key, abort });
            (registration, self.spawner.clone())
        };

        let statement_store = self.statement_store.clone();
        let pairing_host = self.weak_self.clone();
        let future = async move {
            let result = wait_for_sso_peer_disconnect(statement_store, sso).await;
            let Some(pairing_host) = pairing_host.upgrade() else {
                return;
            };
            {
                let mut active = pairing_host
                    .disconnect_monitor
                    .lock()
                    .expect("SSO disconnect monitor mutex poisoned");
                if active.as_ref().is_some_and(|active| active.key == key) {
                    *active = None;
                }
            }
            match result {
                Ok(()) => {
                    pairing_host.handle_signing_host_disconnected(key).await;
                }
                Err(reason) => {
                    warn!(%reason, "SSO peer disconnect monitor stopped");
                }
            }
        };
        spawner(Box::pin(Abortable::new(future, registration).map(|_| ())));
    }

    /// Stop channel work for a cleared session: wake its in-flight waiters
    /// with a local disconnect, then drop the peer-disconnect monitor.
    pub(super) fn stop_session_channel(&self, session: Option<&SessionInfo>) {
        if let Some(sso) = session.and_then(|session| session.sso.as_ref()) {
            self.session_disconnects
                .notify(sso, SSO_LOCAL_DISCONNECT_REASON);
        }
        self.stop_disconnect_monitor();
    }

    /// Best-effort `Disconnected` notification to the SSO peer.
    #[instrument(skip_all, fields(runtime.method = "sso.disconnect.submit"))]
    pub(super) async fn submit_disconnected_message(
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
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };
        let statement = build_outgoing_request_statement(
            sso,
            message_id,
            vec![message],
            fresh_statement_expiry(),
        )?;
        self.statement_store
            .submit_fire_and_forget(statement, "SSO statement-store")
            .await
            .map_err(|err| format!("SSO statement submit failed: {err}"))?;
        Ok(())
    }

    /// Submit an SSO remote message and wait for the signing-host response.
    ///
    /// `timeout = None` is reserved for flows that block on remote user
    /// interaction, such as alias approval.
    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit", action = %action))]
    async fn submit_remote_message(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: RemoteAction,
        message: RemoteMessage,
        timeout: Option<Duration>,
    ) -> Result<SsoRemoteResponse, String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let key = SsoSessionKey::from_session(sso);
        let (_disconnect_guard, disconnect) = self.session_disconnects.subscribe(sso);
        if !session_matches_key(&self.session_state, key) {
            return Err(SSO_LOCAL_DISCONNECT_REASON.to_string());
        }
        let message_id = sso_message_id(cx, action);
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![message],
            fresh_statement_expiry(),
        )?;
        let rpc_client = self.statement_store.client("SSO statement-store").await?;
        let own_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_own)
            .await
            .map_err(|err| format!("SSO own statement-store subscribe failed: {err}"))?;
        let peer_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_peer)
            .await
            .map_err(|err| format!("SSO peer statement-store subscribe failed: {err}"))?;
        let submit_client = rpc_client.clone();
        let session_state = self.session_state.clone();
        let submit = async move {
            if !session_matches_key(&session_state, key) {
                return Err(SSO_LOCAL_DISCONNECT_REASON.to_string());
            }
            statement_store_rpc::submit(&submit_client, statement)
                .await
                .map_err(|err| format!("SSO statement submit failed: {err}"))
        }
        .boxed();
        let action = action.to_string();
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
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
        match &result {
            Ok(_) => debug!(action, %message_id, "SSO remote response received"),
            Err(reason) => warn!(action, %message_id, %reason, "SSO remote message failed"),
        }
        if matches!(&result, Err(reason) if reason == SSO_PEER_DISCONNECT_REASON) {
            self.handle_signing_host_disconnected(key).await;
        }
        result
    }

    pub(super) async fn remote_sign_payload(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let action = AuthorityRequestKind::from(&request);
        let message_id = sso_message_id(cx, RemoteAction::Signing(action));
        let request = match request {
            SignPayloadAuthorityRequest::Product(request) => request,
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => v01::HostSignPayloadRequest {
                account: product_account,
                payload: request.payload,
            },
        };
        let message = sign_payload_message(message_id, request);
        self.submit_sign_request(cx, session, action, message)
            .await
            .map_err(remote_authority_error)
    }

    pub(super) async fn remote_sign_raw(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: SignRawAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let action = AuthorityRequestKind::from(&request);
        let message_id = sso_message_id(cx, RemoteAction::Signing(action));
        let request = match request {
            SignRawAuthorityRequest::Product(request) => request,
            SignRawAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => v01::HostSignRawRequest {
                account: product_account,
                payload: request.payload,
            },
        };
        let message = sign_raw_message(message_id, request);
        let response = self
            .submit_remote_message(
                cx,
                session,
                RemoteAction::Signing(action),
                message,
                Some(DEFAULT_SSO_RESPONSE_TIMEOUT),
            )
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: UNEXPECTED_SSO_SIGNING_RESPONSE.to_string(),
            });
        };
        response
            .payload
            .map(|payload| v01::HostSignPayloadResponse {
                signature: payload.signature,
                signed_transaction: payload.signed_transaction,
            })
            .map_err(remote_authority_error)
    }

    pub(super) async fn remote_create_transaction(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        let action = AuthorityRequestKind::from(&request);
        let message_id = sso_message_id(cx, RemoteAction::Signing(action));
        let request = match request {
            CreateTransactionAuthorityRequest::Product(request) => request,
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => v01::ProductAccountTxPayload {
                signer: product_account,
                genesis_hash: request.genesis_hash,
                call_data: request.call_data,
                extensions: request.extensions,
                tx_ext_version: request.tx_ext_version,
            },
        };
        let message = create_transaction_message(message_id, request);
        let response = self
            .submit_remote_message(
                cx,
                session,
                RemoteAction::Signing(action),
                message,
                Some(DEFAULT_SSO_RESPONSE_TIMEOUT),
            )
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: UNEXPECTED_SSO_TRANSACTION_RESPONSE.to_string(),
            });
        };
        response
            .signed_transaction
            .map(|transaction| v01::HostCreateTransactionResponse { transaction })
            .map_err(remote_authority_error)
    }

    pub(super) async fn remote_account_alias(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_account_id: v01::ProductAccountId,
        requesting_product_id: String,
    ) -> Result<v01::HostAccountGetAliasResponse, AuthorityError> {
        let message_id = sso_message_id(cx, RemoteAction::AccountAlias);
        let message = alias_request_message(
            message_id.clone(),
            product_account_id,
            requesting_product_id,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::AccountAlias, message, None)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::RingVrfAlias(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: "Unexpected SSO response for account alias request".to_string(),
            });
        };
        response.payload.map_err(remote_authority_error)
    }

    pub(super) async fn remote_allocate_resources(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
        request: v01::HostRequestResourceAllocationRequest,
    ) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
        let message_id = sso_message_id(cx, RemoteAction::ResourceAllocation);
        let message = resource_allocation_message(
            message_id,
            product_id,
            request.resources,
            OnExistingAllowancePolicy::Increase,
        );
        let response = self
            .submit_remote_message(
                cx,
                session,
                RemoteAction::ResourceAllocation,
                message,
                Some(DEFAULT_SSO_RESPONSE_TIMEOUT),
            )
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: "Unexpected SSO response for resource allocation request".to_string(),
            });
        };
        response
            .payload
            .map(|outcomes| v01::HostRequestResourceAllocationResponse {
                outcomes: outcomes.into_iter().map(Into::into).collect(),
            })
            .map_err(remote_authority_error)
    }
}

/// True when the current session's SSO channel matches `key`.
pub(super) fn session_matches_key(session_state: &SessionState, key: SsoSessionKey) -> bool {
    session_state.current().as_ref().is_some_and(|current| {
        current
            .sso
            .as_ref()
            .is_some_and(|sso| SsoSessionKey::from_session(sso) == key)
    })
}

fn remote_authority_error(reason: String) -> AuthorityError {
    match reason.as_str() {
        "Rejected" | "User rejected" => AuthorityError::Rejected,
        SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => AuthorityError::Disconnected,
        _ => AuthorityError::Unknown { reason },
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.peer_disconnect.monitor"))]
async fn wait_for_sso_peer_disconnect(
    statement_store: StatementStoreRpc,
    session: SsoSessionInfo,
) -> Result<(), String> {
    let rpc_client = statement_store.client("SSO disconnect monitor").await?;
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("SSO disconnect monitor subscribe failed: {err}"))?;
    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("SSO disconnect monitor item failed: {err}"))?;
        let page = parse_new_statements_result("sso-peer-disconnect-monitor".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            if matches!(
                decode_sso_session_statement(
                    &session,
                    &statement,
                    "truapi:sso-peer-disconnect-monitor",
                    "truapi:sso-peer-disconnect-monitor",
                )?,
                Some(SsoSessionStatement::Disconnected)
            ) {
                return Ok(());
            }
        }
    }
    Err("SSO disconnect monitor response stream ended".to_string())
}

impl From<SsoAllocationOutcome> for v01::AllocationOutcome {
    fn from(outcome: SsoAllocationOutcome) -> Self {
        match outcome {
            SsoAllocationOutcome::Allocated(_) => Self::Allocated,
            SsoAllocationOutcome::Rejected => Self::Rejected,
            SsoAllocationOutcome::NotAvailable => Self::NotAvailable,
        }
    }
}
