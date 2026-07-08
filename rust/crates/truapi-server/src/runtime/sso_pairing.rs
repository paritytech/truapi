//! SSO pairing (login): presents the pairing deeplink, watches the bootstrap
//! topic on the statement store (live subscription plus periodic snapshot
//! queries), and decrypts the wallet's V2 handshake response into a session.

#[cfg(test)]
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::auth_state::AuthStateMachine;
use super::identity::resolve_session_identity_with_chain;
use super::pairing_host::PairingHost;
use super::statement_store_rpc;
use crate::host_logic::session::{SessionInfo, encode_persisted_session};
use crate::host_logic::sso::pairing::{
    PairingBootstrap, PairingDeviceIdentity, VersionedHandshakeResponse,
    create_pairing_bootstrap_from_identity, decode_app_handshake_data,
    decrypt_v2_handshake_response, establish_sso_session_info, generate_pairing_device_identity,
    v2,
};
use crate::host_logic::statement_store::{
    decode_verified_statement_data, parse_new_statements_result,
};
use crate::subscription::Spawner;

use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, StreamExt, pin_mut};
use parity_scale_codec::{Decode, Encode};
use serde_json::Value;
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::RpcSubscription;
use tracing::{debug, info, instrument};
use truapi::CallError;
use truapi::v01;
use truapi::versioned::account::HostRequestLoginError;
#[cfg(test)]
use truapi::versioned::account::HostRequestLoginResponse;
use truapi_platform::{CoreStorage, CoreStorageKey};

#[cfg(not(test))]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_millis(1);
#[cfg(not(test))]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 15;
#[cfg(test)]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 10;

/// Terminal outcome of [`SsoPairingFlow::request_session`].
pub(super) enum SsoPairingOutcome {
    /// The login was cancelled (host `cancel_login`, `disconnect`, or a
    /// cross-tab session win).
    Cancelled,
    /// Wallet handshake completed; the session is resolved and persisted.
    Success(Box<SessionInfo>),
}

/// Resets a `Pairing` state left behind by a dropped login future (e.g. the
/// transport dropping in-flight calls on connection close). A no-op once the
/// flow reached any terminal transition or a newer pairing took over.
struct AbandonedPairingGuard {
    auth_state: AuthStateMachine,
    epoch: u64,
    active: bool,
}

impl AbandonedPairingGuard {
    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for AbandonedPairingGuard {
    fn drop(&mut self) {
        if self.active {
            self.auth_state.reset_abandoned_pairing(self.epoch);
        }
    }
}

pub(super) struct SsoPairingFlow<'a> {
    host: &'a PairingHost,
}

impl<'a> SsoPairingFlow<'a> {
    pub(super) fn new(host: &'a PairingHost) -> Self {
        Self { host }
    }

    /// `request_session` pairing flow: emits `AuthState::Pairing` for the host
    /// to present, then races host cancellation against the wallet handshake
    /// arriving on the statement store; on success it resolves identity,
    /// persists the new session, and returns it to the pairing host.
    pub(super) async fn request_session(
        &self,
    ) -> Result<SsoPairingOutcome, CallError<HostRequestLoginError>> {
        let (mut pairing_identity, reused_identity) =
            read_or_create_pairing_device_identity(self.host.platform.as_ref())
                .await
                .map_err(|reason| self.fail_before_pairing(reason))?;
        let last_processed_statement =
            read_last_processed_pairing_statement(self.host.platform.as_ref())
                .await
                .map_err(|reason| self.fail_before_pairing(reason))?;
        // Pairing success statements are retained by statement-store. Reusing a
        // previous pairing identity means reusing its topic, where the only
        // retained response may be the last processed success. Rotate before
        // presenting QR so every explicit login waits on a fresh wallet scan.
        if reused_identity {
            debug!("regenerating stored pairing device identity");
            pairing_identity = create_fresh_pairing_device_identity(self.host.platform.as_ref())
                .await
                .map_err(|reason| self.fail_before_pairing(reason))?;
        }
        let bootstrap =
            create_pairing_bootstrap_from_identity(&self.host.host_config, pairing_identity)
                .map_err(|err| self.fail_before_pairing(err.to_string()))?;

        let Some((cancel_rx, pairing_epoch)) = self
            .host
            .auth_state
            .pairing_started(bootstrap.deeplink.clone())
        else {
            return Err(CallError::Domain(HostRequestLoginError::V1(
                v01::HostRequestLoginError::Unknown {
                    reason: "login already in progress".to_string(),
                },
            )));
        };
        info!("presenting pairing QR, waiting for wallet handshake");
        let mut reset_guard = AbandonedPairingGuard {
            auth_state: self.host.auth_state.clone(),
            epoch: pairing_epoch,
            active: true,
        };

        match self
            .run_pairing_flow(&bootstrap, cancel_rx, last_processed_statement)
            .await
        {
            Ok(outcome @ SsoPairingOutcome::Cancelled) => {
                reset_guard.disarm();
                Ok(outcome)
            }
            Ok(outcome @ SsoPairingOutcome::Success(_)) => {
                reset_guard.disarm();
                Ok(outcome)
            }
            Err(reason) => {
                self.host.auth_state.login_failed(reason.clone());
                Err(CallError::HostFailure { reason })
            }
        }
    }

    /// Emit `LoginFailed` for an error raised before the pairing was entered
    /// and map it onto the `request_login` error shape.
    fn fail_before_pairing(&self, reason: String) -> CallError<HostRequestLoginError> {
        self.host
            .auth_state
            .login_failed_before_pairing(reason.clone());
        CallError::Domain(HostRequestLoginError::V1(
            v01::HostRequestLoginError::Unknown { reason },
        ))
    }

    /// Everything between the `Pairing` emission and a terminal outcome.
    /// Every error returned here maps to `AuthState::LoginFailed` at the
    /// single exit in [`Self::request_login`].
    async fn run_pairing_flow(
        &self,
        bootstrap: &PairingBootstrap,
        cancel_rx: oneshot::Receiver<()>,
        last_processed_statement: Option<Vec<u8>>,
    ) -> Result<SsoPairingOutcome, String> {
        let mut cancel = cancel_rx.fuse();
        let statement_store = self.host.statement_store.clone();
        let statement_store_connect = statement_store.client("pairing statement-store").fuse();
        pin_mut!(statement_store_connect);

        let rpc_client = futures::select! {
            _ = cancel => return Ok(SsoPairingOutcome::Cancelled),
            connect_result = statement_store_connect => connect_result?,
        };
        let subscribe_client = rpc_client.clone();
        let live_topics = [bootstrap.topic];
        let live_subscription =
            statement_store_rpc::subscribe_match_all(&subscribe_client, &live_topics).fuse();
        pin_mut!(live_subscription);
        let live_subscription = futures::select! {
            _ = cancel => return Ok(SsoPairingOutcome::Cancelled),
            subscribe_result = live_subscription => subscribe_result
                .map_err(|err| format!("pairing statement-store subscribe failed: {err}"))?,
        };
        debug!("subscribed to pairing topic, polling statement store");
        let pairing_response = wait_for_v2_pairing_success(
            rpc_client,
            live_subscription,
            bootstrap.topic,
            bootstrap.encryption_secret_key,
            last_processed_statement,
            self.host.spawner.clone(),
        )
        .fuse();
        pin_mut!(pairing_response);

        let response = futures::select! {
            _ = cancel => return Ok(SsoPairingOutcome::Cancelled),
            response_result = pairing_response => response_result?,
        };
        write_last_processed_pairing_statement(self.host.platform.as_ref(), &response.statement)
            .await;
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
        let resolve_session = resolve_session_identity_with_chain(
            &self.host.chain,
            self.host.host_config.people_chain_genesis_hash,
            session,
        )
        .fuse();
        pin_mut!(resolve_session);
        let session = futures::select! {
            _ = cancel => return Ok(SsoPairingOutcome::Cancelled),
            session = resolve_session => session,
        };
        let persist_session = self
            .host
            .platform
            .write_core_storage(
                CoreStorageKey::AuthSession,
                encode_persisted_session(&session),
            )
            .fuse();
        pin_mut!(persist_session);
        futures::select! {
            _ = cancel => {
                clear_auth_session(self.host.platform.as_ref()).await;
                return Ok(SsoPairingOutcome::Cancelled);
            },
            persist_result = persist_session => persist_result
                .map_err(|err| format!("session persist failed: {err:?}"))?,
        };
        futures::select! {
            _ = cancel => {
                clear_auth_session(self.host.platform.as_ref()).await;
                return Ok(SsoPairingOutcome::Cancelled);
            },
            default => {}
        };
        Ok(SsoPairingOutcome::Success(Box::new(session)))
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing_device.create_fresh"))]
async fn create_fresh_pairing_device_identity(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<PairingDeviceIdentity, String> {
    let identity = generate_pairing_device_identity()
        .map_err(|err| format!("pairing identity failed: {err}"))?;
    storage
        .write_core_storage(CoreStorageKey::PairingDeviceIdentity, identity.encode())
        .await
        .map_err(|err| format!("pairing device identity write failed: {err:?}"))?;
    Ok(identity)
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing_device.read_or_create"))]
async fn read_or_create_pairing_device_identity(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<(PairingDeviceIdentity, bool), String> {
    let stored = storage
        .read_core_storage(CoreStorageKey::PairingDeviceIdentity)
        .await
        .map_err(|err| format!("pairing device identity read failed: {err:?}"))?;
    if let Some(stored) = stored {
        match PairingDeviceIdentity::decode(&mut stored.as_slice()) {
            Ok(identity) => return Ok((identity, true)),
            Err(err) => {
                debug!("discarding invalid stored pairing device identity: {err}");
            }
        }
    }

    create_fresh_pairing_device_identity(storage)
        .await
        .map(|identity| (identity, false))
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.last_processed.read"))]
async fn read_last_processed_pairing_statement(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<Option<Vec<u8>>, String> {
    storage
        .read_core_storage(CoreStorageKey::LastProcessedPairingStatement)
        .await
        .map_err(|err| format!("last processed pairing statement read failed: {err:?}"))
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.last_processed.write"))]
async fn write_last_processed_pairing_statement(
    storage: &(impl CoreStorage + ?Sized),
    statement: &[u8],
) {
    if let Err(err) = storage
        .write_core_storage(
            CoreStorageKey::LastProcessedPairingStatement,
            statement.to_vec(),
        )
        .await
    {
        debug!("last processed pairing statement write failed: {err:?}");
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.auth_session.clear"))]
async fn clear_auth_session(storage: &(impl CoreStorage + ?Sized)) {
    if let Err(err) = storage
        .clear_core_storage(CoreStorageKey::AuthSession)
        .await
    {
        debug!("auth session clear failed: {err:?}");
    }
}

/// Decoded wallet handshake success plus the statement metadata needed to
/// persist the authenticated session and remember the handled statement.
struct PairingSuccess {
    statement: Vec<u8>,
    peer_statement_account_id: [u8; 32],
    success: v2::Success,
}

impl PairingSuccess {
    /// Decode one retained statement-store response for the current pairing
    /// topic. `Ok(None)` means the wallet has not produced a final response for
    /// this statement yet; wallet failure statuses are surfaced as `Err`.
    #[instrument(skip_all, fields(runtime.method = "sso.pairing.decode_statement"))]
    fn from_v2_statement(
        statement: &[u8],
        core_encryption_secret_key: [u8; 32],
    ) -> Result<Option<Self>, String> {
        let verified =
            decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
        let VersionedHandshakeResponse::V2 {
            encrypted_message,
            public_key,
        } = decode_app_handshake_data(&verified.data)?;
        match decrypt_v2_handshake_response(
            core_encryption_secret_key,
            public_key,
            &encrypted_message,
        )? {
            v2::EncryptedResponse::Pending(_) => Ok(None),
            v2::EncryptedResponse::Failed(reason) => Err(reason),
            v2::EncryptedResponse::Success(success) => Ok(Some(Self {
                statement: statement.to_vec(),
                peer_statement_account_id: verified.signer,
                success: *success,
            })),
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.wait_success"))]
async fn wait_for_v2_pairing_success(
    rpc_client: RpcClient,
    mut live_subscription: RpcSubscription<Value>,
    topic: [u8; 32],
    core_encryption_secret_key: [u8; 32],
    last_processed_statement: Option<Vec<u8>>,
    spawner: Spawner,
) -> Result<PairingSuccess, String> {
    let (query_tx, mut query_rx) = mpsc::unbounded();
    let mut query_active = false;
    let poll = futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse();
    pin_mut!(poll);
    loop {
        futures::select! {
            item = live_subscription.next().fuse() => {
                let Some(item) = item else {
                    return Err("pairing statement-store live subscription ended".to_string());
                };
                let value = item.map_err(|err| format!("pairing statement-store live error: {err}"))?;
                if let Some(success) = handle_v2_pairing_result(
                    &value,
                    core_encryption_secret_key,
                    last_processed_statement.as_deref(),
                )? {
                    return Ok(success);
                }
            }
            query = query_rx.next().fuse() => {
                query_active = false;
                if let Some(query) = query
                    && let Some(success) = query? {
                    return Ok(success);
                }
            }
            _ = poll => {
                if !query_active {
                    query_active = true;
                    let rpc_client = rpc_client.clone();
                    let query_tx = query_tx.clone();
                    let last_processed_statement = last_processed_statement.clone();
                    let fut = async move {
                        let result = run_pairing_snapshot_query(
                            rpc_client,
                            topic,
                            core_encryption_secret_key,
                            last_processed_statement,
                        ).await;
                        let _ = query_tx.unbounded_send(result);
                    };
                    // `RpcClient` is transport-only here; spawning lets live
                    // notifications continue to be consumed while a snapshot
                    // query is waiting for backlog completion or timeout.
                    (spawner)(fut.boxed());
                }
                poll.set(futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse());
            }
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.snapshot_query"))]
async fn run_pairing_snapshot_query(
    rpc_client: RpcClient,
    topic: [u8; 32],
    core_encryption_secret_key: [u8; 32],
    last_processed_statement: Option<Vec<u8>>,
) -> Result<Option<PairingSuccess>, String> {
    let topics = [topic];
    let mut subscription = statement_store_rpc::subscribe_match_all(&rpc_client, &topics)
        .await
        .map_err(|err| format!("pairing statement-store query failed: {err}"))?;
    for _ in 0..PAIRING_QUERY_TIMEOUT_TICKS {
        let timeout = futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse();
        pin_mut!(timeout);
        futures::select! {
            item = subscription.next().fuse() => {
                let Some(item) = item else {
                    return Ok(None);
                };
                let value = item.map_err(|err| format!("pairing statement-store query item failed: {err}"))?;
                if let Some(success) = handle_v2_pairing_result(
                    &value,
                    core_encryption_secret_key,
                    last_processed_statement.as_deref(),
                )? {
                    return Ok(Some(success));
                }
                let page = parse_new_statements_result("query".to_string(), &value)
                    .map_err(|err| err.to_string())?;
                if page.remaining == Some(0) {
                    return Ok(None);
                }
            }
            _ = timeout => {}
        }
    }
    Ok(None)
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.handle_result"))]
fn handle_v2_pairing_result(
    value: &Value,
    core_encryption_secret_key: [u8; 32],
    last_processed_statement: Option<&[u8]>,
) -> Result<Option<PairingSuccess>, String> {
    let page =
        parse_new_statements_result("pairing".to_string(), value).map_err(|err| err.to_string())?;
    for statement in page.statements {
        if last_processed_statement == Some(statement.as_slice()) {
            continue;
        }
        if let Some(success) =
            PairingSuccess::from_v2_statement(&statement, core_encryption_secret_key)?
        {
            return Ok(Some(success));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::super::connected_session_ui_info;
    use super::super::{PairingHostRole, ProductRuntimeHost};
    use super::*;
    use crate::host_rpc_client::HostRpcClient;
    use crate::test_support::{
        StubPlatform, core_storage_test_key, pairing_device_from_deeplink, peer_statement_keypair,
        runtime_config, session_info, signed_test_statement, stub_platform, subscribe_ack_frame,
        test_spawner, wallet_handshake_statement,
    };
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use truapi::CallContext;
    use truapi::api::Account;
    use truapi::versioned::account::{
        HostAccountConnectionStatusSubscribeItem, HostRequestLoginRequest,
    };
    use truapi_platform::{AuthState, ChainProvider, CoreStorageKey};

    /// Cancel the login as soon as the host observes the `Pairing` state,
    /// mimicking a user dismissing the pairing UI immediately.
    fn cancel_on_pairing(platform: &StubPlatform, pairing_host: Arc<PairingHostRole>) {
        *platform
            .on_auth_state
            .lock()
            .expect("auth state hook mutex poisoned") = Some(Arc::new(move |state| {
            if matches!(state, AuthState::Pairing { .. }) {
                pairing_host.cancel_login();
            }
        }));
    }

    #[test]
    fn request_login_presents_pairing_and_rejects_when_cancelled() {
        let platform = stub_platform();
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        cancel_on_pairing(&platform, pairing_host);
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
    fn request_login_regenerates_unmarked_pairing_device_identity_between_attempts() {
        let platform = stub_platform();
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        cancel_on_pairing(&platform, pairing_host);
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
                )),
            "cancelled pairing keeps the latest identity; the next unmarked reuse regenerates it"
        );
    }

    #[test]
    fn request_login_regenerates_marked_stored_pairing_device_identity() {
        let platform = stub_platform();
        let identity = generate_pairing_device_identity().unwrap();
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(
                core_storage_test_key(CoreStorageKey::PairingDeviceIdentity),
                identity.encode(),
            );
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(
                core_storage_test_key(CoreStorageKey::LastProcessedPairingStatement),
                vec![0xde, 0xad],
            );
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        cancel_on_pairing(&platform, pairing_host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });

        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        let deeplink = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned")
            .iter()
            .find_map(|state| match state {
                AuthState::Pairing { deeplink } => Some(deeplink.clone()),
                _ => None,
            })
            .expect("pairing state should be emitted");
        assert_ne!(
            pairing_device_from_deeplink(&deeplink),
            (
                identity.statement_store_public_key,
                identity.encryption_public_key
            )
        );
        assert!(
            platform
                .local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .contains_key(&core_storage_test_key(
                    CoreStorageKey::PairingDeviceIdentity
                )),
            "cancelled pairing keeps the rotated identity; the next login rotates again"
        );
    }

    #[test]
    fn request_login_waits_for_pairing_statement() {
        let wallet_ephemeral_secret = p256::SecretKey::from_slice(&[2; 32]).unwrap();
        let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
        let mut wallet_ephemeral_public_bytes = [0u8; 65];
        wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());
        let handshake = VersionedHandshakeResponse::V2 {
            encrypted_message: vec![0xde, 0xad],
            public_key: wallet_ephemeral_public_bytes,
        };
        let statement = signed_test_statement(handshake.encode());
        let notification = format!(
            r#"{{"jsonrpc":"2.0","method":"statement_statement","params":{{"subscription":"remote-sub","result":{{"event":"newStatements","data":{{"statements":["0x{}"],"remaining":0}}}}}}}}"#,
            hex::encode(statement)
        );
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                r#"{"jsonrpc":"2.0","id":"truapi:1","result":"remote-sub"}"#.to_string(),
                notification,
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
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
        let requests = sent_rpc
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .collect::<Vec<_>>();
        let methods = requests
            .iter()
            .map(|request| request["method"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            methods.first().copied(),
            Some("statement_subscribeStatement")
        );
        assert!(
            methods.contains(&"statement_unsubscribeStatement"),
            "pairing subscription should be cleaned up"
        );
        let unsubscribe = requests
            .iter()
            .find(|request| request["method"].as_str() == Some("statement_unsubscribeStatement"))
            .expect("pairing subscription should be cleaned up");
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
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let mut statuses = host.test_session_state().subscribe();
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
            .test_session_state()
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

    #[test]
    fn request_login_surfaces_wallet_failure_status() {
        let session_writes = Arc::new(Mutex::new(Vec::new()));
        let platform = Arc::new(StubPlatform {
            pairing_failure_response: true,
            session_writes: session_writes.clone(),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        let expected_reason =
            "The operation couldn't be completed. (SubstrateSdk.JSONRPCError error 1.)";
        assert_eq!(
            err,
            CallError::HostFailure {
                reason: expected_reason.to_string()
            }
        );
        assert!(
            session_writes
                .lock()
                .expect("session writes mutex poisoned")
                .is_empty()
        );
        let auth_states = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned");
        assert!(
            auth_states
                .iter()
                .any(|state| matches!(state, AuthState::LoginFailed { reason } if reason == expected_reason)),
            "wallet failure should be surfaced to the modal: {auth_states:?}"
        );
    }

    #[test]
    fn request_login_clears_auth_session_when_cancelled_after_persist() {
        let session_writes = Arc::new(Mutex::new(Vec::new()));
        let session_clears = Arc::new(Mutex::new(0));
        let platform = Arc::new(StubPlatform {
            pairing_success_response: true,
            session_writes: session_writes.clone(),
            session_clears: session_clears.clone(),
            ..Default::default()
        });
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let cancel_host = pairing_host.clone();
        *platform
            .on_auth_session_write
            .lock()
            .expect("auth session write hook mutex poisoned") = Some(Arc::new(move || {
            cancel_host.cancel_login();
        }));

        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.test_session_state().current().is_none());
        assert_eq!(
            session_writes
                .lock()
                .expect("session write list mutex poisoned")
                .len(),
            1
        );
        assert_eq!(
            *session_clears
                .lock()
                .expect("session clear counter mutex poisoned"),
            1
        );
    }

    #[test]
    fn request_login_connected_callback_can_clear_session_without_reinstalling_it() {
        let platform = Arc::new(StubPlatform {
            pairing_success_response: true,
            ..Default::default()
        });
        let host = Arc::new(ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        ));
        let disconnect_host = host.clone();
        *platform
            .on_auth_state
            .lock()
            .expect("auth state hook mutex poisoned") = Some(Arc::new(move |state| {
            if matches!(state, AuthState::Connected(_)) {
                disconnect_host.test_session_state().clear_session();
            }
        }));

        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Success)
        );
        assert!(host.test_session_state().current().is_none());

        let auth_states = platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned");
        assert!(matches!(&auth_states[0], AuthState::Pairing { .. }));
        assert!(matches!(&auth_states[1], AuthState::Connected(_)));
    }

    /// Pairing success must also be decoded from a snapshot query page, not only
    /// from the live pairing subscription.
    #[test]
    fn request_login_accepts_pairing_statement_from_snapshot_query_page() {
        let (host_config, _) = runtime_config("myapp.dot");
        let pairing_identity = generate_pairing_device_identity().unwrap();
        let bootstrap =
            create_pairing_bootstrap_from_identity(&host_config, pairing_identity).unwrap();
        let statement = wallet_handshake_statement(&bootstrap.deeplink);
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("truapi:1", "query-sub"),
                crate::test_support::new_statements_frame("query-sub", vec![statement]),
            ],
            ..Default::default()
        });
        let connection =
            futures::executor::block_on(platform.connect(host_config.people_chain_genesis_hash))
                .unwrap();
        let rpc_client = RpcClient::new(HostRpcClient::new(Arc::from(connection), test_spawner()));
        let success = futures::executor::block_on(run_pairing_snapshot_query(
            rpc_client,
            bootstrap.topic,
            bootstrap.encryption_secret_key,
            None,
        ))
        .unwrap()
        .expect("snapshot query should return pairing success");

        assert_eq!(
            success.peer_statement_account_id,
            peer_statement_keypair().1
        );
        assert_eq!(success.success.root_account_id, session_info().public_key);

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
    }

    #[test]
    fn pairing_result_skips_last_processed_statement() {
        let (host_config, _) = runtime_config("myapp.dot");
        let pairing_identity = generate_pairing_device_identity().unwrap();
        let bootstrap =
            create_pairing_bootstrap_from_identity(&host_config, pairing_identity).unwrap();
        let statement = wallet_handshake_statement(&bootstrap.deeplink);
        let page = serde_json::json!({
            "event": "newStatements",
            "data": {
                "statements": [format!("0x{}", hex::encode(&statement))],
                "remaining": 0,
            },
        });

        let ignored = handle_v2_pairing_result(
            &page,
            bootstrap.encryption_secret_key,
            Some(statement.as_slice()),
        )
        .unwrap();
        assert!(ignored.is_none());

        let accepted = handle_v2_pairing_result(&page, bootstrap.encryption_secret_key, None)
            .unwrap()
            .expect("unmarked statement should be accepted");
        assert_eq!(
            accepted.peer_statement_account_id,
            peer_statement_keypair().1
        );
    }

    #[test]
    fn request_login_emits_login_failed_for_pre_pairing_errors() {
        let platform = Arc::new(StubPlatform {
            local_storage_error: Some("identity storage unavailable"),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
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
    fn dropped_request_login_clears_single_flight_for_next_attempt() {
        use std::future::Future;
        use std::task::{Context, Poll};

        let platform = Arc::new(StubPlatform {
            chain_connect_pending: true,
            ..Default::default()
        });
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let cx = CallContext::new();
        let mut first_login = Box::pin(host.request_login(&cx, request.clone()));
        let waker = futures::task::noop_waker();
        let mut task_cx = Context::from_waker(&waker);

        match first_login.as_mut().poll(&mut task_cx) {
            Poll::Pending => {}
            Poll::Ready(result) => panic!("first login should be pending, got {result:?}"),
        }
        assert!(
            platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .iter()
                .any(|state| matches!(state, AuthState::Pairing { .. })),
            "first login did not enter pairing state"
        );

        drop(first_login);

        assert!(
            platform
                .pending_connect_dropped
                .load(std::sync::atomic::Ordering::SeqCst),
            "dropping the login future should drop the pending statement-store connect"
        );

        cancel_on_pairing(&platform, pairing_host);
        let second_cx = CallContext::new();
        let mut second_login = Box::pin(host.request_login(&second_cx, request));
        let second = match second_login.as_mut().poll(&mut task_cx) {
            Poll::Ready(result) => result.expect("second login should complete after cancellation"),
            Poll::Pending => panic!("second login stayed pending behind stale single-flight state"),
        };
        assert_eq!(
            second,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
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
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        cancel_on_pairing(&platform, pairing_host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.test_session_state().current().is_none());
    }

    #[test]
    fn request_login_ignores_corrupt_persisted_session_before_pairing() {
        let session_clears = Arc::new(Mutex::new(0));
        let platform = Arc::new(StubPlatform {
            session_blob: Some(vec![0xff]),
            session_clears: session_clears.clone(),
            ..Default::default()
        });
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        cancel_on_pairing(&platform, pairing_host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.test_session_state().current().is_none());
        assert_eq!(*session_clears.lock().unwrap(), 0);
    }

    #[test]
    fn request_login_ignores_session_store_failure_before_pairing() {
        let platform = Arc::new(StubPlatform {
            session_error: Some("storage failed"),
            ..Default::default()
        });
        let (host, pairing_host) =
            ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
        let host = Arc::new(host);
        cancel_on_pairing(&platform, pairing_host);
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.test_session_state().current().is_none());
    }

    #[test]
    fn request_login_returns_already_connected_when_session_exists() {
        let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.test_session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();
        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::AlreadyConnected)
        );
    }
}
