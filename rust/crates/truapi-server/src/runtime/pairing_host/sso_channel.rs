//! SSO statement-store channel to the paired remote signing host.

use super::super::authority::{
    AccountAliasAuthorityRequest, AuthorityCancelError, AuthorityError, BulletinAllowanceKey,
    CreateProofAuthorityRequest, CreateTransactionAuthorityRequest,
    ListRingVrfKeysAuthorityRequest, RegisterRingVrfKeyAuthorityRequest,
    RingVrfSignAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey,
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
    create_transaction_message, decode_sso_session_statement, list_ring_vrf_keys_message,
    product_subtree_request_message, proof_request_message, register_ring_vrf_key_message,
    resource_allocation_message, ring_vrf_sign_message, sign_payload_message,
    sign_raw_legacy_message, sign_raw_message, sign_vrf_message, v1,
};
use crate::host_logic::statement_store::parse_new_statements_result;

use futures::FutureExt;
use futures::future::{AbortHandle, Abortable};
use tracing::{debug, instrument, warn};
use truapi::{CallContext, latest, v01};

const UNEXPECTED_SSO_SIGNING_RESPONSE: &str = "Unexpected SSO response for signing request";
const UNEXPECTED_SSO_TRANSACTION_RESPONSE: &str = "Unexpected SSO response for transaction request";
const UNEXPECTED_SSO_ALIAS_RESPONSE: &str = "Unexpected SSO response for account alias request";
const UNEXPECTED_SSO_PROOF_RESPONSE: &str = "Unexpected SSO response for ring-VRF proof request";
const UNEXPECTED_SSO_REGISTER_RING_VRF_KEY_RESPONSE: &str =
    "Unexpected SSO response for ring-VRF key registration request";
const UNEXPECTED_SSO_LIST_RING_VRF_KEYS_RESPONSE: &str =
    "Unexpected SSO response for ring-VRF key listing request";
const UNEXPECTED_SSO_RING_VRF_SIGN_RESPONSE: &str =
    "Unexpected SSO response for ring-VRF signing request";

fn unexpected_response_reason(context: &str, response_kind: &str) -> String {
    format!("{context}: {response_kind}")
}

#[derive(Clone, Copy, Debug, derive_more::Display)]
enum RemoteAction {
    #[display("{_0}")]
    Signing(AuthorityRequestKind),
    #[display("account-alias")]
    RingVrfAlias,
    #[display("ring-vrf-proof")]
    RingVrfProof,
    #[display("register-ring-vrf-key")]
    RegisterRingVrfKey,
    #[display("list-ring-vrf-keys")]
    ListRingVrfKeys,
    #[display("ring-vrf-sign")]
    RingVrfSign,
    #[display("sign-vrf")]
    SignVrf,
    #[display("resource-allocation")]
    ResourceAllocation,
    #[display("product-subtree")]
    ProductSubtree,
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
        let response_kind = response.kind();
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(SsoRemoteResponseError::Failure(unexpected_response_reason(
                UNEXPECTED_SSO_SIGNING_RESPONSE,
                response_kind,
            )));
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

    /// Watch the session's topics for a peer disconnect statement, replacing
    /// any monitor for a different session. No-op when one is already running
    /// for this session.
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
        self.clear_product_subtrees(session);
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
        let rpc_client = self
            .statement_store
            .client("SSO statement-store")
            .await
            .map_err(|err| SsoRemoteResponseError::Failure(err.to_string()))?;
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

    /// Fetch and cache a product's hard-subtree public key from the Account Holder.
    pub(super) async fn remote_product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let sso = session.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        let cache_key = (SsoSessionKey::from_session(sso), product_id.clone());
        if let Some(public_key) = self
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .get(&cache_key)
            .copied()
        {
            return Ok(public_key);
        }

        let message_id = sso_message_id();
        let message = product_subtree_request_message(message_id, product_id);
        let response = self
            .submit_remote_message(cx, session, RemoteAction::ProductSubtree, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::ProductSubtree(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected SSO response for product subtree request",
                    response_kind,
                ),
            });
        };
        let public_key = response
            .product_public_key
            .map_err(remote_authority_error)?;
        if !self.cache_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key) {
            return Err(AuthorityError::Disconnected);
        }
        Ok(public_key)
    }

    /// Forward RFC-0023 VRF signing to the paired Account Holder.
    pub(super) async fn remote_sign_vrf(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        calling_product_id: String,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        let message_id = sso_message_id();
        let message = sign_vrf_message(message_id, calling_product_id, request);
        let response = self
            .submit_remote_message(cx, session, RemoteAction::SignVrf, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::SignVrf(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected SSO response for VRF signing request",
                    response_kind,
                ),
            });
        };
        response.payload.map_err(|err| match err {
            v01::HostAccountSignVrfError::NotConnected => AuthorityError::Disconnected,
            v01::HostAccountSignVrfError::Rejected => AuthorityError::Rejected,
            v01::HostAccountSignVrfError::Unknown { reason } => AuthorityError::Unknown { reason },
        })
    }

    /// Forward a payload-signing request to the paired signing host.
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

    /// Forward a raw-signing request to the paired signing host.
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
        let response_kind = response.kind();
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
                reason: unexpected_response_reason(UNEXPECTED_SSO_SIGNING_RESPONSE, response_kind),
            }),
        }
    }

    /// Forward a transaction-creation request to the paired signing host.
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
        let response_kind = response.kind();
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    UNEXPECTED_SSO_TRANSACTION_RESPONSE,
                    response_kind,
                ),
            });
        };
        response
            .signed_transaction
            .map(|transaction| latest::HostCreateTransactionResponse { transaction })
            .map_err(remote_authority_error)
    }

    /// Forward a contextual-alias request to the paired signing host.
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
            request.key_handle,
            request.context,
            request.ring_location,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::RingVrfAlias, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::RingVrfAlias(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: unexpected_response_reason(UNEXPECTED_SSO_ALIAS_RESPONSE, response_kind),
            });
        };
        response.payload
    }

    /// Forward a ring-VRF proof request to the paired signing host.
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
            request.key_handle,
            request.context,
            request.ring_location,
            request.message,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::RingVrfProof, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::RingVrfProof(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: unexpected_response_reason(UNEXPECTED_SSO_PROOF_RESPONSE, response_kind),
            });
        };
        response.payload
    }

    /// Forward a ring-VRF key registration request to the paired signing host.
    pub(super) async fn remote_register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: RegisterRingVrfKeyAuthorityRequest,
    ) -> Result<latest::RingVrfPublicKey, RingVrfError> {
        let message_id = sso_message_id();
        let message = register_ring_vrf_key_message(
            message_id,
            request.calling_product_id,
            request.index,
            request.ring,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::RegisterRingVrfKey, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::RegisterRingVrfKey(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: unexpected_response_reason(
                    UNEXPECTED_SSO_REGISTER_RING_VRF_KEY_RESPONSE,
                    response_kind,
                ),
            });
        };
        response.payload
    }

    /// Forward a ring-VRF key listing request to the paired signing host.
    pub(super) async fn remote_list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ListRingVrfKeysAuthorityRequest,
    ) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
        let message_id = sso_message_id();
        let message = list_ring_vrf_keys_message(
            message_id,
            request.calling_product_id,
            request.owner,
            request.disclosure,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::ListRingVrfKeys, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::ListRingVrfKeys(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: unexpected_response_reason(
                    UNEXPECTED_SSO_LIST_RING_VRF_KEYS_RESPONSE,
                    response_kind,
                ),
            });
        };
        response.payload
    }

    /// Forward a direct ring-VRF signing request to the paired signing host.
    pub(super) async fn remote_ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: RingVrfSignAuthorityRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        let message_id = sso_message_id();
        let message = ring_vrf_sign_message(
            message_id,
            request.calling_product_id,
            request.key_handle,
            request.message,
        );
        let response = self
            .submit_remote_message(cx, session, RemoteAction::RingVrfSign, message)
            .await
            .map_err(remote_authority_error)?;
        let response_kind = response.kind();
        let SsoRemoteResponse::RingVrfSign(response) = response else {
            return Err(RingVrfError::Unknown {
                reason: unexpected_response_reason(
                    UNEXPECTED_SSO_RING_VRF_SIGN_RESPONSE,
                    response_kind,
                ),
            });
        };
        response.payload
    }

    /// Ask the paired signing host to allocate product resources, caching any
    /// returned allowance keys.
    pub(super) async fn remote_allocate_resources(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
        request: latest::HostRequestResourceAllocationRequest,
    ) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
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
        let response_kind = response.kind();
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected SSO response for resource allocation request",
                    response_kind,
                ),
            });
        };
        let outcomes = response.payload.map_err(remote_authority_error)?;
        self.cache_allowance_outcomes(cx, session, lifecycle_epoch, &product_id, &outcomes)
            .await?;
        Ok(latest::HostRequestResourceAllocationResponse {
            outcomes: outcomes.into_iter().map(Into::into).collect(),
        })
    }

    /// Statement-store allowance key for the product, served from the cache
    /// or allocated by the paired signing host.
    pub(super) async fn remote_statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        if let Some(cached) = self
            .cached_statement_store_allowance_key(session, lifecycle_epoch, &product_id)
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
        let response_kind = response.kind();
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected SSO response for statement-store allowance request",
                    response_kind,
                ),
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
                self.cache_statement_store_allowance_key(
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            SsoAllocationOutcome::Allocated(other) => Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected statement-store allowance response resource",
                    other.kind(),
                ),
            }),
            SsoAllocationOutcome::Rejected => Err(AuthorityError::Rejected),
            SsoAllocationOutcome::NotAvailable => Err(AuthorityError::Unavailable {
                reason: "statement-store allowance is not available".to_string(),
            }),
        }
    }

    /// Bulletin allowance key for the product, served from the cache or
    /// allocated by the paired signing host.
    pub(super) async fn remote_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        if let Some(cached) = self
            .cached_bulletin_allowance_key(session, lifecycle_epoch, &product_id)
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
        let response_kind = response.kind();
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected SSO response for bulletin allowance request",
                    response_kind,
                ),
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
                self.cache_bulletin_allowance_key(
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            SsoAllocationOutcome::Allocated(other) => Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected bulletin allowance response resource",
                    other.kind(),
                ),
            }),
            SsoAllocationOutcome::Rejected => Err(AuthorityError::Rejected),
            SsoAllocationOutcome::NotAvailable => Err(AuthorityError::Unavailable {
                reason: "bulletin allowance is not available".to_string(),
            }),
        }
    }

    /// Evict the cached Bulletin allowance key and allocate a fresh one with
    /// an increased allowance.
    pub(super) async fn remote_refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        // Drop the cached (and persisted) key so a stale/exhausted slot is not
        // reused, then request a fresh allocation with `Increase` so the
        // wallet grants a new allowance rather than echoing the old slot.
        self.evict_bulletin_allowance_key(session, lifecycle_epoch, &product_id)
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
        let response_kind = response.kind();
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected SSO response for bulletin allowance refresh",
                    response_kind,
                ),
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
                self.cache_bulletin_allowance_key(
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            SsoAllocationOutcome::Allocated(other) => Err(AuthorityError::Unknown {
                reason: unexpected_response_reason(
                    "Unexpected bulletin allowance refresh resource",
                    other.kind(),
                ),
            }),
            SsoAllocationOutcome::Rejected => Err(AuthorityError::Rejected),
            SsoAllocationOutcome::NotAvailable => Err(AuthorityError::Unavailable {
                reason: "bulletin allowance is not available".to_string(),
            }),
        }
    }

    async fn cache_allowance_outcomes(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        outcomes: &[SsoAllocationOutcome],
    ) -> Result<(), AuthorityError> {
        for outcome in outcomes {
            if let SsoAllocationOutcome::Allocated(resource) = outcome {
                match resource {
                    SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
                        self.cache_statement_store_allowance_key(
                            session,
                            lifecycle_epoch,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                        self.cache_bulletin_allowance_key(
                            session,
                            lifecycle_epoch,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::SmartContractAllowance => {}
                    SsoAllocatedResource::AutoSigning {
                        product_root_private_key,
                        ring_vrf_domain_entropy,
                    } => {
                        let expected_product_subtree_public_key = self
                            .remote_product_subtree_public_key(cx, session, product_id.to_string())
                            .await?;
                        self.remember_auto_signing_key(
                            session,
                            lifecycle_epoch,
                            product_id,
                            expected_product_subtree_public_key,
                            *product_root_private_key,
                            *ring_vrf_domain_entropy,
                        )
                        .await?;
                    }
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
    let rpc_client = statement_store
        .client("SSO disconnect monitor")
        .await
        .map_err(|err| err.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::sso::messages::ResourceAllocationResponse;

    #[test]
    fn unexpected_response_reasons_include_only_safe_discriminants() {
        let private_key = [0xA5; 64];
        let resource = SsoAllocatedResource::AutoSigning {
            product_root_private_key: private_key,
            ring_vrf_domain_entropy: [0x5A; 32],
        };
        let resource_reason = unexpected_response_reason(
            "Unexpected statement-store allowance response resource",
            resource.kind(),
        );
        assert_eq!(
            resource_reason,
            "Unexpected statement-store allowance response resource: auto-signing"
        );
        assert!(!resource_reason.contains("165, 165"));

        let response = SsoRemoteResponse::ResourceAllocation(ResourceAllocationResponse {
            responding_to: "secret-test".to_string(),
            payload: Ok(vec![SsoAllocationOutcome::Allocated(resource)]),
        });
        let response_reason =
            unexpected_response_reason(UNEXPECTED_SSO_SIGNING_RESPONSE, response.kind());
        assert_eq!(
            response_reason,
            "Unexpected SSO response for signing request: resource-allocation"
        );
        assert!(!response_reason.contains("165, 165"));
    }
}
