//! Shared fixtures for the runtime test modules: a stub platform, a
//! recording json-rpc connection, and SSO statement/frame builders.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::host_logic::sso::pairing;
use crate::subscription::Spawner;
#[cfg(not(target_arch = "wasm32"))]
use crate::subscription::thread_per_subscription_spawner;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use futures::stream::{self, BoxStream};
use futures::Stream;
use hkdf::Hkdf;
use p256::PublicKey as P256PublicKey;
use p256::SecretKey as P256SecretKey;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, MiniSecretKey};
use sha2::Sha256;
use truapi::v01;
use truapi::versioned::account::HostAccountGetAliasRequest;
use truapi::versioned::resource_allocation::HostRequestResourceAllocationRequest;
use truapi_platform::{
    AuthPresenter, AuthState, ChainProvider, CoreStorage as PlatformCoreStorage, CoreStorageKey,
    Features as PlatformFeatures, HostInfo, JsonRpcConnection, Navigation as PlatformNavigation,
    Notifications as PlatformNotifications, PairingHostConfig, Permissions as PlatformPermissions,
    PlatformInfo, PreimageHost, ProductContext, ProductStorage as PlatformProductStorage,
    ThemeHost, UserConfirmation, UserConfirmationReview,
};

/// Test spawner that matches the current target.
pub(crate) fn test_spawner() -> Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    {
        thread_per_subscription_spawner()
    }
    #[cfg(target_arch = "wasm32")]
    {
        immediate_spawner()
    }
}

/// Synchronous spawner for tests that should complete work immediately.
#[cfg(target_arch = "wasm32")]
pub(crate) fn immediate_spawner() -> Spawner {
    Arc::new(futures::executor::block_on)
}

/// Test hook invoked after each recorded auth state.
pub type AuthStateHook = Arc<dyn Fn(&AuthState) + Send + Sync>;

/// Minimal Platform impl that only answers `feature_supported`. Every
/// other callback returns a unit value or empty stream, so the runtime
/// can exercise its delegation paths without pulling in a real backend.
#[derive(Default)]
pub(crate) struct StubPlatform {
    pub(crate) remote_permission_denied: bool,
    pub(crate) account_alias_confirmed: bool,
    pub(crate) account_alias_error: Option<&'static str>,
    pub(crate) identity_disclosure_confirmed: bool,
    pub(crate) identity_disclosure_error: Option<&'static str>,
    pub(crate) identity_disclosure_calls: Arc<AtomicUsize>,
    pub(crate) sign_payload_confirmed: bool,
    pub(crate) sign_payload_error: Option<&'static str>,
    pub(crate) sign_raw_confirmed: bool,
    pub(crate) sign_raw_error: Option<&'static str>,
    pub(crate) create_transaction_confirmed: bool,
    pub(crate) create_transaction_error: Option<&'static str>,
    pub(crate) resource_allocation_confirmed: bool,
    pub(crate) resource_allocation_error: Option<&'static str>,
    pub(crate) session_blob: Option<Vec<u8>>,
    pub(crate) session_error: Option<&'static str>,
    pub(crate) session_clears: Arc<Mutex<usize>>,
    pub(crate) session_writes: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Every `auth_state_changed` emission in order.
    pub(crate) auth_states: Arc<Mutex<Vec<AuthState>>>,
    /// Invoked after each recorded auth state, outside any stub lock, so a
    /// test can react to a transition (e.g. cancel the login it observes).
    pub(crate) on_auth_state: Arc<Mutex<Option<AuthStateHook>>>,
    /// Set when a `chain_connect_pending` connect future is dropped, which is
    /// how a dropped login flow manifests on the stub.
    pub(crate) pending_connect_dropped: Arc<AtomicBool>,
    /// When true, `subscribe_theme` returns a never-ending stream.
    pub(crate) theme_stream_pending: bool,
    /// Set when the pending theme stream is dropped.
    pub(crate) theme_stream_dropped: Arc<AtomicBool>,
    pub(crate) pairing_success_response: bool,
    /// Deliver the pairing success statement only through a snapshot
    /// query page; the live subscription stays silent.
    pub(crate) pairing_success_via_query: bool,
    pub(crate) notification_id: v01::NotificationId,
    pub(crate) pushed_notifications: Arc<Mutex<Vec<v01::HostPushNotificationRequest>>>,
    pub(crate) cancelled_notifications: Arc<Mutex<Vec<v01::NotificationId>>>,
    pub(crate) sent_rpc: Arc<Mutex<Vec<String>>>,
    pub(crate) rpc_responses: Vec<String>,
    pub(crate) chain_connect_error: Option<&'static str>,
    pub(crate) chain_connect_pending: bool,
    pub(crate) preimage_submits: Arc<Mutex<Vec<Vec<u8>>>>,
    pub(crate) local_storage: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    /// When set, product/core storage reads fail with this reason.
    pub(crate) local_storage_error: Option<&'static str>,
}

struct DropFlagGuard(Arc<AtomicBool>);

impl Drop for DropFlagGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct PendingThemeStream {
    dropped: Arc<AtomicBool>,
}

impl Drop for PendingThemeStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl Stream for PendingThemeStream {
    type Item = Result<v01::ThemeVariant, v01::GenericError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

/// First `Pairing` deeplink recorded on `auth_states`, if any.
pub(crate) fn first_pairing_deeplink(auth_states: &Mutex<Vec<AuthState>>) -> Option<String> {
    auth_states
        .lock()
        .expect("auth state list mutex poisoned")
        .iter()
        .find_map(|state| match state {
            AuthState::Pairing { deeplink } => Some(deeplink.clone()),
            _ => None,
        })
}

/// Default stub platform wrapped in an `Arc`.
pub(crate) fn stub_platform() -> Arc<StubPlatform> {
    Arc::new(StubPlatform::default())
}

/// Runtime configuration used by platform-backed runtime tests.
pub(crate) fn runtime_config(product_id: &str) -> (PairingHostConfig, ProductContext) {
    (
        PairingHostConfig::new(
            HostInfo {
                name: "Polkadot Web".to_string(),
                icon: Some("https://example.invalid/dotli.png".to_string()),
                version: None,
            },
            PlatformInfo::default(),
            [0; 32],
            "polkadotapp".to_string(),
        )
        .expect("test host runtime config is valid"),
        ProductContext::new(product_id.to_string()).expect("test product context is valid"),
    )
}

/// Basic connected session fixture without SSO channel material.
pub(crate) fn session_info() -> crate::host_logic::session::SessionInfo {
    crate::host_logic::session::SessionInfo {
        public_key: [
            0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d,
            0xb8, 0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab,
            0xc1, 0xd7, 0x20, 0x23,
        ],
        sso: None,
        root_entropy_source: Some([
            0x15, 0xcb, 0x94, 0x34, 0x84, 0x0b, 0x56, 0xbe, 0x1f, 0xdd, 0x91, 0xc4, 0x6a, 0x13,
            0xf5, 0x20, 0xf4, 0x91, 0x61, 0x2e, 0xa5, 0xd6, 0x06, 0x92, 0x0d, 0x91, 0x38, 0xe8,
            0xbd, 0xd6, 0x3c, 0xb0,
        ]),
        identity_account_id: Some([
            0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d,
            0xb8, 0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab,
            0xc1, 0xd7, 0x20, 0x23,
        ]),
        lite_username: Some("alice".to_string()),
        full_username: Some("Alice Smith".to_string()),
    }
}

/// Connected session fixture with deterministic SSO channel material.
pub(crate) fn sso_session_info() -> crate::host_logic::session::SessionInfo {
    let mut session = session_info();
    let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    let (_, peer_public_key) = peer_statement_keypair();
    let core_secret = P256SecretKey::from_slice(&[1; 32]).unwrap();
    let peer_secret = P256SecretKey::from_slice(&[2; 32]).unwrap();
    session.sso = Some(crate::host_logic::session::SsoSessionInfo {
        ss_secret: keypair.secret.to_bytes(),
        ss_public_key: keypair.public.to_bytes(),
        enc_secret: core_secret.to_bytes().into(),
        peer_enc_pubkey: peer_secret
            .public_key()
            .to_encoded_point(false)
            .as_bytes()
            .try_into()
            .unwrap(),
        identity_account_id: peer_public_key,
        session_id_own: [4; 32],
        session_id_peer: [5; 32],
        request_channel: [6; 32],
        response_channel: [7; 32],
        peer_request_channel: [8; 32],
    });
    session.root_entropy_source = Some(keypair.secret.to_bytes()[..32].try_into().unwrap());
    session
}

/// Deterministic peer statement-store signing keypair.
pub(crate) fn peer_statement_keypair() -> ([u8; 64], [u8; 32]) {
    let mini_secret = MiniSecretKey::from_bytes(&[9; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    (keypair.secret.to_bytes(), keypair.public.to_bytes())
}

/// SCALE-encoded statement signed by the deterministic peer keypair.
pub(crate) fn signed_test_statement(data: Vec<u8>) -> Vec<u8> {
    let (secret, public) = peer_statement_keypair();
    crate::host_logic::statement_store::sign_statement_fields(
        secret,
        public,
        vec![crate::host_logic::statement_store::StatementField::Data(
            data,
        )],
    )
    .unwrap()
    .encode()
}

/// Last submitted SSO remote message decoded from the stub RPC log.
pub(crate) fn submitted_remote_message(
    platform: &Arc<StubPlatform>,
    session: &crate::host_logic::session::SessionInfo,
) -> crate::host_logic::sso::messages::RemoteMessage {
    let submit = wait_for_statement_submit(&platform.sent_rpc);
    let value: serde_json::Value = serde_json::from_str(&submit).unwrap();
    let statement_hex = value["params"][0].as_str().unwrap();
    let statement = hex::decode(statement_hex.strip_prefix("0x").unwrap_or(statement_hex)).unwrap();
    let encrypted = crate::host_logic::statement_store::decode_statement_data(&statement)
        .expect("statement data should decode");
    let data = pairing::decrypt_session_statement_data(session.sso.as_ref().unwrap(), &encrypted)
        .expect("statement data should decrypt");
    let pairing::SsoStatementData::Request { data, .. } = data else {
        panic!("expected request statement data");
    };
    crate::host_logic::sso::messages::RemoteMessage::decode(&mut data[0].as_slice())
        .expect("remote message should decode")
}

fn wait_for_statement_submit(sent: &Arc<Mutex<Vec<String>>>) -> String {
    for _ in 0..100 {
        if let Some(request) = sent
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .rev()
            .find(|request| request.contains("\"statement_submit\""))
            .cloned()
        {
            return request;
        }
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(Duration::from_millis(1));
        #[cfg(target_arch = "wasm32")]
        futures::executor::block_on(futures_timer::Delay::new(Duration::from_millis(1)));
    }
    panic!("statement_submit request should be sent");
}

/// JSON-RPC response sequence for a successful SSO request/response exchange.
pub(crate) fn sso_success_responses(
    session: &crate::host_logic::session::SessionInfo,
    message_id: &str,
    response: crate::host_logic::sso::messages::RemoteMessage,
) -> Vec<String> {
    let own_subscription_id = format!("own-sub-{message_id}");
    let peer_subscription_id = format!("peer-sub-{message_id}");
    vec![
        subscribe_ack_frame("truapi:1", &own_subscription_id),
        subscribe_ack_frame("truapi:2", &peer_subscription_id),
        statement_submit_ack_frame("truapi:3"),
        new_statements_frame(
            &own_subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Response {
                    request_id: message_id.to_string(),
                    response_code: 0,
                },
                1,
            )],
        ),
        new_statements_frame(
            &peer_subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Request {
                    request_id: format!("wallet-response-{message_id}"),
                    data: vec![response.encode()],
                },
                2,
            )],
        ),
    ]
}

/// JSON-RPC response sequence where the SSO peer sends `Disconnected`.
pub(crate) fn sso_peer_disconnect_responses(
    session: &crate::host_logic::session::SessionInfo,
    message_id: &str,
) -> Vec<String> {
    let own_subscription_id = format!("own-sub-{message_id}");
    let peer_subscription_id = format!("peer-sub-{message_id}");
    vec![
        subscribe_ack_frame("truapi:1", &own_subscription_id),
        subscribe_ack_frame("truapi:2", &peer_subscription_id),
        statement_submit_ack_frame("truapi:3"),
        new_statements_frame(
            &own_subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Response {
                    request_id: message_id.to_string(),
                    response_code: 0,
                },
                1,
            )],
        ),
        new_statements_frame(
            &peer_subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Request {
                    request_id: format!("wallet-disconnect-{message_id}"),
                    data: vec![
                        crate::host_logic::sso::messages::RemoteMessage {
                            message_id: format!("wallet-disconnect-{message_id}"),
                            data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                                crate::host_logic::sso::messages::v1::RemoteMessage::Disconnected,
                            ),
                        }
                        .encode(),
                    ],
                },
                2,
            )],
        ),
    ]
}

/// JSON-RPC response sequence for the background peer-disconnect monitor.
pub(crate) fn sso_peer_disconnect_monitor_responses(
    session: &crate::host_logic::session::SessionInfo,
) -> Vec<String> {
    let subscription_id = "peer-disconnect-monitor-sub";
    vec![
        subscribe_ack_frame("truapi:1", subscription_id),
        new_statements_frame(
            subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Request {
                    request_id: "wallet-disconnect-monitor".to_string(),
                    data: vec![
                        crate::host_logic::sso::messages::RemoteMessage {
                            message_id: "wallet-disconnect-monitor".to_string(),
                            data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                                crate::host_logic::sso::messages::v1::RemoteMessage::Disconnected,
                            ),
                        }
                        .encode(),
                    ],
                },
                1,
            )],
        ),
    ]
}

/// JSON-RPC subscription acknowledgement frame.
pub(crate) fn subscribe_ack_frame(request_id: &str, subscription_id: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": subscription_id,
    })
    .to_string()
}

fn statement_submit_ack_frame(request_id: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": "0xok",
    })
    .to_string()
}

/// JSON-RPC `newStatements` notification carrying SCALE statements.
pub(crate) fn new_statements_frame(subscription_id: &str, statements: Vec<Vec<u8>>) -> String {
    let statements = statements
        .into_iter()
        .map(|statement| format!("0x{}", hex::encode(statement)))
        .collect::<Vec<_>>();
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "statement_subscribeStatement",
        "params": {
            "subscription": subscription_id,
            "result": {
                "event": "newStatements",
                "data": {
                    "statements": statements,
                    "remaining": 0,
                },
            },
        },
    })
    .to_string()
}

fn sso_statement(
    session: &crate::host_logic::session::SessionInfo,
    data: pairing::SsoStatementData,
    nonce_seed: u8,
) -> Vec<u8> {
    let mut nonce = [0; pairing::AES_GCM_NONCE_LEN];
    nonce[0] = nonce_seed;
    let encrypted = pairing::encrypt_session_statement_data_with_nonce(
        session.sso.as_ref().unwrap(),
        &data,
        nonce,
    )
    .unwrap();
    signed_test_statement(encrypted)
}

fn core_encryption_public_key_from_deeplink(deeplink: &str) -> [u8; 65] {
    pairing_device_from_deeplink(deeplink).1
}

/// Pairing device statement and encryption keys encoded in a deeplink.
pub(crate) fn pairing_device_from_deeplink(deeplink: &str) -> ([u8; 32], [u8; 65]) {
    let encoded = deeplink
        .split("handshake=")
        .nth(1)
        .expect("pairing deeplink should include handshake");
    let handshake = hex::decode(encoded).expect("handshake should be hex");
    let decoded = pairing::VersionedHandshakeProposal::decode(&mut handshake.as_slice())
        .expect("handshake should decode");
    let pairing::VersionedHandshakeProposal::V2(proposal) = decoded;
    (
        proposal.device.statement_account_id,
        proposal.device.encryption_public_key,
    )
}

fn wallet_handshake_statement(deeplink: &str) -> Vec<u8> {
    let core_public_key =
        P256PublicKey::from_sec1_bytes(&core_encryption_public_key_from_deeplink(deeplink))
            .expect("core encryption public key should decode");
    let wallet_ephemeral_secret = P256SecretKey::from_slice(&[3; 32]).unwrap();
    let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
    let mut wallet_ephemeral_public_bytes = [0u8; 65];
    wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());
    let wallet_persistent_public: [u8; 65] = P256SecretKey::from_slice(&[2; 32])
        .unwrap()
        .public_key()
        .to_encoded_point(false)
        .as_bytes()
        .try_into()
        .unwrap();
    let answer = pairing::v2::EncryptedResponse::Success(Box::new(pairing::v2::Success {
        identity_account_id: peer_statement_keypair().1,
        root_account_id: session_info().public_key,
        identity_chat_private_key: [0x77; 32],
        sso_enc_pub_key: wallet_persistent_public,
        device_enc_pub_key: wallet_persistent_public,
        root_entropy_source: [0x66; 32],
    }));
    let shared_secret = diffie_hellman(
        wallet_ephemeral_secret.to_nonzero_scalar(),
        core_public_key.as_affine(),
    );
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret.raw_secret_bytes());
    let mut aes_key = [0u8; 32];
    hkdf.expand(&[], &mut aes_key).unwrap();
    let nonce = [0x44; pairing::AES_GCM_NONCE_LEN];
    let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
    let mut encrypted_message = nonce.to_vec();
    encrypted_message.extend(
        cipher
            .encrypt(Nonce::from_slice(&nonce), answer.encode().as_slice())
            .unwrap(),
    );
    let handshake = pairing::VersionedHandshakeResponse::V2 {
        encrypted_message,
        public_key: wallet_ephemeral_public_bytes,
    };

    signed_test_statement(handshake.encode())
}

/// SSO signing response message for the given request id.
pub(crate) fn sign_response_message(
    message_id: &str,
    signature: Vec<u8>,
    signed_transaction: Option<Vec<u8>>,
) -> crate::host_logic::sso::messages::RemoteMessage {
    crate::host_logic::sso::messages::RemoteMessage {
        message_id: format!("wallet-{message_id}"),
        data: crate::host_logic::sso::messages::RemoteMessageData::V1(
            crate::host_logic::sso::messages::v1::RemoteMessage::SignResponse(
                crate::host_logic::sso::messages::SigningResponse {
                    responding_to: message_id.to_string(),
                    payload: Ok(
                        crate::host_logic::sso::messages::SigningPayloadResponseData {
                            signature,
                            signed_transaction,
                        },
                    ),
                },
            ),
        ),
    }
}

/// Product account id fixture for `identifier` and derivation slot.
pub(crate) fn account_id(identifier: &str, derivation_index: u32) -> v01::ProductAccountId {
    v01::ProductAccountId {
        dot_ns_identifier: identifier.to_string(),
        derivation_index,
    }
}

/// Account-alias request fixture for a product identifier.
pub(crate) fn account_alias_request(identifier: &str) -> HostAccountGetAliasRequest {
    HostAccountGetAliasRequest::V1(v01::HostAccountGetAliasRequest {
        product_account_id: account_id(identifier, 0),
    })
}

/// Raw signing payload fixture.
pub(crate) fn raw_payload() -> v01::RawPayload {
    v01::RawPayload::Bytes {
        bytes: b"hello".to_vec(),
    }
}

/// Structured signing payload fixture.
pub(crate) fn sign_payload_data() -> v01::HostSignPayloadData {
    v01::HostSignPayloadData {
        block_hash: vec![0; 32],
        block_number: vec![0; 4],
        era: vec![0],
        genesis_hash: vec![1; 32],
        method: vec![0],
        nonce: vec![0],
        spec_version: vec![0],
        tip: vec![0],
        transaction_version: vec![0],
        signed_extensions: vec![],
        version: 4,
        asset_id: None,
        metadata_hash: None,
        mode: None,
        with_signed_transaction: None,
    }
}

/// Product transaction payload fixture for `identifier`.
pub(crate) fn product_tx_payload(identifier: &str) -> v01::ProductAccountTxPayload {
    v01::ProductAccountTxPayload {
        signer: account_id(identifier, 0),
        genesis_hash: [1; 32],
        call_data: vec![0],
        extensions: vec![],
        tx_ext_version: 0,
    }
}

/// Resource-allocation request fixture containing all supported resource kinds.
pub(crate) fn resource_allocation_request() -> HostRequestResourceAllocationRequest {
    HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
        resources: vec![
            v01::AllocatableResource::StatementStoreAllowance,
            v01::AllocatableResource::AutoSigning,
        ],
    })
}

/// Unsigned statement fixture with channel, topics, expiry, and data.
pub(crate) fn statement() -> v01::Statement {
    v01::Statement {
        proof: None,
        decryption_key: None,
        expiry: Some(99),
        channel: Some([1; 32]),
        topics: vec![[2; 32], [3; 32]],
        data: Some(vec![4, 5, 6]),
    }
}

/// Signed statement fixture scoped to `topic`.
pub(crate) fn signed_statement(topic: [u8; 32]) -> v01::SignedStatement {
    v01::SignedStatement {
        proof: v01::StatementProof::Sr25519 {
            signature: [9; 64],
            signer: [8; 32],
        },
        decryption_key: None,
        expiry: Some(99),
        channel: Some([1; 32]),
        topics: vec![topic],
        data: Some(vec![4, 5, 6]),
    }
}

#[truapi_platform::async_trait]
impl PlatformProductStorage for StubPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        if let Some(reason) = self.local_storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            });
        }
        Ok(self
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .get(&key)
            .cloned())
    }
    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        if let Some(reason) = self.local_storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(key, value);
        Ok(())
    }
    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        if let Some(reason) = self.local_storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .remove(&key);
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl PlatformCoreStorage for StubPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        if let CoreStorageKey::AuthSession = key {
            if let Some(reason) = self.session_error {
                return Err(v01::GenericError {
                    reason: reason.to_string(),
                });
            }
            return Ok(self.session_blob.clone());
        }
        if let Some(reason) = self.local_storage_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        Ok(self
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .get(&core_storage_test_key(key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        if let CoreStorageKey::AuthSession = key {
            self.session_writes
                .lock()
                .expect("session write list mutex poisoned")
                .push(value);
            return Ok(());
        }
        if let Some(reason) = self.local_storage_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(core_storage_test_key(key), value);
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        if let CoreStorageKey::AuthSession = key {
            *self
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned") += 1;
            return Ok(());
        }
        if let Some(reason) = self.local_storage_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .remove(&core_storage_test_key(key));
        Ok(())
    }
}

/// Stable string key used by the stub core-storage map.
pub(crate) fn core_storage_test_key(key: CoreStorageKey) -> String {
    format!("core:{}", hex::encode(key.encode()))
}

#[truapi_platform::async_trait]
impl PlatformNavigation for StubPlatform {
    async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl PlatformNotifications for StubPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.pushed_notifications
            .lock()
            .expect("notification list mutex poisoned")
            .push(notification);
        Ok(v01::HostPushNotificationResponse {
            id: self.notification_id,
        })
    }

    async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
        self.cancelled_notifications
            .lock()
            .expect("notification cancellation list mutex poisoned")
            .push(id);
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl PlatformPermissions for StubPlatform {
    async fn device_permission(
        &self,
        _request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        Ok(v01::HostDevicePermissionResponse { granted: true })
    }

    async fn remote_permission(
        &self,
        _request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        Ok(v01::RemotePermissionResponse {
            granted: !self.remote_permission_denied,
        })
    }
}

#[truapi_platform::async_trait]
impl PlatformFeatures for StubPlatform {
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        Ok(v01::HostFeatureSupportedResponse { supported: true })
    }
}

struct RecordingConnection {
    sent: Arc<Mutex<Vec<String>>>,
    responses: Vec<String>,
    auth_states: Arc<Mutex<Vec<AuthState>>>,
    pairing_success_response: bool,
    pairing_success_via_query: bool,
}

async fn wait_for_statement_subscribe_id(sent: Arc<Mutex<Vec<String>>>, index: usize) -> String {
    for _ in 0..100 {
        let ids = sent
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .filter_map(|request| {
                let value: serde_json::Value = serde_json::from_str(request).ok()?;
                (value.get("method")?.as_str()? == "statement_subscribeStatement")
                    .then(|| value.get("id")?.as_str().map(ToString::to_string))?
            })
            .collect::<Vec<_>>();
        if let Some(id) = ids.get(index) {
            return id.clone();
        }
        futures_timer::Delay::new(Duration::from_millis(1)).await;
    }
    panic!("statement_subscribeStatement request {index} was not issued");
}

impl JsonRpcConnection for RecordingConnection {
    fn send(&self, request: String) {
        self.sent
            .lock()
            .expect("rpc list mutex poisoned")
            .push(request);
    }
    fn responses(&self) -> BoxStream<'static, String> {
        if self.pairing_success_via_query {
            let auth_states = self.auth_states.clone();
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let auth_states = auth_states.clone();
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        1 => {
                            let query_id = wait_for_statement_subscribe_id(sent.clone(), 1).await;
                            Some((subscribe_ack_frame(&query_id, "query-sub"), 2))
                        }
                        2 => {
                            for _ in 0..100 {
                                if let Some(deeplink) = first_pairing_deeplink(&auth_states) {
                                    return Some((
                                        new_statements_frame(
                                            "query-sub",
                                            vec![wallet_handshake_statement(&deeplink)],
                                        ),
                                        3,
                                    ));
                                }
                                futures_timer::Delay::new(Duration::from_millis(1)).await;
                            }
                            panic!("pairing deeplink was not presented");
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if self.pairing_success_response {
            let auth_states = self.auth_states.clone();
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let auth_states = auth_states.clone();
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        1 => {
                            for _ in 0..100 {
                                if let Some(deeplink) = first_pairing_deeplink(&auth_states) {
                                    return Some((
                                        new_statements_frame(
                                            "pairing-sub",
                                            vec![wallet_handshake_statement(&deeplink)],
                                        ),
                                        2,
                                    ));
                                }
                                futures_timer::Delay::new(Duration::from_millis(1)).await;
                            }
                            panic!("pairing deeplink was not presented");
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if self.responses.is_empty() {
            Box::pin(futures::stream::pending())
        } else {
            let responses = self.responses.clone();
            let sent = self.sent.clone();
            Box::pin(stream::unfold(0, move |index| {
                let responses = responses.clone();
                let sent = sent.clone();
                async move {
                    let Some(response) = responses.get(index).cloned() else {
                        return futures::future::pending().await;
                    };
                    wait_for_matching_request_id(sent, &response).await;
                    Some((response, index + 1))
                }
            }))
        }
    }

    fn close(&self) {}
}

async fn wait_for_matching_request_id(sent: Arc<Mutex<Vec<String>>>, response: &str) {
    let Some(id) = json_rpc_id(response) else {
        return;
    };
    for _ in 0..100 {
        if sent
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .any(|request| json_rpc_id(request).as_deref() == Some(id.as_str()))
        {
            return;
        }
        futures_timer::Delay::new(Duration::from_millis(1)).await;
    }
    panic!("request {id} was not issued before scripted response");
}

fn json_rpc_id(frame: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(frame).ok()?;
    match value.get("id")? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

#[truapi_platform::async_trait]
impl ChainProvider for StubPlatform {
    async fn connect(
        &self,
        _genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        if let Some(reason) = self.chain_connect_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        if self.chain_connect_pending {
            let _guard = DropFlagGuard(self.pending_connect_dropped.clone());
            futures::future::pending::<()>().await;
        }
        Ok(Box::new(RecordingConnection {
            sent: self.sent_rpc.clone(),
            responses: self.rpc_responses.clone(),
            auth_states: self.auth_states.clone(),
            pairing_success_response: self.pairing_success_response,
            pairing_success_via_query: self.pairing_success_via_query,
        }))
    }
}

impl AuthPresenter for StubPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        self.auth_states
            .lock()
            .expect("auth state list mutex poisoned")
            .push(state.clone());
        let hook = self
            .on_auth_state
            .lock()
            .expect("auth state hook mutex poisoned")
            .clone();
        if let Some(hook) = hook {
            hook(&state);
        }
    }
}

#[truapi_platform::async_trait]
impl UserConfirmation for StubPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        let (error, confirmed) = match review {
            UserConfirmationReview::SignPayload(_) => {
                (self.sign_payload_error, self.sign_payload_confirmed)
            }
            UserConfirmationReview::SignRaw(_) => (self.sign_raw_error, self.sign_raw_confirmed),
            UserConfirmationReview::CreateTransaction(_) => (
                self.create_transaction_error,
                self.create_transaction_confirmed,
            ),
            UserConfirmationReview::AccountAlias(_) => {
                (self.account_alias_error, self.account_alias_confirmed)
            }
            UserConfirmationReview::IdentityDisclosure(_) => {
                self.identity_disclosure_calls
                    .fetch_add(1, Ordering::SeqCst);
                (
                    self.identity_disclosure_error,
                    self.identity_disclosure_confirmed,
                )
            }
            UserConfirmationReview::ResourceAllocation(_) => (
                self.resource_allocation_error,
                self.resource_allocation_confirmed,
            ),
            UserConfirmationReview::PreimageSubmit(_) => (None, true),
        };
        if let Some(reason) = error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        Ok(confirmed)
    }
}

impl ThemeHost for StubPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        if self.theme_stream_pending {
            return Box::pin(PendingThemeStream {
                dropped: self.theme_stream_dropped.clone(),
            });
        }
        Box::pin(stream::once(async { Ok(v01::ThemeVariant::Dark) }))
    }
}

#[truapi_platform::async_trait]
impl PreimageHost for StubPlatform {
    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        self.preimage_submits
            .lock()
            .expect("preimage submit list mutex poisoned")
            .push(value.clone());
        Ok(value)
    }
    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        Box::pin(stream::once(async { Ok(Some(vec![9, 8, 7])) }))
    }
}
