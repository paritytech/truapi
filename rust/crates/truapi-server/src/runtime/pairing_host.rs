//! Pairing-host role for inter-host account authority.
//!
//! A pairing host does not own the user's signing keys. It pairs with a signing
//! host, keeps the active inter-host session, and sends authority requests to
//! that signing host over the SSO channel in [`sso_channel`].

mod sso_channel;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use futures::channel::oneshot;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::SecretKey;
use sso_channel::SsoDisconnectMonitor;
use zeroize::Zeroize;

use super::allowances::{self, AllowanceCacheKey, AllowanceResource};
use super::auth_state::AuthStateMachine;
use super::authority::{
    AccountAliasAuthorityRequest, AuthorityError, AuthoritySession, AutoSigningKey,
    BulletinAllowanceKey, CreateProofAuthorityRequest, CreateTransactionAuthorityRequest,
    ListRingVrfKeysAuthorityRequest, ProductAuthority, RegisterRingVrfKeyAuthorityRequest,
    RingVrfSignAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey, authority_session, require_current_session,
};
use super::connected_session_ui_info;
use super::identity::resolve_session_identity_with_chain;
use super::services::RuntimeServices;
use super::sso_pairing::{SsoPairingFlow, SsoPairingOutcome};
use super::sso_remote::{
    SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey, sso_message_id,
};
use super::statement_store_rpc::StatementStoreRpc;
use crate::chain_runtime::ChainRuntime;
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::product_account::{
    derivation_index_bytes, derive_product_keypair_from_subtree_secret,
    derive_ring_vrf_entropy_from_domain,
};
use crate::host_logic::session::{SessionInfo, SessionState, encode_persisted_session};
use crate::host_logic::session_store::SessionStoreChangeNotifier;
use crate::host_logic::sso::messages::RingVrfError;
use crate::subscription::Spawner;

use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};
use truapi_platform::{
    CoreStorageKey, PairingHostConfig, Platform, ProductContext, SignVrfReview,
    UserConfirmationReview, normalize_product_identifier,
};
use zeroize::Zeroizing;

use super::ring_vrf_registry::{RingVrfRegistryStore, validate_owner_listing};
use super::signing_host::ring_vrf::{
    ChainRingResolver, MemberCandidate, RingResolver, alias_from_entropy, context_bytes,
    create_proof, member_from_entropy, sign_from_entropy,
};

/// Distinguishes all remote authority request entrypoints by wire label.
#[derive(Clone, Copy, Debug, derive_more::Display)]
pub(super) enum AuthorityRequestKind {
    /// `sign_payload` with a product account.
    #[display("sign-payload")]
    SignPayload,
    /// `sign_raw` with a product account.
    #[display("sign-raw")]
    SignRaw,
    /// `create_transaction` with a product account.
    #[display("create-transaction")]
    CreateTransaction,
    /// `sign_payload` through the legacy-account API.
    #[display("legacy-sign-payload")]
    LegacySignPayload,
    /// `sign_raw` through the legacy-account API.
    #[display("legacy-sign-raw")]
    LegacySignRaw,
    /// `create_transaction` through the legacy-account API.
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
            CreateTransactionAuthorityRequest::IdentityAccount(_) => Self::LegacyCreateTransaction,
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

#[derive(Clone, PartialEq, Eq, Hash, Encode, Decode)]
struct AutoSigningOwner {
    root_public_key: [u8; 32],
    authenticated_sso_identity: Option<[u8; 32]>,
}

impl AutoSigningOwner {
    fn from_session(session: &SessionInfo) -> Self {
        Self {
            root_public_key: session.public_key,
            authenticated_sso_identity: session.sso.as_ref().map(|sso| sso.identity_account_id),
        }
    }
}

#[derive(Encode, Decode)]
struct PersistedAutoSigningKey {
    owner: AutoSigningOwner,
    product_id: String,
    expected_product_subtree_public_key: [u8; 32],
    secret: [u8; 64],
    ring_vrf_domain_entropy: [u8; 32],
}

impl Drop for PersistedAutoSigningKey {
    fn drop(&mut self) {
        self.secret.zeroize();
        self.ring_vrf_domain_entropy.zeroize();
    }
}

type AutoSigningCacheKey = (AutoSigningOwner, String);

fn decode_auto_signing_keys(blob: &[u8]) -> Result<Vec<PersistedAutoSigningKey>, AuthorityError> {
    let mut input = blob;
    let keys = Vec::<PersistedAutoSigningKey>::decode(&mut input).map_err(|_| {
        AuthorityError::Unavailable {
            reason: "persisted AutoSigning capabilities are invalid".to_string(),
        }
    })?;
    if !input.is_empty() {
        return Err(AuthorityError::Unavailable {
            reason: "persisted AutoSigning capabilities contain trailing bytes".to_string(),
        });
    }
    Ok(keys)
}

fn validate_auto_signing_key(
    secret: [u8; 64],
    expected_product_subtree_public_key: [u8; 32],
    ring_vrf_domain_entropy: [u8; 32],
) -> Result<AutoSigningKey, AuthorityError> {
    let secret = Zeroizing::new(secret);
    let ring_vrf_domain_entropy = Zeroizing::new(ring_vrf_domain_entropy);
    let secret_key =
        SecretKey::from_bytes(&secret[..]).map_err(|_| AuthorityError::Unavailable {
            reason: "AutoSigning capability contains an invalid subtree secret".to_string(),
        })?;
    if secret_key.to_public().to_bytes() != expected_product_subtree_public_key {
        return Err(AuthorityError::Unavailable {
            reason: "AutoSigning capability does not match the authenticated product subtree"
                .to_string(),
        });
    }
    Ok(AutoSigningKey::from_parts(
        *secret,
        *ring_vrf_domain_entropy,
    ))
}

#[derive(Default)]
struct SessionLifecycle {
    epoch: u64,
    external_session_active: bool,
}

impl SessionLifecycle {
    fn advance(&mut self) -> u64 {
        self.epoch = self
            .epoch
            .checked_add(1)
            .expect("session lifecycle epoch exhausted");
        self.external_session_active = false;
        self.epoch
    }

    fn advance_preserving_source(&mut self) -> u64 {
        let external_session_active = self.external_session_active;
        let epoch = self.advance();
        self.external_session_active = external_session_active;
        epoch
    }
}

#[derive(Debug, derive_more::Display)]
enum StoredSessionActivationError {
    #[display("stored auth session is absent")]
    Missing,
    #[display("invalid stored auth session: {_0}")]
    Invalid(String),
    #[display("failed to read stored auth session: {_0}")]
    Read(String),
    #[display("stored auth session changed during activation")]
    Changed,
}

/// Remote account authority for a pairing host.
pub(crate) struct PairingHost {
    /// Host platform backing all syscalls.
    pub(super) platform: Arc<dyn Platform>,
    /// Pairing configuration supplied by the embedding host.
    pub(super) host_config: PairingHostConfig,
    /// Shared chain runtime, used to resolve session identity.
    pub(super) chain: ChainRuntime,
    /// Active inter-host session with a signing host.
    session_state: Arc<SessionState>,
    session_store_changes: Arc<SessionStoreChangeNotifier>,
    /// Core-owned auth-state machine emitting to the host.
    pub(super) auth_state: AuthStateMachine,
    /// People-chain statement store RPC client.
    pub(super) statement_store: StatementStoreRpc,
    session_disconnects: Arc<SessionDisconnects>,
    disconnect_monitor: Mutex<Option<SsoDisconnectMonitor>>,
    login_in_flight: Mutex<Option<LoginInFlight>>,
    login_generation: Mutex<u64>,
    statement_store_allowances: Mutex<HashMap<AllowanceCacheKey, StatementStoreAllowanceKey>>,
    bulletin_allowances: Mutex<HashMap<AllowanceCacheKey, BulletinAllowanceKey>>,
    product_subtrees: Mutex<HashMap<(SsoSessionKey, String), [u8; 32]>>,
    auto_signing_keys: Mutex<HashMap<AutoSigningCacheKey, AutoSigningKey>>,
    ring_resolver: Arc<dyn RingResolver>,
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    /// Orders session-secret cache/storage writes against teardown and activation.
    session_secret_storage: futures::lock::Mutex<()>,
    session_store_activation: futures::lock::Mutex<()>,
    session_lifecycle: Mutex<SessionLifecycle>,
    #[cfg(test)]
    external_session_activation_pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
    /// Self-reference captured by the spawned disconnect-monitor task.
    weak_self: Weak<PairingHost>,
    /// Task spawner for background monitors.
    pub(super) spawner: Spawner,
}

impl PairingHost {
    /// Build a pairing host over the shared runtime services.
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
            product_subtrees: Mutex::new(HashMap::new()),
            auto_signing_keys: Mutex::new(HashMap::new()),
            ring_resolver: ChainRingResolver::new(services.chain.clone()),
            ring_vrf_registry: RingVrfRegistryStore::new(services.platform.clone()),
            session_secret_storage: futures::lock::Mutex::new(()),
            session_store_activation: futures::lock::Mutex::new(()),
            session_lifecycle: Mutex::new(SessionLifecycle::default()),
            #[cfg(test)]
            external_session_activation_pause: Mutex::new(None),
            weak_self: weak_self.clone(),
            spawner: services.spawner.clone(),
        })
    }

    /// Shared session holder for connection-status subscriptions.
    pub(crate) fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Signal that the persisted auth session may have changed; the sync task
    /// re-reads it.
    pub(crate) fn notify_session_store_changed(&self) {
        self.advance_session_lifecycle();
        self.session_store_changes.notify();
    }

    fn advance_session_lifecycle(&self) -> u64 {
        let mut lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        lifecycle.advance()
    }

    pub(super) fn current_session_lifecycle_epoch(&self) -> u64 {
        self.session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned")
            .epoch
    }

    pub(super) fn is_session_lifecycle_current(&self, epoch: u64) -> bool {
        self.current_session_lifecycle_epoch() == epoch
    }

    /// Test hook for [`Self::start_session_store_sync`].
    #[cfg(test)]
    pub(crate) fn start_session_store_sync_for_tests(self: Arc<Self>, spawner: Spawner) {
        self.start_session_store_sync(spawner);
    }

    /// Test alias for [`Self::start_remote_monitor_for_current_session`].
    #[cfg(test)]
    pub(crate) fn start_session_supervision_for_current_session(&self) {
        self.start_remote_monitor_for_current_session();
    }

    #[cfg(test)]
    pub(crate) fn pause_external_session_activation_for_tests(
        &self,
    ) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *self
            .external_session_activation_pause
            .lock()
            .expect("external activation pause mutex poisoned") = Some((entered_tx, resume_rx));
        (entered_rx, resume_tx)
    }

    #[cfg(test)]
    async fn wait_at_external_session_activation_pause(&self) {
        let pause = self
            .external_session_activation_pause
            .lock()
            .expect("external activation pause mutex poisoned")
            .take();
        if let Some((entered, resume)) = pause {
            let _ = entered.send(());
            let _ = resume.await;
        }
    }

    fn current_session(&self) -> Option<AuthoritySession> {
        self.session_state.current().as_ref().map(authority_session)
    }

    pub(crate) async fn ring_vrf_providers(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Vec<v01::ProductAccountId>, RingVrfError> {
        let session = self.session_state.current().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .providers(session.public_key, ring)
            .await
    }

    pub(crate) async fn selected_ring_vrf_provider(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Option<v01::ProductAccountId>, RingVrfError> {
        let session = self.session_state.current().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .selected_provider(session.public_key, ring)
            .await
    }

    pub(crate) async fn select_ring_vrf_provider(
        &self,
        ring: v01::RingLocation,
        handle: v01::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let session = self.session_state.current().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .select_provider(session.public_key, ring, handle)
            .await
    }

    /// Start the disconnect monitor when a session is already active.
    #[cfg(test)]
    pub(crate) fn start_remote_monitor_for_current_session(&self) {
        if let Some(session) = self.session_state.current() {
            self.start_disconnect_monitor(&session);
        }
    }

    /// Validate, resolve, and install an externally persisted canonical
    /// session blob without copying it into core storage.
    pub(crate) async fn activate_external_session(&self, blob: &[u8]) -> Result<(), String> {
        let _activation = self.session_store_activation.lock().await;
        let session = crate::host_logic::session::decode_persisted_session(blob)?;
        let activation_epoch = self.advance_session_lifecycle();
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.people_chain_genesis_hash,
            session,
        )
        .await;
        #[cfg(test)]
        self.wait_at_external_session_activation_pause().await;
        self.set_connected_session_if_current(resolved, activation_epoch, true)
            .await;
        Ok(())
    }

    /// Read, validate, resolve, and install the persisted auth session before
    /// returning. Product frames may use the connected session once this
    /// future resolves.
    pub(crate) async fn activate_stored_session(&self) -> Result<(), String> {
        self.reconcile_stored_session(true, false)
            .await
            .map_err(|error| error.to_string())
    }

    async fn reconcile_stored_session(
        &self,
        clear_after_read_error: bool,
        preserve_external_session: bool,
    ) -> Result<(), StoredSessionActivationError> {
        let _activation = self.session_store_activation.lock().await;
        if preserve_external_session
            && self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned")
                .external_session_active
        {
            return Ok(());
        }
        let blob = match self
            .platform
            .read_core_storage(CoreStorageKey::AuthSession)
            .await
        {
            Ok(Some(blob)) => blob,
            Ok(None) => {
                self.clear_disconnected_session(false).await;
                return Err(StoredSessionActivationError::Missing);
            }
            Err(error) => {
                self.clear_disconnected_session(false).await;
                if clear_after_read_error {
                    let _ = self
                        .platform
                        .clear_core_storage(CoreStorageKey::AuthSession)
                        .await;
                }
                return Err(StoredSessionActivationError::Read(error.reason));
            }
        };
        let session = match crate::host_logic::session::decode_persisted_session(&blob) {
            Ok(session) => session,
            Err(error) => {
                self.clear_disconnected_session(true).await;
                return Err(StoredSessionActivationError::Invalid(error));
            }
        };
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.people_chain_genesis_hash,
            session,
        )
        .await;

        // Identity resolution can await chain I/O. Re-read the slot before
        // installation so an older activation cannot overwrite or expose a
        // session replaced while that lookup was in flight.
        let latest = match self
            .platform
            .read_core_storage(CoreStorageKey::AuthSession)
            .await
        {
            Ok(latest) => latest,
            Err(error) => {
                self.clear_disconnected_session(false).await;
                if clear_after_read_error {
                    let _ = self
                        .platform
                        .clear_core_storage(CoreStorageKey::AuthSession)
                        .await;
                }
                return Err(StoredSessionActivationError::Read(error.reason));
            }
        };
        if latest.as_deref() != Some(blob.as_slice()) {
            self.clear_disconnected_session(false).await;
            return Err(StoredSessionActivationError::Changed);
        }

        let resolved_blob = encode_persisted_session(&resolved);
        if resolved_blob != blob {
            let _ = self
                .platform
                .write_core_storage(CoreStorageKey::AuthSession, resolved_blob)
                .await;
        }
        self.set_connected_session(resolved).await;
        Ok(())
    }

    /// Spawn the background task that re-reads the persisted auth session on
    /// every change notification and reconciles the in-memory session.
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
                    .reconcile_stored_session(!cleared_after_read_error, true)
                    .await
                {
                    Ok(())
                    | Err(StoredSessionActivationError::Missing)
                    | Err(StoredSessionActivationError::Invalid(_))
                    | Err(StoredSessionActivationError::Changed) => {
                        cleared_after_read_error = false;
                    }
                    Err(StoredSessionActivationError::Read(_)) => {
                        cleared_after_read_error = true;
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
                self.set_connected_session(*session).await;
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
        if let Some(session) = session {
            let weak_self = self.weak_self.clone();
            (self.spawner)(Box::pin(async move {
                if let Some(host) = weak_self.upgrade() {
                    let _ = host.submit_disconnected_message(&session).await;
                }
            }));
        }
    }

    /// Disconnect and discard pairing bootstrap material so the next login
    /// generates a new device keypair and topic.
    pub(crate) async fn logout_and_reset_pairing(&self) -> Result<(), String> {
        self.disconnect().await;
        self.clear_auto_signing_keys().await.map_err(|reason| {
            format!("session disconnected, but AutoSigning reset failed: {reason}")
        })?;
        self.platform
            .clear_core_storage(CoreStorageKey::PairingDeviceIdentity)
            .await
            .map_err(|error| {
                format!(
                    "session disconnected, but pairing identity reset failed: {}",
                    error.reason
                )
            })?;
        self.platform
            .clear_core_storage(CoreStorageKey::LastProcessedPairingStatement)
            .await
            .map_err(|error| {
                format!(
                    "session disconnected and pairing identity reset, but pairing history reset failed: {}",
                    error.reason
                )
            })
    }

    /// Clear all capability material owned by one product while preserving the
    /// active session and unrelated products.
    pub(crate) async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
        let product_id =
            normalize_product_identifier(product_id).map_err(|error| error.to_string())?;
        let session = {
            let mut lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            lifecycle.advance_preserving_source();
            self.session_state.current()
        };
        let _storage_guard = self.session_secret_storage.lock().await;

        self.statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_product(&product_id));
        self.bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_product(&product_id));
        self.product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .retain(|(_, cached_product_id), _| cached_product_id != &product_id);
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .retain(|(_, cached_product_id), _| cached_product_id != &product_id);

        let mut first_error = self
            .clear_auto_signing_product_under_storage_guard(&product_id)
            .await
            .err();
        if let Some(session) = session.as_ref()
            && let Err(error) =
                allowances::clear_product_allowance_keys(&*self.platform, session, &product_id)
                    .await
            && first_error.is_none()
        {
            first_error = Some(error.to_string());
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Clear the canonical local session and all session capabilities without
    /// sending a peer-disconnect statement.
    pub(crate) async fn reset_session_state(&self) {
        self.cancel_login();
        self.clear_disconnected_session(true).await;
        let _storage_guard = self.session_secret_storage.lock().await;
        self.clear_statement_store_allowance_keys(None);
        self.clear_bulletin_allowance_keys(None);
        self.clear_product_subtrees(None);
    }

    /// Invalidate in-flight login attempts and emit the cancelled auth state.
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
        let previous = {
            let mut lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            lifecycle.advance();
            let previous = self.session_state.current();
            self.session_state.clear_session();
            previous
        };
        self.stop_session_channel(previous.as_ref());
        if clear_auth_session {
            let _ = self
                .platform
                .clear_core_storage(CoreStorageKey::AuthSession)
                .await;
        }
        let _storage_guard = self.session_secret_storage.lock().await;
        if let Some(session) = previous.as_ref() {
            self.clear_statement_store_allowance_keys(Some(session));
            self.clear_bulletin_allowance_keys(Some(session));
            if let Err(reason) =
                allowances::clear_session_allowance_keys(&*self.platform, session).await
            {
                warn!(%reason, "allowance capability clear failed during disconnect");
            }
        }
        self.clear_product_subtrees(previous.as_ref());
        if let Err(reason) = self.clear_auto_signing_keys_under_storage_guard().await {
            warn!(%reason, "AutoSigning capability clear failed during disconnect");
        }
        self.auth_state.store_disconnected();
    }

    async fn set_connected_session(&self, session: SessionInfo) {
        let activation_epoch = self.advance_session_lifecycle();
        self.set_connected_session_if_current(session, activation_epoch, false)
            .await;
    }

    async fn set_connected_session_if_current(
        &self,
        session: SessionInfo,
        activation_epoch: u64,
        external_session: bool,
    ) -> bool {
        if !self.is_session_lifecycle_current(activation_epoch) {
            return false;
        }
        let previous = self.session_state.current();
        let identity_replaced = previous.as_ref().is_some_and(|previous| {
            AutoSigningOwner::from_session(previous) != AutoSigningOwner::from_session(&session)
        });
        if identity_replaced {
            if let Err(reason) = self.clear_auto_signing_keys().await {
                warn!(%reason, "AutoSigning capability clear failed during identity replacement");
            }
        } else if previous.is_none() {
            if let Err(reason) = self.clear_auto_signing_keys_for_other_owner(&session).await {
                warn!(%reason, "AutoSigning capability owner reconciliation failed");
            }
        } else {
            let _storage_guard = self.session_secret_storage.lock().await;
        }
        if let Some(previous) = previous.as_ref().filter(|previous| *previous != &session) {
            let _storage_guard = self.session_secret_storage.lock().await;
            self.clear_statement_store_allowance_keys(Some(previous));
            self.clear_bulletin_allowance_keys(Some(previous));
            if let Err(reason) =
                allowances::clear_session_allowance_keys(&*self.platform, previous).await
            {
                warn!(%reason, "allowance capability clear failed during session replacement");
            }
        }
        let previous = {
            let mut lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            if lifecycle.epoch != activation_epoch {
                return false;
            }
            let previous = self.session_state.current();
            self.session_state.set_session(session.clone());
            lifecycle.external_session_active = external_session;
            previous
        };
        if previous.as_ref() != Some(&session) {
            self.stop_session_channel(previous.as_ref());
        }
        self.start_disconnect_monitor(&session);
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        true
    }

    #[cfg(test)]
    pub(crate) async fn set_connected_session_for_tests(&self, session: SessionInfo) {
        self.set_connected_session(session).await;
    }

    #[cfg(test)]
    pub(crate) async fn has_auto_signing_key_for_tests(
        &self,
        session: &SessionInfo,
        product_id: &str,
    ) -> Result<bool, AuthorityError> {
        self.auto_signing_key(session, product_id)
            .await
            .map(|key| key.is_some())
    }

    #[cfg(test)]
    pub(crate) async fn remember_auto_signing_key_for_tests(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        expected_product_subtree_public_key: [u8; 32],
        secret: [u8; 64],
        ring_vrf_domain_entropy: [u8; 32],
    ) -> Result<(), AuthorityError> {
        self.remember_auto_signing_key(
            session,
            lifecycle_epoch,
            product_id,
            expected_product_subtree_public_key,
            secret,
            ring_vrf_domain_entropy,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn register_ring_vrf_key_for_tests(
        &self,
        session: &SessionInfo,
        handle: v01::ProductAccountId,
        ring: v01::RingLocation,
        public_key: [u8; 32],
    ) -> Result<(), RingVrfError> {
        self.ring_vrf_registry
            .register(session.public_key, handle, ring, public_key)
            .await
    }

    #[cfg(test)]
    pub(crate) fn capability_cache_sizes_for_tests(&self) -> (usize, usize, usize, usize) {
        (
            self.statement_store_allowances
                .lock()
                .expect("statement-store allowance cache mutex poisoned")
                .len(),
            self.bulletin_allowances
                .lock()
                .expect("bulletin allowance cache mutex poisoned")
                .len(),
            self.product_subtrees
                .lock()
                .expect("product subtree cache mutex poisoned")
                .len(),
            self.auto_signing_keys
                .lock()
                .expect("AutoSigning key cache mutex poisoned")
                .len(),
        )
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

    fn session_secret_allocation_is_current(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
    ) -> bool {
        session.sso.as_ref().is_some_and(|sso| {
            self.current_sso_session_matches(SsoSessionKey::from_session(sso))
                && self.is_session_lifecycle_current(lifecycle_epoch)
        })
    }

    fn cache_auto_signing_key_if_current(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        cache_key: AutoSigningCacheKey,
        key: AutoSigningKey,
    ) -> bool {
        let lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        if lifecycle.epoch != lifecycle_epoch {
            return false;
        }
        let Some(sso) = session.sso.as_ref() else {
            return false;
        };
        if !self.current_sso_session_matches(SsoSessionKey::from_session(sso)) {
            return false;
        }
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .insert(cache_key, key);
        true
    }

    pub(super) fn cache_product_subtree_if_current(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        cache_key: (SsoSessionKey, String),
        public_key: [u8; 32],
    ) -> bool {
        let lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        if lifecycle.epoch != lifecycle_epoch {
            return false;
        }
        let Some(sso) = session.sso.as_ref() else {
            return false;
        };
        if !self.current_sso_session_matches(SsoSessionKey::from_session(sso)) {
            return false;
        }
        self.product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .insert(cache_key, public_key);
        true
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

    /// Persist and memory-cache a freshly allocated statement-store allowance
    /// key.
    pub(super) async fn cache_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        slot_account_key: Vec<u8>,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let allowance = StatementStoreAllowanceKey::from_secret_bytes(slot_account_key)?;
        let _storage_guard = self.session_secret_storage.lock().await;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        allowances::write_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::StatementStore,
            allowance.secret.to_vec(),
        )
        .await?;
        if let Err(error) = self.remember_statement_store_allowance_key(
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
        ) {
            let _ = allowances::remove_allowance_key(
                &*self.platform,
                session,
                product_id,
                AllowanceResource::StatementStore,
            )
            .await;
            return Err(error);
        }
        Ok(allowance)
    }

    fn remember_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: StatementStoreAllowanceKey,
    ) -> Result<(), AuthorityError> {
        let cache_key =
            AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
        let lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        if lifecycle.epoch != lifecycle_epoch
            || !session.sso.as_ref().is_some_and(|sso| {
                self.current_sso_session_matches(SsoSessionKey::from_session(sso))
            })
        {
            return Err(AuthorityError::Disconnected);
        }
        self.statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .insert(cache_key, allowance);
        Ok(())
    }

    /// Cached statement-store allowance key for the product, falling back to
    /// persisted storage.
    pub(super) async fn cached_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<Option<StatementStoreAllowanceKey>, AuthorityError> {
        let cache_key =
            AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
        let _storage_guard = self.session_secret_storage.lock().await;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
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
        self.remember_statement_store_allowance_key(
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
        )?;
        Ok(Some(allowance))
    }

    /// Persist and memory-cache a freshly allocated Bulletin allowance key.
    pub(super) async fn cache_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        slot_account_key: Vec<u8>,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let allowance = BulletinAllowanceKey::from_secret_bytes(slot_account_key)?;
        let _storage_guard = self.session_secret_storage.lock().await;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        allowances::write_allowance_key(
            &*self.platform,
            session,
            product_id,
            AllowanceResource::Bulletin,
            allowance.as_secret_bytes().to_vec(),
        )
        .await?;
        if let Err(error) = self.remember_bulletin_allowance_key(
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
        ) {
            let _ = allowances::remove_allowance_key(
                &*self.platform,
                session,
                product_id,
                AllowanceResource::Bulletin,
            )
            .await;
            return Err(error);
        }
        Ok(allowance)
    }

    fn remember_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: BulletinAllowanceKey,
    ) -> Result<(), AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
        let lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        if lifecycle.epoch != lifecycle_epoch
            || !session.sso.as_ref().is_some_and(|sso| {
                self.current_sso_session_matches(SsoSessionKey::from_session(sso))
            })
        {
            return Err(AuthorityError::Disconnected);
        }
        self.bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .insert(cache_key, allowance);
        Ok(())
    }

    /// Cached Bulletin allowance key for the product, falling back to
    /// persisted storage.
    pub(super) async fn cached_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<Option<BulletinAllowanceKey>, AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
        let _storage_guard = self.session_secret_storage.lock().await;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
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
        self.remember_bulletin_allowance_key(
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
        )?;
        Ok(Some(allowance))
    }

    /// Drop the cached and persisted Bulletin allowance key for one product.
    pub(super) async fn evict_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<(), AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
        let _storage_guard = self.session_secret_storage.lock().await;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
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
        .await?;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }

    /// Drop memory-cached statement-store allowance keys, scoped to `session`
    /// when given, otherwise all.
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

    /// Drop memory-cached Bulletin allowance keys, scoped to `session` when
    /// given, otherwise all.
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

    async fn clear_auto_signing_keys(&self) -> Result<(), String> {
        let _storage_guard = self.session_secret_storage.lock().await;
        self.clear_auto_signing_keys_under_storage_guard().await
    }

    async fn clear_auto_signing_keys_under_storage_guard(&self) -> Result<(), String> {
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .clear();
        self.platform
            .clear_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)
    }

    async fn clear_auto_signing_product_under_storage_guard(
        &self,
        product_id: &str,
    ) -> Result<(), String> {
        let aggregate_result = match self
            .platform
            .read_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
        {
            Err(error) => Err(error.reason),
            Ok(None) => Ok(()),
            Ok(Some(mut blob)) => {
                let decoded = decode_auto_signing_keys(&blob);
                blob.zeroize();
                match decoded {
                    Err(_) => self
                        .platform
                        .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                        .await
                        .map_err(|error| error.reason),
                    Ok(mut keys) => {
                        let before = keys.len();
                        keys.retain(|key| key.product_id != product_id);
                        if keys.len() == before {
                            Ok(())
                        } else if keys.is_empty() {
                            self.platform
                                .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                                .await
                                .map_err(|error| error.reason)
                        } else {
                            self.platform
                                .write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
                                .await
                                .map_err(|error| error.reason)
                        }
                    }
                }
            }
        };
        let legacy_result = self
            .clear_legacy_auto_signing_key(product_id)
            .await
            .map_err(|error| error.to_string());
        aggregate_result.and(legacy_result.map(|_| ()))
    }

    async fn clear_auto_signing_keys_for_other_owner(
        &self,
        session: &SessionInfo,
    ) -> Result<(), String> {
        let owner = AutoSigningOwner::from_session(session);
        let _storage_guard = self.session_secret_storage.lock().await;
        let Some(mut blob) = self
            .platform
            .read_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)?
        else {
            return Ok(());
        };
        let decoded = decode_auto_signing_keys(&blob);
        blob.zeroize();
        let should_clear = decoded
            .as_ref()
            .map(|keys| keys.iter().any(|key| key.owner != owner))
            .unwrap_or(true);
        if !should_clear {
            return Ok(());
        }
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .clear();
        self.platform
            .clear_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)
    }

    async fn clear_legacy_auto_signing_key(
        &self,
        product_id: &str,
    ) -> Result<bool, AuthorityError> {
        let storage_key = CoreStorageKey::AutoSigningKey {
            product_id: product_id.to_string(),
        };
        let legacy = self
            .platform
            .read_core_storage(storage_key.clone())
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to inspect legacy AutoSigning key: {}", err.reason),
            })?;
        let present = if let Some(mut secret) = legacy {
            secret.zeroize();
            true
        } else {
            false
        };
        if present {
            self.platform
                .clear_core_storage(storage_key)
                .await
                .map_err(|err| AuthorityError::Unknown {
                    reason: format!("failed to clear legacy AutoSigning key: {}", err.reason),
                })?;
        }
        Ok(present)
    }

    async fn remember_auto_signing_key(
        &self,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        expected_product_subtree_public_key: [u8; 32],
        secret: [u8; 64],
        ring_vrf_domain_entropy: [u8; 32],
    ) -> Result<(), AuthorityError> {
        let key = validate_auto_signing_key(
            secret,
            expected_product_subtree_public_key,
            ring_vrf_domain_entropy,
        )?;
        let owner = AutoSigningOwner::from_session(session);
        let cache_key = (owner.clone(), product_id.to_string());
        let _storage_guard = self.session_secret_storage.lock().await;
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        self.clear_legacy_auto_signing_key(product_id).await?;
        let mut keys = match self
            .platform
            .read_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
            })? {
            Some(mut blob) => {
                let decoded = decode_auto_signing_keys(&blob).unwrap_or_default();
                blob.zeroize();
                decoded
            }
            None => Vec::new(),
        };
        keys.retain(|persisted| persisted.owner == owner && persisted.product_id != product_id);
        keys.push(PersistedAutoSigningKey {
            owner,
            product_id: product_id.to_string(),
            expected_product_subtree_public_key,
            secret,
            ring_vrf_domain_entropy,
        });
        if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        self.platform
            .write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to persist AutoSigning capability: {}", err.reason),
            })?;
        if !self.cache_auto_signing_key_if_current(session, lifecycle_epoch, cache_key, key) {
            let _ = self
                .clear_auto_signing_product_under_storage_guard(product_id)
                .await;
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }

    async fn auto_signing_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
    ) -> Result<Option<AutoSigningKey>, AuthorityError> {
        let owner = AutoSigningOwner::from_session(session);
        let cache_key = (owner.clone(), product_id.to_string());
        if let Some(key) = self
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(Some(key));
        }

        let _storage_guard = self.session_secret_storage.lock().await;
        if let Some(key) = self
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(Some(key));
        }
        let legacy_present = self.clear_legacy_auto_signing_key(product_id).await?;
        let Some(mut blob) = self
            .platform
            .read_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
            })?
        else {
            return if legacy_present {
                Err(AuthorityError::Unavailable {
                    reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
                })
            } else {
                Ok(None)
            };
        };
        let decoded = decode_auto_signing_keys(&blob);
        blob.zeroize();
        let keys = match decoded {
            Ok(keys) => keys,
            Err(err) => {
                self.auto_signing_keys
                    .lock()
                    .expect("AutoSigning key cache mutex poisoned")
                    .clear();
                let _ = self
                    .platform
                    .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                    .await;
                return Err(err);
            }
        };
        if keys.iter().any(|persisted| persisted.owner != owner) {
            self.auto_signing_keys
                .lock()
                .expect("AutoSigning key cache mutex poisoned")
                .clear();
            let _ = self
                .platform
                .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                .await;
            return Ok(None);
        }
        let Some(persisted) = keys
            .iter()
            .find(|persisted| persisted.product_id == product_id)
        else {
            return if legacy_present {
                Err(AuthorityError::Unavailable {
                    reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
                })
            } else {
                Ok(None)
            };
        };
        let current_expected_subtree = session.sso.as_ref().and_then(|sso| {
            self.product_subtrees
                .lock()
                .expect("product subtree cache mutex poisoned")
                .get(&(SsoSessionKey::from_session(sso), product_id.to_string()))
                .copied()
        });
        if current_expected_subtree
            .is_some_and(|expected| expected != persisted.expected_product_subtree_public_key)
        {
            self.auto_signing_keys
                .lock()
                .expect("AutoSigning key cache mutex poisoned")
                .clear();
            let _ = self
                .platform
                .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                .await;
            return Err(AuthorityError::Unavailable {
                reason: "AutoSigning capability is not for the current product subtree".to_string(),
            });
        }
        let key = match validate_auto_signing_key(
            persisted.secret,
            persisted.expected_product_subtree_public_key,
            persisted.ring_vrf_domain_entropy,
        ) {
            Ok(key) => key,
            Err(err) => {
                self.auto_signing_keys
                    .lock()
                    .expect("AutoSigning key cache mutex poisoned")
                    .clear();
                let _ = self
                    .platform
                    .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                    .await;
                return Err(err);
            }
        };
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .insert(cache_key, key.clone());
        Ok(Some(key))
    }

    fn clear_product_subtrees(&self, session: Option<&SessionInfo>) {
        let mut subtrees = self
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned");
        let Some(session) = session else {
            subtrees.clear();
            return;
        };
        let Some(sso) = session.sso.as_ref() else {
            return;
        };
        let session_key = SsoSessionKey::from_session(sso);
        subtrees.retain(|(key, _), _| *key != session_key);
    }

    fn require_owned_ring_vrf_key(
        calling_product_id: &str,
        handle: &v01::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let caller = normalize_product_identifier(calling_product_id).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        if caller != handle.dot_ns_identifier {
            return Err(RingVrfError::NotAllowlisted);
        }
        Ok(())
    }

    async fn local_ring_vrf_entropy(
        &self,
        session: &SessionInfo,
        handle: &v01::ProductAccountId,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(auto_signing) = self
            .auto_signing_key(session, &handle.dot_ns_identifier)
            .await
            .map_err(RingVrfError::from)?
        else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
            auto_signing.ring_vrf_domain_entropy(),
            &handle.derivation_index,
        ));
        if entry.public_key != Some(member_from_entropy(&entropy)?) {
            return Err(RingVrfError::Unknown {
                reason: "registered ring-VRF public key does not match the AutoSigning capability"
                    .to_string(),
            });
        }
        Ok(Some(entropy))
    }

    async fn local_ring_vrf_entropy_for_ring(
        &self,
        session: &SessionInfo,
        handle: &v01::ProductAccountId,
        ring: &v01::RingLocation,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(entropy) = self.local_ring_vrf_entropy(session, handle).await? else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        if !entry.rings.contains(ring) {
            return Err(RingVrfError::KeyNotInRing);
        }
        Ok(Some(entropy))
    }

    fn mirror_ring_vrf_registration(
        &self,
        session: SessionInfo,
        request: RegisterRingVrfKeyAuthorityRequest,
    ) {
        let weak_self = self.weak_self.clone();
        (self.spawner)(Box::pin(async move {
            let Some(host) = weak_self.upgrade() else {
                return;
            };
            let cx = CallContext::with_request_id(format!(
                "ring-vrf-registration-mirror:{}",
                sso_message_id()
            ));
            if let Err(error) = host
                .remote_register_ring_vrf_key(&cx, &session, request)
                .await
            {
                warn!(?error, "ring-VRF registration mirror failed");
            }
        }));
    }

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_product_subtree_public_key(cx, &session, product_id)
            .await
    }

    async fn sign_vrf(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: String,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        let session = self.current_private_session(session)?;
        if calling_product_id == request.account.dot_ns_identifier
            && let Some(auto_signing_key) = self
                .auto_signing_key(&session, &request.account.dot_ns_identifier)
                .await?
        {
            let keypair = derive_product_keypair_from_subtree_secret(
                *auto_signing_key.as_secret_bytes(),
                derivation_index_bytes(&request.account.derivation_index),
            )
            .map_err(|err| AuthorityError::Unknown {
                reason: err.to_string(),
            })?;
            let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
                &keypair,
                &request.transcript_label,
                request
                    .items
                    .iter()
                    .map(|item| (item.label.as_slice(), item.value.as_slice())),
            );
            return Ok(v01::VrfSignature { pre_output, proof });
        }
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::SignVrf(SignVrfReview {
                calling_product_id: calling_product_id.clone(),
                request: request.clone(),
            }))
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("VRF signing confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(AuthorityError::Rejected);
        }
        self.remote_sign_vrf(cx, &session, calling_product_id, request)
            .await
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
        let private_session = self.current_private_session(session)?;
        if request.calling_product_id == request.key_handle.dot_ns_identifier
            && let Some(entropy) = self
                .local_ring_vrf_entropy_for_ring(
                    &private_session,
                    &request.key_handle,
                    &request.ring_location,
                )
                .await?
        {
            self.ring_resolver.validate(&request.ring_location).await?;
            self.current_private_session(session)?;
            let context = context_bytes(&request.context);
            let alias = alias_from_entropy(&entropy, &context)?;
            return Ok(v01::ContextualAlias {
                context,
                alias: alias.to_vec(),
            });
        }
        self.remote_account_alias(cx, &private_session, request)
            .await
    }

    async fn create_proof(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: CreateProofAuthorityRequest,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        Self::require_owned_ring_vrf_key(&request.calling_product_id, &request.key_handle)?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy_for_ring(
                &private_session,
                &request.key_handle,
                &request.ring_location,
            )
            .await?
        {
            let member = member_from_entropy(&entropy)?;
            let resolved = self
                .ring_resolver
                .resolve(&request.ring_location, &[MemberCandidate { member }])
                .await?;
            self.current_private_session(session)?;
            let context = context_bytes(&request.context);
            let (proof, alias) = create_proof(&entropy, &resolved, &context, &request.message)?;
            return Ok(v01::HostAccountCreateProofResponse {
                proof,
                contextual_alias: v01::ContextualAlias {
                    context,
                    alias: alias.to_vec(),
                },
                ring_index: resolved.ring_index,
                ring_revision: resolved.ring_revision,
            });
        }
        self.remote_create_proof(cx, &private_session, request)
            .await
    }

    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: RegisterRingVrfKeyAuthorityRequest,
    ) -> Result<v01::RingVrfPublicKey, RingVrfError> {
        let private_session = self.current_private_session(session)?;
        let handle = v01::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&request.calling_product_id).map_err(
                |error| RingVrfError::Unknown {
                    reason: error.to_string(),
                },
            )?,
            derivation_index: request.index.clone(),
        };
        if let Some(auto_signing) = self
            .auto_signing_key(&private_session, &request.calling_product_id)
            .await
            .map_err(RingVrfError::from)?
        {
            self.ring_resolver.validate(&request.ring).await?;
            self.current_private_session(session)?;
            let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
                auto_signing.ring_vrf_domain_entropy(),
                &request.index,
            ));
            let public_key = member_from_entropy(&entropy)?;
            self.ring_vrf_registry
                .register(
                    private_session.public_key,
                    handle,
                    request.ring.clone(),
                    public_key,
                )
                .await?;
            self.current_private_session(session)?;
            self.mirror_ring_vrf_registration(private_session, request);
            return Ok(public_key);
        }
        let public_key = self
            .remote_register_ring_vrf_key(cx, &private_session, request.clone())
            .await?;
        self.ring_vrf_registry
            .register(private_session.public_key, handle, request.ring, public_key)
            .await?;
        self.current_private_session(session)?;
        Ok(public_key)
    }

    async fn list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ListRingVrfKeysAuthorityRequest,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        let private_session = self.current_private_session(session)?;
        let owner = normalize_product_identifier(&request.owner).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        if request.calling_product_id == owner
            && let Some(mut entries) = self
                .ring_vrf_registry
                .complete_owner_entries(private_session.public_key, &owner)
                .await?
        {
            self.current_private_session(session)?;
            apply_ring_vrf_disclosure(&mut entries, request.disclosure);
            return Ok(entries);
        }
        let requested_disclosure = request.disclosure;
        let mut remote_request = request;
        if remote_request.calling_product_id == owner {
            remote_request.disclosure = v01::RingVrfKeyDisclosure::PublicKey;
        }
        let mut entries = self
            .remote_list_ring_vrf_keys(cx, &private_session, remote_request)
            .await?;
        validate_owner_listing(&owner, &entries)?;
        if entries.iter().all(|entry| entry.public_key.is_some()) {
            entries = self
                .ring_vrf_registry
                .reconcile_owner(private_session.public_key, &owner, entries)
                .await?;
        }
        self.current_private_session(session)?;
        apply_ring_vrf_disclosure(&mut entries, requested_disclosure);
        Ok(entries)
    }

    async fn ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: RingVrfSignAuthorityRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        Self::require_owned_ring_vrf_key(&request.calling_product_id, &request.key_handle)?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy(&private_session, &request.key_handle)
            .await?
        {
            self.current_private_session(session)?;
            return sign_from_entropy(&entropy, &request.message);
        }
        self.remote_ring_vrf_sign(cx, &private_session, request)
            .await
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

fn apply_ring_vrf_disclosure(
    entries: &mut [v01::RegisteredRingVrfKey],
    disclosure: v01::RingVrfKeyDisclosure,
) {
    if disclosure == v01::RingVrfKeyDisclosure::Anonymized {
        for entry in entries {
            entry.public_key = None;
        }
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

    #[cfg(test)]
    fn cache_product_subtree_for_test(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        let sso = session.sso.as_ref().expect("test session must contain SSO");
        self.product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .insert(
                (SsoSessionKey::from_session(sso), product_id.to_string()),
                public_key,
            );
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

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        PairingHost::product_subtree_public_key(self, cx, session, product_id).await
    }

    async fn sign_vrf(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: String,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        PairingHost::sign_vrf(self, cx, session, calling_product_id, request).await
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

    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: RegisterRingVrfKeyAuthorityRequest,
    ) -> Result<v01::RingVrfPublicKey, RingVrfError> {
        PairingHost::register_ring_vrf_key(self, cx, session, request).await
    }

    async fn list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ListRingVrfKeysAuthorityRequest,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        PairingHost::list_ring_vrf_keys(self, cx, session, request).await
    }

    async fn ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: RingVrfSignAuthorityRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        PairingHost::ring_vrf_sign(self, cx, session, request).await
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
