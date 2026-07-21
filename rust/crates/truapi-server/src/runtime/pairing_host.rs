//! Pairing-host role for inter-host account authority.
//!
//! A pairing host does not own the user's signing keys. It pairs with a signing
//! host, keeps the active inter-host session, and sends authority requests to
//! that signing host over the SSO channel in [`sso_channel`].

mod sso_channel;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use futures::channel::oneshot;
use sso_channel::SsoDisconnectMonitor;

use super::allowances::{self, AllowanceCacheKey, AllowanceResource};
use super::auth_state::AuthStateMachine;
use super::authority::{
    AccountAliasAuthorityRequest, AuthorityError, AuthoritySession, BulletinAllowanceKey,
    CreateProofAuthorityRequest, CreateTransactionAuthorityRequest, ProductAuthority,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
    authority_session, require_current_session,
};
use super::connected_session_ui_info;
use super::identity::resolve_session_identity_with_chain;
use super::services::RuntimeServices;
use super::sso_pairing::{SsoPairingFlow, SsoPairingOutcome};
use super::sso_remote::{SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey};
use super::statement_store_rpc::StatementStoreRpc;
use crate::chain_runtime::ChainRuntime;
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::session::{SessionInfo, SessionState, encode_persisted_session};
use crate::host_logic::session_store::SessionStoreChangeNotifier;
use crate::host_logic::sso::messages::RingVrfError;
use crate::subscription::Spawner;

use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};
use truapi_platform::{CoreStorageKey, PairingHostConfig, Platform, ProductContext};

/// Distinguishes all remote authority request entrypoints by wire label.
#[derive(Clone, Copy, Debug, derive_more::Display)]
pub(super) enum AuthorityRequestKind {
    #[display("sign-payload")]
    SignPayload,
    #[display("sign-raw")]
    SignRaw,
    #[display("create-transaction")]
    CreateTransaction,
    #[display("legacy-sign-payload")]
    LegacySignPayload,
    #[display("legacy-sign-raw")]
    LegacySignRaw,
    #[display("legacy-create-transaction")]
    LegacyCreateTransaction,
}

impl From<&SignPayloadAuthorityRequest> for AuthorityRequestKind {
    fn from(request: &SignPayloadAuthorityRequest) -> Self {
        match request {
            SignPayloadAuthorityRequest::Product(_) => Self::SignPayload,
            SignPayloadAuthorityRequest::LegacyAccount { .. } => Self::LegacySignPayload,
        }
    }
}

impl From<&SignRawAuthorityRequest> for AuthorityRequestKind {
    fn from(request: &SignRawAuthorityRequest) -> Self {
        match request {
            SignRawAuthorityRequest::Product(_) => Self::SignRaw,
            SignRawAuthorityRequest::LegacyAccount { .. } => Self::LegacySignRaw,
        }
    }
}

impl From<&CreateTransactionAuthorityRequest> for AuthorityRequestKind {
    fn from(request: &CreateTransactionAuthorityRequest) -> Self {
        match request {
            CreateTransactionAuthorityRequest::Product(_) => Self::CreateTransaction,
            CreateTransactionAuthorityRequest::LegacyAccount { .. } => {
                Self::LegacyCreateTransaction
            }
        }
    }
}

struct LoginInFlight {
    waiters: Vec<oneshot::Sender<Result<(), String>>>,
}

struct LoginInFlightOwner<'a> {
    host: &'a PairingHost,
    active: bool,
}

impl<'a> LoginInFlightOwner<'a> {
    fn new(host: &'a PairingHost) -> Self {
        Self { host, active: true }
    }

    fn finish(&mut self, result: Result<(), String>) {
        if self.active {
            self.active = false;
            self.host.finish_login_in_flight(result);
        }
    }
}

impl Drop for LoginInFlightOwner<'_> {
    fn drop(&mut self) {
        if self.active {
            self.host
                .finish_login_in_flight(Err("login request aborted".to_string()));
        }
    }
}

/// Remote account authority for a pairing host.
pub(crate) struct PairingHost {
    pub(super) platform: Arc<dyn Platform>,
    pub(super) host_config: PairingHostConfig,
    pub(super) chain: ChainRuntime,
    /// Active inter-host session with a signing host.
    session_state: Arc<SessionState>,
    session_store_changes: Arc<SessionStoreChangeNotifier>,
    pub(super) auth_state: AuthStateMachine,
    pub(super) statement_store: StatementStoreRpc,
    session_disconnects: Arc<SessionDisconnects>,
    disconnect_monitor: Mutex<Option<SsoDisconnectMonitor>>,
    login_in_flight: Mutex<Option<LoginInFlight>>,
    login_generation: Mutex<u64>,
    statement_store_allowances: Mutex<HashMap<AllowanceCacheKey, StatementStoreAllowanceKey>>,
    bulletin_allowances: Mutex<HashMap<AllowanceCacheKey, BulletinAllowanceKey>>,
    /// Self-reference captured by the spawned disconnect-monitor task.
    weak_self: Weak<PairingHost>,
    pub(super) spawner: Spawner,
}

impl PairingHost {
    pub(crate) fn new(services: Arc<RuntimeServices>, host_config: PairingHostConfig) -> Arc<Self> {
        let platform = services.platform.clone();
        let auth_state = AuthStateMachine::new(platform.clone());
        Arc::new_cyclic(|weak_self| Self {
            platform,
            host_config,
            chain: services.chain.clone(),
            session_state: SessionState::new(),
            session_store_changes: SessionStoreChangeNotifier::new(),
            auth_state,
            statement_store: services.statement_store.clone(),
            session_disconnects: Arc::new(SessionDisconnects::default()),
            disconnect_monitor: Mutex::new(None),
            login_in_flight: Mutex::new(None),
            login_generation: Mutex::new(0),
            statement_store_allowances: Mutex::new(HashMap::new()),
            bulletin_allowances: Mutex::new(HashMap::new()),
            weak_self: weak_self.clone(),
            spawner: services.spawner.clone(),
        })
    }

    pub(crate) fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    pub(crate) fn notify_session_store_changed(&self) {
        self.session_store_changes.notify();
    }

    #[cfg(test)]
    pub(crate) fn start_session_store_sync_for_tests(self: Arc<Self>, spawner: Spawner) {
        self.start_session_store_sync(spawner);
    }

    #[cfg(test)]
    pub(crate) fn start_session_supervision_for_current_session(&self) {
        self.start_remote_monitor_for_current_session();
    }

    fn current_session(&self) -> Option<AuthoritySession> {
        self.session_state.current().as_ref().map(authority_session)
    }

    #[cfg(test)]
    pub(crate) fn start_remote_monitor_for_current_session(&self) {
        if let Some(session) = self.session_state.current() {
            self.start_disconnect_monitor(&session);
        }
    }

    #[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
    pub(crate) fn start_session_store_sync(self: Arc<Self>, spawner: Spawner) {
        let pairing_host = Arc::downgrade(&self);
        spawner(Box::pin(async move {
            let Some(current) = pairing_host.upgrade() else {
                return;
            };
            let mut ticks = current.session_store_changes.subscribe();
            drop(current);
            // Clearing the store can itself notify this subscription; clear at
            // most once per read-error streak so a persistently failing read
            // cannot spin the loop through its own clear notifications.
            let mut cleared_after_read_error = false;
            while ticks.next().await.is_some() {
                let Some(pairing_host) = pairing_host.upgrade() else {
                    break;
                };
                match pairing_host
                    .platform
                    .read_core_storage(CoreStorageKey::AuthSession)
                    .await
                {
                    Ok(Some(blob)) => {
                        cleared_after_read_error = false;
                        match crate::host_logic::session::decode_persisted_session(&blob) {
                            Ok(session) => {
                                let resolved = resolve_session_identity_with_chain(
                                    &pairing_host.chain,
                                    pairing_host.host_config.people_chain_genesis_hash,
                                    session,
                                )
                                .await;
                                if encode_persisted_session(&resolved) != blob {
                                    let _ = pairing_host
                                        .platform
                                        .write_core_storage(
                                            CoreStorageKey::AuthSession,
                                            encode_persisted_session(&resolved),
                                        )
                                        .await;
                                }
                                pairing_host.set_connected_session(resolved);
                            }
                            Err(_) => {
                                pairing_host.clear_disconnected_session(true).await;
                            }
                        }
                    }
                    Ok(None) => {
                        cleared_after_read_error = false;
                        pairing_host.clear_disconnected_session(false).await;
                    }
                    Err(_) => {
                        pairing_host.clear_disconnected_session(false).await;
                        if !cleared_after_read_error {
                            cleared_after_read_error = true;
                            let _ = pairing_host
                                .platform
                                .clear_core_storage(CoreStorageKey::AuthSession)
                                .await;
                        }
                    }
                }
            }
        }));
    }

    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %product.product_id))]
    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        let _ = product;
        if let Some(session) = self.session_state.current() {
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            return Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ));
        }

        if let Some(waiter) = self.login_waiter() {
            match waiter.await {
                Ok(Ok(())) => {
                    return Ok(HostRequestLoginResponse::V1(
                        if self.session_state.current().is_some() {
                            v01::HostRequestLoginResponse::AlreadyConnected
                        } else {
                            v01::HostRequestLoginResponse::Rejected
                        },
                    ));
                }
                Ok(Err(reason)) => {
                    return Err(CallError::Domain(HostRequestLoginError::V1(
                        v01::HostRequestLoginError::Unknown { reason },
                    )));
                }
                Err(_) => {
                    return Err(CallError::Domain(HostRequestLoginError::V1(
                        v01::HostRequestLoginError::Unknown {
                            reason: "login waiter dropped".to_string(),
                        },
                    )));
                }
            }
        }

        let mut login_owner = LoginInFlightOwner::new(self);
        let login_generation = self.begin_login_attempt();
        let outcome = match SsoPairingFlow::new(self).request_session().await {
            Ok(outcome) => outcome,
            Err(err) => {
                login_owner.finish(Err(login_error_reason(&err)));
                return Err(err);
            }
        };
        match outcome {
            SsoPairingOutcome::Cancelled => {
                login_owner.finish(Ok(()));
                if self.session_state.current().is_some() {
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::AlreadyConnected,
                    ))
                } else {
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ))
                }
            }
            SsoPairingOutcome::Success(session) => {
                if !self.is_current_login_attempt(login_generation) {
                    let _ = self
                        .platform
                        .clear_core_storage(CoreStorageKey::AuthSession)
                        .await;
                    login_owner.finish(Ok(()));
                    return Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ));
                }
                self.set_connected_session(*session);
                login_owner.finish(Ok(()));
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ))
            }
        }
    }

    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    async fn disconnect(&self) {
        self.cancel_login();
        let session = self.session_state.current();
        self.clear_disconnected_session(true).await;
        if let Some(session) = session.as_ref() {
            let _ = self.submit_disconnected_message(session).await;
        }
    }

    #[instrument(skip_all, fields(runtime.method = "account.cancel_login"))]
    pub(crate) fn cancel_login(&self) {
        self.invalidate_login_attempts();
        self.auth_state.login_cancelled();
    }

    fn begin_login_attempt(&self) -> u64 {
        let mut generation = self
            .login_generation
            .lock()
            .expect("login generation mutex poisoned");
        *generation = generation.wrapping_add(1);
        *generation
    }

    fn invalidate_login_attempts(&self) {
        let mut generation = self
            .login_generation
            .lock()
            .expect("login generation mutex poisoned");
        *generation = generation.wrapping_add(1);
    }

    fn is_current_login_attempt(&self, generation: u64) -> bool {
        *self
            .login_generation
            .lock()
            .expect("login generation mutex poisoned")
            == generation
    }

    fn login_waiter(&self) -> Option<oneshot::Receiver<Result<(), String>>> {
        let mut in_flight = self
            .login_in_flight
            .lock()
            .expect("login in-flight mutex poisoned");
        if let Some(in_flight) = in_flight.as_mut() {
            let (tx, rx) = oneshot::channel();
            in_flight.waiters.push(tx);
            Some(rx)
        } else {
            *in_flight = Some(LoginInFlight {
                waiters: Vec::new(),
            });
            None
        }
    }

    fn finish_login_in_flight(&self, result: Result<(), String>) {
        let waiters = self
            .login_in_flight
            .lock()
            .expect("login in-flight mutex poisoned")
            .take()
            .map(|in_flight| in_flight.waiters)
            .unwrap_or_default();
        for waiter in waiters {
            let _ = waiter.send(result.clone());
        }
    }

    #[instrument(skip_all, fields(runtime.method = "session_store.clear_disconnected"))]
    async fn clear_disconnected_session(&self, clear_auth_session: bool) {
        let previous = self.session_state.current();
        self.session_state.clear_session();
        self.stop_session_channel(previous.as_ref());
        if clear_auth_session {
            let _ = self
                .platform
                .clear_core_storage(CoreStorageKey::AuthSession)
                .await;
        }
        if let Some(session) = previous.as_ref() {
            let _ = allowances::clear_session_allowance_keys(&*self.platform, session).await;
        }
        self.auth_state.store_disconnected();
    }

    fn set_connected_session(&self, session: SessionInfo) {
        let previous = self.session_state.current();
        self.session_state.set_session(session.clone());
        if previous.as_ref() != Some(&session) {
            self.stop_session_channel(previous.as_ref());
        }
        self.start_disconnect_monitor(&session);
        self.auth_state
            .connected(&connected_session_ui_info(&session));
    }

    /// Single funnel for peer-initiated disconnects. Every detection source
    /// (monitor task, request-path error) must route here: it wakes in-flight
    /// waiters for `key`, then clears the session when `key` is still current,
    /// so stale notifications for replaced sessions only wake their own
    /// waiters.
    async fn handle_signing_host_disconnected(&self, key: SsoSessionKey) {
        self.session_disconnects
            .notify_key(key, SSO_PEER_DISCONNECT_REASON);
        if !self.current_sso_session_matches(key) {
            return;
        }

        self.clear_disconnected_session(true).await;
    }

    fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
        sso_channel::session_matches_key(&self.session_state, key)
    }

    fn current_private_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        require_current_session(&self.session_state, session)
    }

    async fn refresh_current_session_identity(&self) -> Option<AuthoritySession> {
        let current = self.session_state.current()?;
        if current.has_username() || self.host_config.people_chain_genesis_hash == [0; 32] {
            return Some(authority_session(&current));
        }

        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.people_chain_genesis_hash,
            current.clone(),
        )
        .await;
        if !resolved.has_username() || resolved == current {
            return self.current_session();
        }

        if !self
            .session_state
            .replace_session_if_current(&current, resolved.clone())
        {
            return self.current_session();
        }
        self.auth_state
            .connected(&connected_session_ui_info(&resolved));

        if let Err(err) = self
            .platform
            .write_core_storage(
                CoreStorageKey::AuthSession,
                encode_persisted_session(&resolved),
            )
            .await
        {
            warn!(reason = %err.reason, "refreshed session identity persist failed");
        }

        match self.session_state.current() {
            Some(live) if live != resolved => {
                if let Err(err) = self
                    .platform
                    .write_core_storage(
                        CoreStorageKey::AuthSession,
                        encode_persisted_session(&live),
                    )
                    .await
                {
                    warn!(reason = %err.reason, "live session identity persist repair failed");
                }
                Some(authority_session(&live))
            }
            None => {
                if let Err(err) = self
                    .platform
                    .clear_core_storage(CoreStorageKey::AuthSession)
                    .await
                {
                    warn!(reason = %err.reason, "cleared session identity persist repair failed");
                }
                None
            }
            _ => Some(authority_session(&resolved)),
        }
    }

    pub(super) async fn cache_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
        slot_account_key: Vec<u8>,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let allowance = StatementStoreAllowanceKey::from_secret_bytes(slot_account_key)?;
        allowances::write_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::StatementStore,
            allowance.secret.to_vec(),
        )
        .await?;
        self.remember_statement_store_allowance_key(session, product_id, allowance.clone())?;
        Ok(allowance)
    }

    fn remember_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
        allowance: StatementStoreAllowanceKey,
    ) -> Result<(), AuthorityError> {
        let cache_key =
            AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
        self.statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .insert(cache_key, allowance);
        Ok(())
    }

    pub(super) async fn cached_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
    ) -> Result<Option<StatementStoreAllowanceKey>, AuthorityError> {
        let cache_key =
            AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
        if let Some(allowance) = self
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(Some(allowance));
        }
        let Some(secret) = allowances::read_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::StatementStore,
        )
        .await?
        else {
            return Ok(None);
        };
        let allowance = StatementStoreAllowanceKey::from_secret_bytes(secret)?;
        self.remember_statement_store_allowance_key(session, product_id, allowance.clone())?;
        Ok(Some(allowance))
    }

    pub(super) async fn cache_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
        slot_account_key: Vec<u8>,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let allowance = BulletinAllowanceKey::from_secret_bytes(slot_account_key)?;
        allowances::write_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::Bulletin,
            allowance.as_secret_bytes().to_vec(),
        )
        .await?;
        self.remember_bulletin_allowance_key(session, product_id, allowance.clone())?;
        Ok(allowance)
    }

    fn remember_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
        allowance: BulletinAllowanceKey,
    ) -> Result<(), AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
        self.bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .insert(cache_key, allowance);
        Ok(())
    }

    pub(super) async fn cached_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
    ) -> Result<Option<BulletinAllowanceKey>, AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
        if let Some(allowance) = self
            .bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(Some(allowance));
        }
        let Some(secret) = allowances::read_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::Bulletin,
        )
        .await?
        else {
            return Ok(None);
        };
        let allowance = BulletinAllowanceKey::from_secret_bytes(secret)?;
        self.remember_bulletin_allowance_key(session, product_id, allowance.clone())?;
        Ok(Some(allowance))
    }

    /// Drop the cached and persisted Bulletin allowance key for one product.
    pub(super) async fn evict_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
    ) -> Result<(), AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
        self.bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .remove(&cache_key);
        allowances::remove_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::Bulletin,
        )
        .await
    }

    pub(super) fn clear_statement_store_allowance_keys(&self, session: Option<&SessionInfo>) {
        let mut allowances = self
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned");
        let Some(session) = session else {
            allowances.clear();
            return;
        };
        let Some(sso) = session.sso.as_ref() else {
            return;
        };
        let session_key = SsoSessionKey::from_session(sso);
        allowances.retain(|key, _| !key.is_for_session(session_key));
    }

    pub(super) fn clear_bulletin_allowance_keys(&self, session: Option<&SessionInfo>) {
        let mut allowances = self
            .bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned");
        let Some(session) = session else {
            allowances.clear();
            return;
        };
        let Some(sso) = session.sso.as_ref() else {
            return;
        };
        let session_key = SsoSessionKey::from_session(sso);
        allowances.retain(|key, _| !key.is_for_session(session_key));
    }

    async fn sign_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_sign_payload(cx, &session, request).await
    }

    async fn sign_raw(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: SignRawAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_sign_raw(cx, &session, request).await
    }

    async fn create_transaction(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_create_transaction(cx, &session, request).await
    }

    async fn account_alias(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: AccountAliasAuthorityRequest,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        let session = self.current_private_session(session)?;
        self.remote_account_alias(cx, &session, request).await
    }

    async fn create_proof(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: CreateProofAuthorityRequest,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        let session = self.current_private_session(session)?;
        self.remote_create_proof(cx, &session, request).await
    }

    async fn allocate_resources(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: v01::HostRequestResourceAllocationRequest,
    ) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_allocate_resources(cx, &session, product_id, request)
            .await
    }

    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_statement_store_allowance_key(cx, &session, product_id)
            .await
    }

    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_bulletin_allowance_key(cx, &session, product_id)
            .await
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_refresh_bulletin_allowance_key(cx, &session, product_id)
            .await
    }

    async fn sign_statement_store_product_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        _account: v01::ProductAccountId,
        _payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        self.current_private_session(session)?;
        Err(AuthorityError::Unavailable {
            reason: "pairing host: exact statement proof signing is not supported over the \
                     current SSO raw-signing protocol"
                .to_string(),
        })
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        let session = self.current_private_session(session)?;
        if session.sso.is_none() {
            return Err(AuthorityError::Disconnected);
        }
        let root_entropy_source =
            session
                .root_entropy_source
                .ok_or_else(|| AuthorityError::Unavailable {
                    reason: "Session secret missing".to_string(),
                })?;
        derive_product_entropy_from_source(&root_entropy_source, product_id, context).map_err(
            |err| AuthorityError::Unknown {
                reason: err.to_string(),
            },
        )
    }
}

fn login_error_reason(err: &CallError<HostRequestLoginError>) -> String {
    match err {
        CallError::Domain(HostRequestLoginError::V1(v01::HostRequestLoginError::Unknown {
            reason,
        }))
        | CallError::HostFailure { reason } => reason.clone(),
        CallError::Unsupported => "login unsupported".to_string(),
        CallError::Denied => "login denied".to_string(),
        CallError::MalformedFrame { reason } => reason.clone(),
    }
}

#[async_trait::async_trait]
impl ProductAuthority for PairingHost {
    fn current_session(&self) -> Option<AuthoritySession> {
        PairingHost::current_session(self)
    }

    fn session_state(&self) -> Arc<SessionState> {
        PairingHost::session_state(self)
    }

    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        PairingHost::request_login(self, product).await
    }

    async fn disconnect(&self) {
        PairingHost::disconnect(self).await;
    }

    async fn refresh_session_identity(&self) -> Option<AuthoritySession> {
        self.refresh_current_session_identity().await
    }

    async fn sign_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        PairingHost::sign_payload(self, cx, session, request).await
    }

    async fn sign_raw(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: SignRawAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        PairingHost::sign_raw(self, cx, session, request).await
    }

    async fn create_transaction(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        PairingHost::create_transaction(self, cx, session, request).await
    }

    async fn account_alias(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: AccountAliasAuthorityRequest,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        PairingHost::account_alias(self, cx, session, request).await
    }

    async fn create_proof(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: CreateProofAuthorityRequest,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        PairingHost::create_proof(self, cx, session, request).await
    }

    async fn allocate_resources(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: v01::HostRequestResourceAllocationRequest,
    ) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
        PairingHost::allocate_resources(self, cx, session, product_id, request).await
    }

    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        PairingHost::statement_store_allowance_key(self, cx, session, product_id).await
    }

    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        PairingHost::bulletin_allowance_key(self, cx, session, product_id).await
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        PairingHost::refresh_bulletin_allowance_key(self, cx, session, product_id).await
    }

    async fn sign_statement_store_product_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        account: v01::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        PairingHost::sign_statement_store_product_payload(self, cx, session, account, payload).await
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        PairingHost::derive_entropy(self, session, product_id, context)
    }
}
