//! SSO statement-store channel to the paired remote signing host.

use super::super::authority::{
    AccountAliasAuthorityRequest, AuthorityCancelError, AuthorityError, BulletinAllowanceKey,
    CreateProofAuthorityRequest, CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest,
    SignRawAuthorityRequest, StatementStoreAllowanceKey,
};
use super::super::sso_remote::{
    RemoteResponseWait, SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON,
    SsoRemoteResponseError, SsoSessionKey, fresh_statement_expiry, sso_message_id,
    statement_subscription_stream, subscribe_statement_topic, wait_for_sso_remote_response,
};
use super::super::statement_store_rpc::{self, StatementStoreRpc};
use super::AuthorityRequestKind;
use super::PairingHost;
use crate::host_logic::session::{SessionInfo, SessionState, SsoSessionInfo};
use crate::host_logic::sso::messages::{
    OnExistingAllowancePolicy, RemoteMessage, RemoteMessageData, RingVrfError,
    SsoAllocatedResource, SsoAllocationOutcome, SsoRemoteResponse, SsoSessionStatement,
    alias_request_message, build_outgoing_request_statement, create_transaction_legacy_message,
    create_transaction_message, decode_sso_session_statement, proof_request_message,
    resource_allocation_message, sign_payload_message, sign_raw_legacy_message, sign_raw_message,
    v1,
};
use crate::host_logic::statement_store::parse_new_statements_result;

use futures::FutureExt;
use futures::future::{AbortHandle, Abortable};
use tracing::{debug, instrument, warn};
use truapi::{CallContext, latest};

const UNEXPECTED_SSO_SIGNING_RESPONSE: &str = "Unexpected SSO response for signing request";
const UNEXPECTED_SSO_TRANSACTION_RESPONSE: &str = "Unexpected SSO response for transaction request";
const UNEXPECTED_SSO_ALIAS_RESPONSE: &str = "Unexpected SSO response for account alias request";
const UNEXPECTED_SSO_PROOF_RESPONSE: &str = "Unexpected SSO response for ring-VRF proof request";

#[derive(Clone, Copy, Debug, derive_more::Display)]
enum RemoteAction {
    #[display("{_0}")]
    Signing(AuthorityRequestKind),
    #[display("account-alias")]
    RingVrfAlias,
    #[display("ring-vrf-proof")]
    RingVrfProof,
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
    ) -> Result<latest::HostSignPayloadResponse, SsoRemoteResponseError> {
        let response = self
            .submit_remote_message(cx, session, RemoteAction::Signing(action), message)
            .await?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(SsoRemoteResponseError::Failure(
                UNEXPECTED_SSO_SIGNING_RESPONSE.to_string(),
            ));
        };
        response
            .payload
            .map(|payload| latest::HostSignPayloadResponse {
                signature: payload.signature,
                signed_transaction: payload.signed_transaction,
            })
            .map_err(SsoRemoteResponseError::Failure)
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
        self.clear_statement_store_allowance_keys(session);
        self.clear_bulletin_allowance_keys(session);
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
    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit", action = %action))]
    async fn submit_remote_message(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: RemoteAction,
        message: RemoteMessage,
    ) -> Result<SsoRemoteResponse, SsoRemoteResponseError> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| SsoRemoteResponseError::Failure("No SSO session state".to_string()))?;
        let key = SsoSessionKey::from_session(sso);
        let (_disconnect_guard, disconnect) = self.session_disconnects.subscribe(sso);
        if !session_matches_key(&self.session_state, key) {
            return Err(SsoRemoteResponseError::LocalDisconnected);
        }
        let message_id = message.message_id.clone();
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![message],
            fresh_statement_expiry(),
        )
        .map_err(SsoRemoteResponseError::Failure)?;
        let rpc_client = self.statement_store.client("SSO statement-store").await?;
        let own_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_own)
            .await
            .map_err(|err| {
                SsoRemoteResponseError::Failure(format!(
                    "SSO own statement-store subscribe failed: {err}"
                ))
            })?;
        let peer_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_peer)
            .await
            .map_err(|err| {
                SsoRemoteResponseError::Failure(format!(
                    "SSO peer statement-store subscribe failed: {err}"
                ))
            })?;
        let submit_client = rpc_client.clone();
        let session_state = self.session_state.clone();
        let submit = async move {
            if !session_matches_key(&session_state, key) {
                return Err(SsoRemoteResponseError::LocalDisconnected);
            }
            statement_store_rpc::submit_sso(&submit_client, statement, "pairing-host request")
                .await
                .map_err(|err| {
                    SsoRemoteResponseError::Failure(format!("SSO statement submit failed: {err}"))
                })
        }
        .boxed();
        let action = action.to_string();
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
        let result = wait_for_sso_remote_response(RemoteResponseWait {
            own_statements: statement_subscription_stream(own_subscription, "own"),
            peer_statements: statement_subscription_stream(peer_subscription, "peer"),
            submit,
            session: sso,
            statement_request_id: &message_id,
            remote_message_id: &message_id,
            cancel: cx.cancel(),
            disconnect: Some(disconnect),
        })
        .await;
        let result = result.map_err(|reason| match reason {
            SsoRemoteResponseError::Cancelled(err) if !cx.request_id().is_empty() => {
                SsoRemoteResponseError::Cancelled(err.with_remote_message_id(cx.request_id()))
            }
            reason => reason,
        });
        match &result {
            Ok(_) => debug!(action, %message_id, "SSO remote response received"),
            Err(reason) => warn!(action, %message_id, %reason, "SSO remote message failed"),
        }
        if matches!(&result, Err(SsoRemoteResponseError::PeerDisconnected)) {
            self.handle_signing_host_disconnected(key).await;
        }
        result
    }

    pub(super) async fn remote_sign_payload(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: SignPayloadAuthorityRequest,
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        let action = AuthorityRequestKind::from(&request);
        let message_id = sso_message_id();
        let request = match request {
            SignPayloadAuthorityRequest::Product(request) => request,
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => latest::HostSignPayloadRequest {
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
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        let action = AuthorityRequestKind::from(&request);
        let message_id = sso_message_id();
        let (message, expects_legacy_response) = match request {
            SignRawAuthorityRequest::Product(request) => {
                (sign_raw_message(message_id, request), false)
            }
            SignRawAuthorityRequest::LegacyAccount { account, request } => (
                sign_raw_legacy_message(message_id, account, request.payload),
                true,
            ),
        };
        let response = self
            .submit_remote_message(cx, session, RemoteAction::Signing(action), message)
            .await
            .map_err(remote_authority_error)?;
        match (expects_legacy_response, response) {
            (false, SsoRemoteResponse::Sign(response)) => response
                .payload
                .map(|payload| latest::HostSignPayloadResponse {
                    signature: payload.signature,
                    signed_transaction: payload.signed_transaction,
                })
                .map_err(remote_authority_error),
            (true, SsoRemoteResponse::SignRawLegacy(response)) => response
                .signature
                .map(|signature| latest::HostSignPayloadResponse {
                    signature,
                    signed_transaction: None,
                })
                .map_err(remote_authority_error),
            _ => Err(AuthorityError::Unknown {
                reason: UNEXPECTED_SSO_SIGNING_RESPONSE.to_string(),
            }),
        }
    }

    pub(super) async fn remote_create_transaction(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
        let action = AuthorityRequestKind::from(&request);
        let message_id = sso_message_id();
        let message = match request {
            CreateTransactionAuthorityRequest::Product(request) => {
                create_transaction_message(message_id, request)
            }
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => create_transaction_message(
                message_id,
                latest::ProductAccountTxPayload {
                    signer: product_account,
                    genesis_hash: request.genesis_hash,
                    call_data: request.call_data,
                    extensions: request.extensions,
                    tx_ext_version: request.tx_ext_version,
                },
            ),
            CreateTransactionAuthorityRequest::IdentityAccount(request) => {
                create_transaction_legacy_message(message_id, request)
            }
        };
        let response = self
            .submit_remote_message(cx, session, RemoteAction::Signing(action), message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: UNEXPECTED_SSO_TRANSACTION_RESPONSE.to_string(),
            });
        };
        response
            .signed_transaction
            .map(|transaction| latest::HostCreateTransactionResponse { transaction })
            .map_err(remote_authority_error)
    }

    pub(super) async fn remote_account_alias(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: AccountAliasAuthorityRequest,
    ) -> Result<latest::HostAccountGetAliasResponse, RingVrfError> {
        let message_id = sso_message_id();
        let message = alias_request_message(
            message_id,
            request.calling_product_id,
            request.context,
            request.ring_location,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::RingVrfAlias, message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::RingVrfAlias(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: UNEXPECTED_SSO_ALIAS_RESPONSE.to_string(),
            });
        };
        response.payload
    }

    pub(super) async fn remote_create_proof(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: CreateProofAuthorityRequest,
    ) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
        let message_id = sso_message_id();
        let message = proof_request_message(
            message_id,
            request.calling_product_id,
            request.context,
            request.ring_location,
            request.message,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::RingVrfProof, message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::RingVrfProof(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: UNEXPECTED_SSO_PROOF_RESPONSE.to_string(),
            });
        };
        response.payload
    }

    pub(super) async fn remote_allocate_resources(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
        request: latest::HostRequestResourceAllocationRequest,
    ) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
        let message_id = sso_message_id();
        let message = resource_allocation_message(
            message_id,
            product_id.clone(),
            request.resources,
            OnExistingAllowancePolicy::Increase,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::ResourceAllocation, message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: "Unexpected SSO response for resource allocation request".to_string(),
            });
        };
        let outcomes = response.payload.map_err(remote_authority_error)?;
        self.cache_allowance_outcomes(session, &product_id, &outcomes)
            .await?;
        Ok(latest::HostRequestResourceAllocationResponse {
            outcomes: outcomes.into_iter().map(Into::into).collect(),
        })
    }

    pub(super) async fn remote_statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        if let Some(cached) = self
            .cached_statement_store_allowance_key(session, &product_id)
            .await?
        {
            return Ok(cached);
        }

        let message_id = sso_message_id();
        let message = resource_allocation_message(
            message_id,
            product_id.clone(),
            vec![latest::AllocatableResource::StatementStoreAllowance],
            OnExistingAllowancePolicy::Ignore,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::ResourceAllocation, message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: "Unexpected SSO response for statement-store allowance request".to_string(),
            });
        };
        let mut outcomes = response
            .payload
            .map_err(remote_authority_error)?
            .into_iter();
        let outcome = outcomes.next().ok_or_else(|| AuthorityError::Unknown {
            reason: "Empty statement-store allowance response".to_string(),
        })?;
        match outcome {
            SsoAllocationOutcome::Allocated(SsoAllocatedResource::StatementStoreAllowance {
                slot_account_key,
            }) => {
                self.cache_statement_store_allowance_key(session, &product_id, slot_account_key)
                    .await
            }
            SsoAllocationOutcome::Allocated(other) => Err(AuthorityError::Unknown {
                reason: format!(
                    "Unexpected statement-store allowance response resource: {other:?}"
                ),
            }),
            SsoAllocationOutcome::Rejected => Err(AuthorityError::Rejected),
            SsoAllocationOutcome::NotAvailable => Err(AuthorityError::Unavailable {
                reason: "statement-store allowance is not available".to_string(),
            }),
        }
    }

    pub(super) async fn remote_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        if let Some(cached) = self
            .cached_bulletin_allowance_key(session, &product_id)
            .await?
        {
            return Ok(cached);
        }

        let message_id = sso_message_id();
        let message = resource_allocation_message(
            message_id,
            product_id.clone(),
            vec![latest::AllocatableResource::BulletinAllowance],
            OnExistingAllowancePolicy::Ignore,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::ResourceAllocation, message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: "Unexpected SSO response for bulletin allowance request".to_string(),
            });
        };
        let mut outcomes = response
            .payload
            .map_err(remote_authority_error)?
            .into_iter();
        let outcome = outcomes.next().ok_or_else(|| AuthorityError::Unknown {
            reason: "Empty bulletin allowance response".to_string(),
        })?;
        match outcome {
            SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
                slot_account_key,
            }) => {
                self.cache_bulletin_allowance_key(session, &product_id, slot_account_key)
                    .await
            }
            SsoAllocationOutcome::Allocated(other) => Err(AuthorityError::Unknown {
                reason: format!("Unexpected bulletin allowance response resource: {other:?}"),
            }),
            SsoAllocationOutcome::Rejected => Err(AuthorityError::Rejected),
            SsoAllocationOutcome::NotAvailable => Err(AuthorityError::Unavailable {
                reason: "bulletin allowance is not available".to_string(),
            }),
        }
    }

    pub(super) async fn remote_refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        // Drop the cached (and persisted) key so a stale/exhausted slot is not
        // reused, then request a fresh allocation with `Increase` so the
        // wallet grants a new allowance rather than echoing the old slot.
        self.evict_bulletin_allowance_key(session, &product_id)
            .await?;

        let message_id = sso_message_id();
        let message = resource_allocation_message(
            message_id,
            product_id.clone(),
            vec![latest::AllocatableResource::BulletinAllowance],
            OnExistingAllowancePolicy::Increase,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::ResourceAllocation, message)
            .await
            .map_err(remote_authority_error)?;
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: "Unexpected SSO response for bulletin allowance refresh".to_string(),
            });
        };
        let mut outcomes = response
            .payload
            .map_err(remote_authority_error)?
            .into_iter();
        let outcome = outcomes.next().ok_or_else(|| AuthorityError::Unknown {
            reason: "Empty bulletin allowance refresh response".to_string(),
        })?;
        match outcome {
            SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
                slot_account_key,
            }) => {
                self.cache_bulletin_allowance_key(session, &product_id, slot_account_key)
                    .await
            }
            SsoAllocationOutcome::Allocated(other) => Err(AuthorityError::Unknown {
                reason: format!("Unexpected bulletin allowance refresh resource: {other:?}"),
            }),
            SsoAllocationOutcome::Rejected => Err(AuthorityError::Rejected),
            SsoAllocationOutcome::NotAvailable => Err(AuthorityError::Unavailable {
                reason: "bulletin allowance is not available".to_string(),
            }),
        }
    }

    async fn cache_allowance_outcomes(
        &self,
        session: &SessionInfo,
        product_id: &str,
        outcomes: &[SsoAllocationOutcome],
    ) -> Result<(), AuthorityError> {
        for outcome in outcomes {
            if let SsoAllocationOutcome::Allocated(resource) = outcome {
                match resource {
                    SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
                        self.cache_statement_store_allowance_key(
                            session,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                        self.cache_bulletin_allowance_key(
                            session,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::SmartContractAllowance
                    | SsoAllocatedResource::AutoSigning { .. } => {}
                }
            }
        }
        Ok(())
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

fn remote_authority_error(reason: impl Into<SsoRemoteResponseError>) -> AuthorityError {
    match reason.into() {
        SsoRemoteResponseError::Cancelled(err) => AuthorityError::Cancelled(
            AuthorityCancelError::new(err.remote_message_id(), err.reason()),
        ),
        SsoRemoteResponseError::LocalDisconnected | SsoRemoteResponseError::PeerDisconnected => {
            AuthorityError::Disconnected
        }
        SsoRemoteResponseError::Failure(reason) => match reason.as_str() {
            "Rejected" | "User rejected" => AuthorityError::Rejected,
            SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => {
                AuthorityError::Disconnected
            }
            _ => AuthorityError::Unknown { reason },
        },
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

impl From<SsoAllocationOutcome> for latest::AllocationOutcome {
    fn from(outcome: SsoAllocationOutcome) -> Self {
        match outcome {
            SsoAllocationOutcome::Allocated(_) => Self::Allocated,
            SsoAllocationOutcome::Rejected => Self::Rejected,
            SsoAllocationOutcome::NotAvailable => Self::NotAvailable,
        }
    }
}
