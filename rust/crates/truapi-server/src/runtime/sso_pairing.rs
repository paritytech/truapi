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
use super::statement_store_rpc::{self, StatementStoreRpc};
use super::{PlatformRuntimeHost, connected_session_ui_info};
use crate::host_logic::session::{SessionInfo, encode_persisted_session};
use crate::host_logic::sso::pairing::{
    EncryptedHandshakeResponseV2, PairingBootstrap, PairingDeviceIdentity,
    VersionedHandshakeResponse, create_pairing_bootstrap_from_identity, decode_app_handshake_data,
    decrypt_v2_handshake_response, establish_sso_session_info, generate_pairing_device_identity,
};
use crate::host_logic::statement_store::{
    decode_verified_statement_data, parse_new_statements_result,
};
use crate::subscription::Spawner;

use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, StreamExt, pin_mut};
use parity_scale_codec::Encode;
use serde_json::Value;
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::RpcSubscription;
use tracing::{debug, info, instrument};
use truapi::CallError;
use truapi::v01;
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi_platform::{CoreStorage, CoreStorageKey};

#[cfg(not(test))]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_millis(1);
#[cfg(not(test))]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 15;
#[cfg(test)]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 10;

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
struct AbandonedPairingGuard {
    auth_state: AuthStateMachine,
    epoch: u64,
}

impl Drop for AbandonedPairingGuard {
    fn drop(&mut self) {
        self.auth_state.reset_abandoned_pairing(self.epoch);
    }
}

impl PlatformRuntimeHost {
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
        let statement_store = StatementStoreRpc::new(
            self.platform.clone(),
            self.runtime_config.people_chain_genesis_hash,
            self.spawner.clone(),
        );
        let statement_store_connect = statement_store.client("pairing statement-store").fuse();
        pin_mut!(statement_store_connect);

        let rpc_client = futures::select! {
            _ = cancel => return Ok(PairingFlowOutcome::Cancelled),
            connect_result = statement_store_connect => connect_result?,
        };
        let subscribe_client = rpc_client.clone();
        let live_topics = [bootstrap.topic];
        let live_subscription =
            statement_store_rpc::subscribe_match_all(&subscribe_client, &live_topics).fuse();
        pin_mut!(live_subscription);
        let live_subscription = futures::select! {
            _ = cancel => return Ok(PairingFlowOutcome::Cancelled),
            subscribe_result = live_subscription => subscribe_result
                .map_err(|err| format!("pairing statement-store subscribe failed: {err}"))?,
        };
        debug!("subscribed to pairing topic, polling statement store");
        let pairing_response = wait_for_v2_pairing_success(
            rpc_client,
            live_subscription,
            bootstrap.topic,
            bootstrap.encryption_secret_key,
            self.spawner.clone(),
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
        self.platform
            .write_core_storage(
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

struct PairingSuccess {
    peer_statement_account_id: [u8; 32],
    success: crate::host_logic::sso::pairing::HandshakeSuccessV2,
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.wait_success"))]
async fn wait_for_v2_pairing_success(
    rpc_client: RpcClient,
    mut live_subscription: RpcSubscription<Value>,
    topic: [u8; 32],
    core_encryption_secret_key: [u8; 32],
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
                if let Some(success) = handle_v2_pairing_result(&value, core_encryption_secret_key)? {
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
                    let fut = async move {
                        let result = run_pairing_snapshot_query(
                            rpc_client,
                            topic,
                            core_encryption_secret_key,
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
                if let Some(success) = handle_v2_pairing_result(&value, core_encryption_secret_key)? {
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
) -> Result<Option<PairingSuccess>, String> {
    let page =
        parse_new_statements_result("pairing".to_string(), value).map_err(|err| err.to_string())?;
    for statement in page.statements {
        if let Some(success) = decode_v2_pairing_statement(&statement, core_encryption_secret_key)?
        {
            return Ok(Some(success));
        }
    }

    Ok(None)
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.decode_statement"))]
fn decode_v2_pairing_statement(
    statement: &[u8],
    core_encryption_secret_key: [u8; 32],
) -> Result<Option<PairingSuccess>, String> {
    let verified =
        decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
    let VersionedHandshakeResponse::V2 {
        encrypted_message,
        public_key,
    } = decode_app_handshake_data(&verified.data)?;
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
    fn cancel_on_pairing(platform: &StubPlatform, host: &Arc<PlatformRuntimeHost>) {
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
        let handshake = crate::host_logic::sso::pairing::VersionedHandshakeResponse::V2 {
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
                r#"{"jsonrpc":"2.0","id":"truapi:1","result":"remote-sub"}"#.to_string(),
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

        let methods = platform
            .sent_rpc
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["method"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(
            methods
                .iter()
                .filter(|method| method.as_str() == "statement_subscribeStatement")
                .count()
                >= 2,
            "core should issue snapshot queries while pairing: {methods:?}"
        );
        assert!(
            methods
                .iter()
                .any(|method| method == "statement_unsubscribeStatement"),
            "drained query subscription should be cleaned up: {methods:?}"
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
