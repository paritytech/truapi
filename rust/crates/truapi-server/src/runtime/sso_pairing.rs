//! SSO pairing (login): presents the pairing deeplink, watches the bootstrap
//! topic on the statement store (live subscription plus periodic snapshot
//! queries), and decrypts the wallet's V2 handshake response into a session.

use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::auth_state::AuthStateMachine;
use super::sso_remote::SharedRemoteSubscriptionId;
use super::{PlatformRuntimeHost, connected_session_ui_info};
use crate::host_logic::session::{SessionInfo, encode_persisted_session};
use crate::host_logic::sso_pairing::{
    EncryptedHandshakeResponseV2, PairingBootstrap, PairingDeviceIdentity,
    VersionedHandshakeResponse, create_pairing_bootstrap_from_identity, decode_app_handshake_data,
    decrypt_v2_handshake_response, establish_sso_session_info, generate_pairing_device_identity,
};
use crate::host_logic::statement_store::{
    decode_verified_statement_data, parse_new_statements, parse_subscribe_ack,
    subscribe_match_all_request, unsubscribe_request,
};

use futures::channel::oneshot;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use parity_scale_codec::Encode;
use tracing::{debug, info, instrument};
use truapi::CallError;
use truapi::v01;
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi_platform::{
    ChainProvider as PlatformChainProvider, CoreStorage as PlatformCoreStorage, CoreStorageKey,
    JsonRpcConnection, Platform,
};

/// Request id for the long-lived pairing topic subscription.
pub(crate) const PAIRING_SUBSCRIBE_REQUEST_ID: &str = "truapi:sso-pairing:1";
#[cfg(not(test))]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_millis(1);
#[cfg(not(test))]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 15;
#[cfg(test)]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 10;

struct PairingSubscriptionGuard {
    connection: Box<dyn JsonRpcConnection>,
    unsubscribe_request_id: String,
    remote_subscription_id: SharedRemoteSubscriptionId,
}

impl PairingSubscriptionGuard {
    fn new(connection: Box<dyn JsonRpcConnection>) -> Self {
        Self {
            connection,
            unsubscribe_request_id: format!("{PAIRING_SUBSCRIBE_REQUEST_ID}:unsubscribe"),
            remote_subscription_id: Arc::new(Mutex::new(None)),
        }
    }

    fn remote_subscription_id(&self) -> SharedRemoteSubscriptionId {
        self.remote_subscription_id.clone()
    }
}

impl Drop for PairingSubscriptionGuard {
    fn drop(&mut self) {
        if let Some(remote_subscription_id) = self
            .remote_subscription_id
            .lock()
            .expect("pairing subscription id mutex poisoned")
            .as_ref()
        {
            self.connection.send(unsubscribe_request(
                &self.unsubscribe_request_id,
                remote_subscription_id,
            ));
        }
    }
}

/// Terminal outcome of [`PlatformRuntimeHost::run_pairing_flow`].
enum PairingFlowOutcome {
    /// The login was cancelled (host `cancel_login`, `disconnect`, or a
    /// cross-tab session win).
    Cancelled,
    /// Wallet handshake completed; the session is resolved and persisted.
    Success(Box<SessionInfo>),
}

/// Resets a `Pairing` state left behind by a dropped login future (e.g. the
/// transport dropping in-flight calls on connection close). A no-op once the
/// flow reached any terminal transition or a newer pairing took over.
struct AbandonedPairingGuard<P: Platform> {
    auth_state: AuthStateMachine<P>,
    epoch: u64,
}

impl<P: Platform> Drop for AbandonedPairingGuard<P> {
    fn drop(&mut self) {
        self.auth_state.reset_abandoned_pairing(self.epoch);
    }
}

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    /// `request_login` pairing flow: emits `AuthState::Pairing` for the host
    /// to present, then races host cancellation against the wallet handshake
    /// arriving on the statement store; on success resolves identity and
    /// persists the new session.
    pub(super) async fn request_login_flow(
        &self,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        if let Some(session) = self.session_state.current() {
            debug!("request_login: already connected, returning early");
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            return Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ));
        }

        let pairing_identity = create_fresh_pairing_device_identity(self.platform.as_ref())
            .await
            .map_err(|reason| self.fail_before_pairing(reason))?;
        let bootstrap =
            create_pairing_bootstrap_from_identity(&self.runtime_config, pairing_identity)
                .map_err(|err| self.fail_before_pairing(err.to_string()))?;

        let Some((cancel_rx, pairing_epoch)) =
            self.auth_state.pairing_started(bootstrap.deeplink.clone())
        else {
            return Err(CallError::Domain(HostRequestLoginError::V1(
                v01::HostRequestLoginError::Unknown {
                    reason: "login already in progress".to_string(),
                },
            )));
        };
        info!("presenting pairing QR, waiting for wallet handshake");
        let _reset_guard = AbandonedPairingGuard {
            auth_state: self.auth_state.clone(),
            epoch: pairing_epoch,
        };

        match self.run_pairing_flow(&bootstrap, cancel_rx).await {
            Ok(PairingFlowOutcome::Cancelled) => {
                // `cancel_login` (or the cross-tab `connected` transition)
                // already moved the auth state; only the call result is left
                // to map. A session appearing mid-flow means another runtime
                // won the login.
                if self.session_state.current().is_some() {
                    info!("login cancelled by a cross-runtime session win");
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::AlreadyConnected,
                    ))
                } else {
                    info!("login cancelled before handshake, login rejected");
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ))
                }
            }
            Ok(PairingFlowOutcome::Success(session)) => {
                let session = *session;
                self.auth_state
                    .connected(&connected_session_ui_info(&session));
                self.session_state.set_session(session.clone());
                self.start_sso_disconnect_monitor(&session);
                info!("login succeeded, SSO session established");
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ))
            }
            Err(reason) => {
                self.auth_state.login_failed(reason.clone());
                Err(CallError::HostFailure { reason })
            }
        }
    }

    /// Emit `LoginFailed` for an error raised before the pairing was entered
    /// and map it onto the `request_login` error shape.
    fn fail_before_pairing(&self, reason: String) -> CallError<HostRequestLoginError> {
        self.auth_state.login_failed_before_pairing(reason.clone());
        CallError::Domain(HostRequestLoginError::V1(
            v01::HostRequestLoginError::Unknown { reason },
        ))
    }

    /// Everything between the `Pairing` emission and a terminal outcome.
    /// Every error returned here maps to `AuthState::LoginFailed` at the
    /// single exit in [`Self::request_login_flow`].
    async fn run_pairing_flow(
        &self,
        bootstrap: &PairingBootstrap,
        cancel_rx: oneshot::Receiver<()>,
    ) -> Result<PairingFlowOutcome, String> {
        let mut cancel = cancel_rx.fuse();
        let statement_store_connect = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .fuse();
        pin_mut!(statement_store_connect);

        let statement_store = futures::select! {
            _ = cancel => return Ok(PairingFlowOutcome::Cancelled),
            connect_result = statement_store_connect => connect_result.map_err(|err| {
                format!("pairing statement-store connect failed: {err:?}")
            })?,
        };
        statement_store.send(subscribe_match_all_request(
            PAIRING_SUBSCRIBE_REQUEST_ID,
            &[bootstrap.topic],
        ));
        debug!("subscribed to pairing topic, polling statement store");
        let responses = statement_store.responses();
        let subscription_guard = PairingSubscriptionGuard::new(statement_store);
        let pairing_response = wait_for_v2_pairing_success(
            subscription_guard.connection.as_ref(),
            responses,
            subscription_guard.remote_subscription_id(),
            bootstrap.topic,
            bootstrap.encryption_secret_key,
        )
        .fuse();
        pin_mut!(pairing_response);

        let response = futures::select! {
            _ = cancel => return Ok(PairingFlowOutcome::Cancelled),
            response_result = pairing_response => response_result?,
        };
        let sso = establish_sso_session_info(
            bootstrap,
            response.peer_statement_account_id,
            response.success.sso_enc_pub_key,
        )?;
        let session = SessionInfo {
            public_key: response.success.root_account_id,
            sso: Some(sso),
            root_entropy_source: Some(response.success.root_entropy_source),
            identity_account_id: Some(response.success.identity_account_id),
            lite_username: None,
            full_username: None,
        };
        let session = self.resolve_session_identity(session).await;
        PlatformCoreStorage::write_core_storage(
            self.platform.as_ref(),
            CoreStorageKey::AuthSession,
            encode_persisted_session(&session),
        )
        .await
        .map_err(|err| format!("session persist failed: {err:?}"))?;
        Ok(PairingFlowOutcome::Success(Box::new(session)))
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing_device.create_fresh"))]
async fn create_fresh_pairing_device_identity(
    storage: &(impl PlatformCoreStorage + ?Sized),
) -> Result<PairingDeviceIdentity, String> {
    let identity = generate_pairing_device_identity()
        .map_err(|err| format!("pairing identity failed: {err}"))?;
    PlatformCoreStorage::write_core_storage(
        storage,
        CoreStorageKey::PairingDeviceIdentity,
        identity.encode(),
    )
    .await
    .map_err(|err| format!("pairing device identity write failed: {err:?}"))?;
    Ok(identity)
}

struct PairingSuccess {
    peer_statement_account_id: [u8; 32],
    success: crate::host_logic::sso_pairing::HandshakeSuccessV2,
}

#[derive(Default)]
struct PairingFrameState {
    remote_subscription_id: Option<String>,
    query: PairingQueryState,
}

#[derive(Default)]
enum PairingQueryState {
    #[default]
    Idle,
    AwaitingAck {
        request_id: String,
        elapsed_ticks: u8,
    },
    Active {
        request_id: String,
        remote_id: String,
        elapsed_ticks: u8,
    },
}

impl PairingQueryState {
    fn request_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::AwaitingAck { request_id, .. } | Self::Active { request_id, .. } => {
                Some(request_id)
            }
        }
    }

    fn remote_id(&self) -> Option<&str> {
        match self {
            Self::Active { remote_id, .. } => Some(remote_id),
            Self::Idle | Self::AwaitingAck { .. } => None,
        }
    }

    fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    fn start(&mut self, request_id: String) {
        *self = Self::AwaitingAck {
            request_id,
            elapsed_ticks: 0,
        };
    }

    fn activate(&mut self, request_id: String, remote_id: String) {
        *self = Self::Active {
            request_id,
            remote_id,
            elapsed_ticks: 0,
        };
    }

    fn finish(&mut self) {
        *self = Self::Idle;
    }

    fn tick_timeout(&mut self) -> Option<(String, String)> {
        match self {
            Self::Idle => None,
            Self::AwaitingAck { elapsed_ticks, .. } => {
                *elapsed_ticks = elapsed_ticks.saturating_add(1);
                if *elapsed_ticks >= PAIRING_QUERY_TIMEOUT_TICKS {
                    *self = Self::Idle;
                }
                None
            }
            Self::Active {
                request_id,
                remote_id,
                elapsed_ticks,
            } => {
                *elapsed_ticks = elapsed_ticks.saturating_add(1);
                if *elapsed_ticks < PAIRING_QUERY_TIMEOUT_TICKS {
                    return None;
                }
                let timeout = Some((request_id.clone(), remote_id.clone()));
                *self = Self::Idle;
                timeout
            }
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.wait_success"))]
async fn wait_for_v2_pairing_success(
    connection: &dyn JsonRpcConnection,
    mut responses: BoxStream<'static, String>,
    remote_subscription_slot: SharedRemoteSubscriptionId,
    topic: [u8; 32],
    core_encryption_secret_key: [u8; 32],
) -> Result<PairingSuccess, String> {
    let mut state = PairingFrameState::default();
    let mut query_counter = 0usize;
    let poll = futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse();
    pin_mut!(poll);
    loop {
        futures::select! {
            frame = responses.next().fuse() => {
                let Some(frame) = frame else {
                    return Err("pairing statement-store response stream ended".to_string());
                };
                if let Some(success) = handle_v2_pairing_frame(
                    connection,
                    &frame,
                    &mut state,
                    &remote_subscription_slot,
                    core_encryption_secret_key,
                )? {
                    return Ok(success);
                }
            }
            _ = poll => {
                if let Some((request_id, remote_id)) = state.query.tick_timeout() {
                    connection.send(unsubscribe_request(
                        &format!("{request_id}:timeout-unsubscribe"),
                        &remote_id,
                    ));
                }
                if state.query.is_idle() {
                    query_counter += 1;
                    let query_request_id =
                        format!("{PAIRING_SUBSCRIBE_REQUEST_ID}:query:{query_counter}");
                    connection.send(subscribe_match_all_request(&query_request_id, &[topic]));
                    state.query.start(query_request_id);
                }
                poll.set(futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse());
            }
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.handle_frame"))]
fn handle_v2_pairing_frame(
    connection: &dyn JsonRpcConnection,
    frame: &str,
    state: &mut PairingFrameState,
    remote_subscription_slot: &SharedRemoteSubscriptionId,
    core_encryption_secret_key: [u8; 32],
) -> Result<Option<PairingSuccess>, String> {
    if state.remote_subscription_id.is_none()
        && let Some(id) = parse_subscribe_ack(frame, PAIRING_SUBSCRIBE_REQUEST_ID)
            .map_err(|err| err.to_string())?
    {
        *remote_subscription_slot
            .lock()
            .expect("pairing subscription id mutex poisoned") = Some(id.clone());
        state.remote_subscription_id = Some(id);
        return Ok(None);
    }
    if let Some((query_request_id, id)) =
        parse_pairing_query_subscribe_ack(frame, state.query.request_id())?
    {
        state.query.activate(query_request_id, id);
        return Ok(None);
    }

    let Some(page) = parse_new_statements(frame).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let is_live_subscription =
        Some(page.remote_subscription_id.as_str()) == state.remote_subscription_id.as_deref();
    let is_query_subscription =
        Some(page.remote_subscription_id.as_str()) == state.query.remote_id();
    if !is_live_subscription && !is_query_subscription {
        return Ok(None);
    }

    if is_query_subscription && page.remaining.unwrap_or(0) == 0 {
        if let Some(request_id) = state.query.request_id() {
            connection.send(unsubscribe_request(
                &format!("{request_id}:unsubscribe"),
                &page.remote_subscription_id,
            ));
        }
        state.query.finish();
    }
    for statement in page.statements {
        if let Some(success) = decode_v2_pairing_statement(&statement, core_encryption_secret_key)?
        {
            return Ok(Some(success));
        }
    }

    Ok(None)
}

fn parse_pairing_query_subscribe_ack(
    frame: &str,
    pending_query_request_id: Option<&str>,
) -> Result<Option<(String, String)>, String> {
    let value: serde_json::Value = serde_json::from_str(frame).map_err(|err| err.to_string())?;
    let Some(request_id) = value.get("id").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let is_pending_query = pending_query_request_id == Some(request_id);
    let is_pairing_query = request_id
        .strip_prefix(PAIRING_SUBSCRIBE_REQUEST_ID)
        .is_some_and(|suffix| suffix.starts_with(":query:"));
    if !is_pending_query && !is_pairing_query {
        return Ok(None);
    }
    if value
        .get("method")
        .and_then(serde_json::Value::as_str)
        .is_some()
        && value.get("params").is_some()
        && value.get("result").is_none()
        && value.get("error").is_none()
    {
        return Ok(None);
    }
    if let Some(error) = value.get("error") {
        return Err(error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("statement-store query subscribe failed")
            .to_string());
    }
    let Some(remote_id) = value.get("result").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    Ok(Some((request_id.to_string(), remote_id.to_string())))
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.decode_statement"))]
fn decode_v2_pairing_statement(
    statement: &[u8],
    core_encryption_secret_key: [u8; 32],
) -> Result<Option<PairingSuccess>, String> {
    let verified =
        decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
    let handshake = decode_app_handshake_data(&verified.data)?;
    let VersionedHandshakeResponse::V2 {
        encrypted_message,
        public_key,
    } = handshake
    else {
        return Err("pairing response is not SSO V2".to_string());
    };
    match decrypt_v2_handshake_response(core_encryption_secret_key, public_key, &encrypted_message)?
    {
        EncryptedHandshakeResponseV2::Pending(_) => Ok(None),
        EncryptedHandshakeResponseV2::Failed(reason) => Err(reason),
        EncryptedHandshakeResponseV2::Success(success) => Ok(Some(PairingSuccess {
            peer_statement_account_id: verified.signer,
            success: *success,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        StubPlatform, core_storage_test_key, pairing_device_from_deeplink, peer_statement_keypair,
        runtime_config, session_info, signed_test_statement, stub_platform, test_spawner,
    };
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use truapi::CallContext;
    use truapi::api::Account;
    use truapi::versioned::account::{
        HostAccountConnectionStatusSubscribeItem, HostRequestLoginRequest,
    };
    use truapi_platform::{AuthState, CoreStorageKey};

    /// Cancel the login as soon as the host observes the `Pairing` state,
    /// mimicking a user dismissing the pairing UI immediately.
    fn cancel_on_pairing(platform: &StubPlatform, host: &Arc<PlatformRuntimeHost<StubPlatform>>) {
        let host = host.clone();
        *platform
            .on_auth_state
            .lock()
            .expect("auth state hook mutex poisoned") = Some(Arc::new(move |state| {
            if matches!(state, AuthState::Pairing { .. }) {
                host.cancel_login();
            }
        }));
    }

    #[test]
    fn request_login_presents_pairing_and_rejects_when_cancelled() {
        let platform = stub_platform();
        let host = Arc::new(PlatformRuntimeHost::new_compat(
            platform.clone(),
            test_spawner(),
        ));
        cancel_on_pairing(&platform, &host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        let auth_states = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned");
        assert_eq!(auth_states.len(), 2, "states: {auth_states:?}");
        match &auth_states[0] {
            AuthState::Pairing { deeplink } => {
                assert!(deeplink.starts_with("polkadotapp://pair?handshake="));
            }
            other => panic!("expected pairing state first, got {other:?}"),
        }
        assert_eq!(auth_states[1], AuthState::Disconnected);

        let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        if let Some(sent) = sent_rpc.first() {
            let request: serde_json::Value = serde_json::from_str(sent).unwrap();
            assert_eq!(request["method"], "statement_subscribeStatement");
            assert_eq!(
                request["params"][0]["matchAll"][0].as_str().unwrap().len(),
                66
            );
        }
    }

    #[test]
    fn request_login_rotates_pairing_device_identity_between_attempts() {
        let platform = stub_platform();
        let host = Arc::new(PlatformRuntimeHost::new_compat(
            platform.clone(),
            test_spawner(),
        ));
        cancel_on_pairing(&platform, &host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });

        let first = futures::executor::block_on(host.request_login(&cx, request.clone())).unwrap();
        let second = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            first,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert_eq!(
            second,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        let deeplinks: Vec<String> = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned")
            .iter()
            .filter_map(|state| match state {
                AuthState::Pairing { deeplink } => Some(deeplink.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(deeplinks.len(), 2);
        assert_ne!(
            pairing_device_from_deeplink(&deeplinks[0]),
            pairing_device_from_deeplink(&deeplinks[1])
        );
        assert!(
            platform
                .local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .contains_key(&core_storage_test_key(
                    CoreStorageKey::PairingDeviceIdentity
                ))
        );
    }

    #[test]
    fn request_login_waits_for_pairing_statement() {
        let wallet_ephemeral_secret = p256::SecretKey::from_slice(&[2; 32]).unwrap();
        let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
        let mut wallet_ephemeral_public_bytes = [0u8; 65];
        wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());
        let handshake = crate::host_logic::sso_pairing::VersionedHandshakeResponse::V2 {
            encrypted_message: vec![0xde, 0xad],
            public_key: wallet_ephemeral_public_bytes,
        };
        let statement = signed_test_statement(handshake.encode());
        let notification = format!(
            r#"{{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":{{"subscription":"remote-sub","result":{{"event":"newStatements","data":{{"statements":["0x{}"],"remaining":0}}}}}}}}"#,
            hex::encode(statement)
        );
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                r#"{"jsonrpc":"2.0","id":"truapi:sso-pairing:1","result":"remote-sub"}"#
                    .to_string(),
                notification,
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::HostFailure { reason } => {
                assert_eq!(reason, "encrypted SSO handshake answer is too short");
            }
            other => panic!("expected handshake decrypt failure, got {other:?}"),
        }
        let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let methods = sent_rpc
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["method"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            methods.first().map(String::as_str),
            Some("statement_subscribeStatement")
        );
        assert!(
            methods
                .iter()
                .any(|method| method == "statement_unsubscribeStatement"),
            "pairing subscription should be cleaned up"
        );
        let unsubscribe: serde_json::Value = serde_json::from_str(&sent_rpc[1]).unwrap();
        assert_eq!(unsubscribe["params"][0], "remote-sub");
    }

    #[test]
    fn request_login_accepts_valid_pairing_statement_and_persists_session() {
        let session_writes = Arc::new(Mutex::new(Vec::new()));
        let platform = Arc::new(StubPlatform {
            pairing_success_response: true,
            session_writes: session_writes.clone(),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let mut statuses = host.session_state().subscribe();
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Disconnected
            )
        );

        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Success)
        );
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );

        let session = host
            .session_state()
            .current()
            .expect("paired session should be active");
        assert_eq!(session.public_key, session_info().public_key);
        assert_eq!(session.root_entropy_source, Some([0x66; 32]));
        assert_eq!(
            session.sso.as_ref().unwrap().identity_account_id,
            peer_statement_keypair().1
        );

        let writes = session_writes
            .lock()
            .expect("session write list mutex poisoned");
        assert_eq!(writes.len(), 1);
        assert_eq!(
            crate::host_logic::session::decode_persisted_session(&writes[0]).unwrap(),
            session
        );

        let auth_states = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned");
        assert_eq!(auth_states.len(), 2, "states: {auth_states:?}");
        assert!(matches!(&auth_states[0], AuthState::Pairing { .. }));
        assert_eq!(
            auth_states[1],
            AuthState::Connected(connected_session_ui_info(&session))
        );
        drop(auth_states);

        let methods = platform
            .sent_rpc
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["method"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            methods.first().map(String::as_str),
            Some("statement_subscribeStatement")
        );
        assert!(
            methods
                .iter()
                .any(|method| method == "statement_unsubscribeStatement"),
            "pairing subscription should be cleaned up"
        );
    }

    /// The pairing success must also be reachable through the core's own 2s
    /// snapshot queries: the live subscription stays silent and the wallet
    /// statement is delivered only on a query subscription page.
    #[test]
    fn request_login_accepts_pairing_statement_from_snapshot_query_page() {
        let platform = Arc::new(StubPlatform {
            pairing_success_via_query: true,
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Success)
        );
        assert_eq!(
            host.session_state()
                .current()
                .map(|session| session.public_key),
            Some(session_info().public_key)
        );

        let ids = platform
            .sent_rpc
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(
            ids.iter()
                .any(|id| id.starts_with("truapi:sso-pairing:1:query:")),
            "core should issue snapshot queries while pairing: {ids:?}"
        );
        assert!(
            ids.iter()
                .any(|id| id.contains(":query:") && id.ends_with(":unsubscribe")),
            "drained query subscription should be cleaned up: {ids:?}"
        );
    }

    #[test]
    fn pairing_query_parser_ignores_echoed_subscribe_request() {
        let frame = r#"{"jsonrpc":"2.0","id":"truapi:sso-pairing:1:query:7","method":"statement_subscribeStatement","params":[{"matchAll":["0x0707070707070707070707070707070707070707070707070707070707070707"]}]}"#;

        assert_eq!(
            parse_pairing_query_subscribe_ack(frame, Some("truapi:sso-pairing:1:query:7")).unwrap(),
            None
        );
        assert_eq!(
            parse_pairing_query_subscribe_ack(frame, None).unwrap(),
            None
        );
    }

    #[test]
    fn pairing_query_parser_ignores_no_result_subscribe_response() {
        let frame = r#"{"jsonrpc":"2.0","id":"truapi:sso-pairing:1:query:7"}"#;

        assert_eq!(
            parse_pairing_query_subscribe_ack(frame, Some("truapi:sso-pairing:1:query:7")).unwrap(),
            None
        );
        assert_eq!(
            parse_pairing_query_subscribe_ack(frame, None).unwrap(),
            None
        );
    }

    #[test]
    fn request_login_emits_login_failed_for_pre_pairing_errors() {
        let platform = Arc::new(StubPlatform {
            local_storage_error: Some("identity storage unavailable"),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        assert!(matches!(err, CallError::Domain(_)));
        let auth_states = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned");
        assert_eq!(auth_states.len(), 1, "states: {auth_states:?}");
        assert!(matches!(&auth_states[0], AuthState::LoginFailed { reason }
            if reason.contains("identity storage unavailable")));
    }

    #[test]
    fn request_login_does_not_restore_persisted_session_before_pairing() {
        let stored = session_info();
        let platform = Arc::new(StubPlatform {
            session_blob: Some(crate::host_logic::session::encode_persisted_session(
                &stored,
            )),
            ..Default::default()
        });
        let host = Arc::new(PlatformRuntimeHost::new_compat(
            platform.clone(),
            test_spawner(),
        ));
        cancel_on_pairing(&platform, &host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.session_state().current().is_none());
    }

    #[test]
    fn request_login_ignores_corrupt_persisted_session_before_pairing() {
        let session_clears = Arc::new(Mutex::new(0));
        let platform = Arc::new(StubPlatform {
            session_blob: Some(vec![0xff]),
            session_clears: session_clears.clone(),
            ..Default::default()
        });
        let host = Arc::new(PlatformRuntimeHost::new_compat(
            platform.clone(),
            test_spawner(),
        ));
        cancel_on_pairing(&platform, &host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.session_state().current().is_none());
        assert_eq!(*session_clears.lock().unwrap(), 0);
    }

    #[test]
    fn request_login_ignores_session_store_failure_before_pairing() {
        let platform = Arc::new(StubPlatform {
            session_error: Some("storage failed"),
            ..Default::default()
        });
        let host = Arc::new(PlatformRuntimeHost::new_compat(
            platform.clone(),
            test_spawner(),
        ));
        cancel_on_pairing(&platform, &host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.session_state().current().is_none());
    }

    #[test]
    fn request_login_returns_already_connected_when_session_exists() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();
        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::AlreadyConnected)
        );
    }
}
