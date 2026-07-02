//! Deterministic, in-process mock **wallet** for the SSO / statement-store seam.
//!
//! [`truapi_platform::mock::MockPlatform`] mocks the OS/platform seam; this
//! module mocks the *wallet* seam, so login and signing complete with no device
//! and no network. It answers the People-chain statement-store RPC that the core
//! opens through `ChainProvider::connect`, posting sr25519-signed and
//! P-256/AES-GCM-encrypted statements exactly as a paired wallet would.
//!
//! Crate-graph note: the SSO / statement-store crypto lives in `truapi-server`
//! and `truapi-platform` cannot depend on it, so the mock wallet lives here and
//! is composed with `MockPlatform` through [`MockWalletPlatform`], which
//! delegates the ten platform-seam capabilities to an inner `MockPlatform` and
//! answers `ChainProvider::connect` with the wallet connection.
//!
//! The deterministic key material and response frames are promoted verbatim from
//! the runtime test fixtures, so this shares the exact behaviour those tests
//! prove.
//!
//! **Scope:** the returned signatures are valid *in-process* — the core accepts
//! them because the mock plays both sides of the seam — but they are fixed and
//! **not chain-valid**. Exercising a real on-chain signature needs a genuine
//! signer / signer-bot, which is out of scope for this in-process mock.

use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use futures::stream::{self, BoxStream};
use hkdf::Hkdf;
use p256::PublicKey as P256PublicKey;
use p256::SecretKey as P256SecretKey;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, MiniSecretKey};
use sha2::Sha256;
use truapi::v01;
use truapi_platform::mock::{MockConfig, MockPlatform};
use truapi_platform::{
    AuthPresenter, AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features,
    JsonRpcConnection, Navigation, Notifications, Permissions, PreimageHost, ProductStorage,
    ThemeHost, UserConfirmation, UserConfirmationReview, async_trait,
};

use crate::host_logic::session::{SessionInfo, SsoSessionInfo};
use crate::host_logic::sso::messages::{
    RemoteMessage, RemoteMessageData, RemoteMessageV1, SigningPayloadResponseData, SigningResponse,
};
use crate::host_logic::sso::pairing::{
    AES_GCM_NONCE_LEN, EncryptedHandshakeResponseV2, HandshakeSuccessV2, SsoStatementData,
    VersionedHandshakeProposal, VersionedHandshakeResponse,
    encrypt_session_statement_data_with_nonce,
};
use crate::host_logic::statement_store::{StatementField, sign_statement_fields};

// ---------------------------------------------------------------------------
// Deterministic key + session material (promoted from the runtime test fixtures)
// ---------------------------------------------------------------------------

/// Basic connected session fixture (root account + identity), without SSO material.
fn base_session_info() -> SessionInfo {
    SessionInfo {
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

/// A connected session fixture with deterministic SSO channel material. This is
/// the session the mock wallet and the core share for signing.
pub fn mock_wallet_session() -> SessionInfo {
    let mut session = base_session_info();
    let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    let (_, peer_public_key) = peer_statement_keypair();
    let core_secret = P256SecretKey::from_slice(&[1; 32]).unwrap();
    let peer_secret = P256SecretKey::from_slice(&[2; 32]).unwrap();
    session.sso = Some(SsoSessionInfo {
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

/// Deterministic wallet statement-store signing keypair (secret, public).
fn peer_statement_keypair() -> ([u8; 64], [u8; 32]) {
    let mini_secret = MiniSecretKey::from_bytes(&[9; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    (keypair.secret.to_bytes(), keypair.public.to_bytes())
}

/// SCALE-encoded statement signed by the deterministic wallet keypair.
fn signed_statement(data: Vec<u8>) -> Vec<u8> {
    let (secret, public) = peer_statement_keypair();
    sign_statement_fields(secret, public, vec![StatementField::Data(data)])
        .unwrap()
        .encode()
}

// ---------------------------------------------------------------------------
// JSON-RPC frame builders
// ---------------------------------------------------------------------------

fn subscribe_ack_frame(request_id: &str, subscription_id: &str) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": request_id, "result": subscription_id }).to_string()
}

fn statement_submit_ack_frame(request_id: &str) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": request_id, "result": "0xok" }).to_string()
}

fn new_statements_frame(subscription_id: &str, statements: Vec<Vec<u8>>) -> String {
    let statements = statements
        .into_iter()
        .map(|statement| format!("0x{}", hex::encode(statement)))
        .collect::<Vec<_>>();
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "statement_subscribeStatement",
        "params": {
            "subscription": subscription_id,
            "result": { "event": "newStatements", "data": { "statements": statements, "remaining": 0 } },
        },
    })
    .to_string()
}

fn sso_statement(session: &SessionInfo, data: SsoStatementData, nonce_seed: u8) -> Vec<u8> {
    let mut nonce = [0; AES_GCM_NONCE_LEN];
    nonce[0] = nonce_seed;
    let encrypted =
        encrypt_session_statement_data_with_nonce(session.sso.as_ref().unwrap(), &data, nonce)
            .unwrap();
    signed_statement(encrypted)
}

fn core_encryption_public_key_from_deeplink(deeplink: &str) -> [u8; 65] {
    let encoded = deeplink
        .split("handshake=")
        .nth(1)
        .expect("pairing deeplink should include handshake");
    let handshake = hex::decode(encoded).expect("handshake should be hex");
    let VersionedHandshakeProposal::V2(proposal) =
        VersionedHandshakeProposal::decode(&mut handshake.as_slice())
            .expect("handshake should decode");
    proposal.device.encryption_public_key
}

/// Build the wallet's handshake response statement for a pairing deeplink.
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
    let answer = EncryptedHandshakeResponseV2::Success(Box::new(HandshakeSuccessV2 {
        identity_account_id: peer_statement_keypair().1,
        root_account_id: base_session_info().public_key,
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
    let nonce = [0x44; AES_GCM_NONCE_LEN];
    let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
    let mut encrypted_message = nonce.to_vec();
    encrypted_message.extend(
        cipher
            .encrypt(Nonce::from_slice(&nonce), answer.encode().as_slice())
            .unwrap(),
    );
    let handshake = VersionedHandshakeResponse::V2 {
        encrypted_message,
        public_key: wallet_ephemeral_public_bytes,
    };
    signed_statement(handshake.encode())
}

/// A signing response message (`SignResponse`) for the given request id.
pub fn sign_response_message(
    message_id: &str,
    signature: Vec<u8>,
    signed_transaction: Option<Vec<u8>>,
) -> RemoteMessage {
    RemoteMessage {
        message_id: format!("wallet-{message_id}"),
        data: RemoteMessageData::V1(RemoteMessageV1::SignResponse(SigningResponse {
            responding_to: message_id.to_string(),
            payload: Ok(SigningPayloadResponseData {
                signature,
                signed_transaction,
            }),
        })),
    }
}

/// The JSON-RPC response sequence for a successful SSO request/response exchange:
/// an ack on the own channel plus the wallet's `response` on the peer channel.
pub fn sso_success_responses(
    session: &SessionInfo,
    message_id: &str,
    response: RemoteMessage,
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
                SsoStatementData::Response {
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
                SsoStatementData::Request {
                    request_id: format!("wallet-response-{message_id}"),
                    data: vec![response.encode()],
                },
                2,
            )],
        ),
    ]
}

// ---------------------------------------------------------------------------
// The mock wallet connection (promoted from the runtime `RecordingConnection`)
// ---------------------------------------------------------------------------

fn json_rpc_id(frame: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(frame).ok()?;
    match value.get("id")? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn first_pairing_deeplink(auth_states: &Arc<Mutex<Vec<AuthState>>>) -> Option<String> {
    auth_states
        .lock()
        .expect("auth state list mutex poisoned")
        .iter()
        .find_map(|state| match state {
            AuthState::Pairing { deeplink } => Some(deeplink.clone()),
            _ => None,
        })
}

async fn wait_for_statement_subscribe_id(sent: Arc<Mutex<Vec<String>>>, index: usize) -> String {
    for _ in 0..1000 {
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

async fn wait_for_matching_request_id(sent: Arc<Mutex<Vec<String>>>, response: &str) {
    let Some(id) = json_rpc_id(response) else {
        return;
    };
    for _ in 0..1000 {
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

/// A `JsonRpcConnection` that answers the core's statement-store RPC as the
/// paired wallet: it acknowledges subscriptions, injects the handshake response
/// on the pairing topic (login), and replays scripted request/response frames
/// gated on the matching outbound request id (signing).
struct MockWalletConnection {
    sent: Arc<Mutex<Vec<String>>>,
    responses: Vec<String>,
    auth_states: Arc<Mutex<Vec<AuthState>>>,
    login: bool,
}

impl JsonRpcConnection for MockWalletConnection {
    fn send(&self, request: String) {
        self.sent
            .lock()
            .expect("rpc list mutex poisoned")
            .push(request);
    }

    fn responses(&self) -> BoxStream<'static, String> {
        if self.login {
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
                            for _ in 0..1000 {
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
            return Box::pin(futures::stream::pending());
        }
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

    fn close(&self) {}
}

// ---------------------------------------------------------------------------
// The mock wallet platform (Option A: delegate 10 caps, own ChainProvider)
// ---------------------------------------------------------------------------

/// How the mock wallet answers the core over the SSO channel.
#[derive(Clone, Default)]
pub struct MockWalletConfig {
    /// When set, the chain connection drives a successful login handshake.
    pub login: bool,
    /// Scripted request/response frames for a signing exchange (see
    /// [`sso_success_responses`]). Ignored when `login` is set.
    pub responses: Vec<String>,
}

/// A `Platform` that mocks both seams: the platform seam via an inner
/// [`MockPlatform`], and the wallet seam via [`MockWalletConnection`] returned
/// from `ChainProvider::connect`. Build the real core from this to exercise
/// login and signing end to end, deterministically and in-process.
pub struct MockWalletPlatform {
    inner: MockPlatform,
    config: MockWalletConfig,
    sent: Arc<Mutex<Vec<String>>>,
    auth_states: Arc<Mutex<Vec<AuthState>>>,
}

impl MockWalletPlatform {
    /// A mock host whose chain connection completes a login handshake.
    pub fn for_login() -> Self {
        Self::new(
            MockConfig::default(),
            MockWalletConfig {
                login: true,
                responses: vec![],
            },
        )
    }

    /// A mock host whose chain connection replays the given signing frames.
    pub fn for_signing(responses: Vec<String>) -> Self {
        Self::new(
            MockConfig::default(),
            MockWalletConfig {
                login: false,
                responses,
            },
        )
    }

    /// Build with explicit platform and wallet configuration.
    pub fn new(platform: MockConfig, wallet: MockWalletConfig) -> Self {
        Self {
            inner: MockPlatform::with_config(platform),
            config: wallet,
            sent: Arc::new(Mutex::new(Vec::new())),
            auth_states: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The RPC the core sent toward the wallet, in order (assertion oracle).
    pub fn sent_rpc(&self) -> Vec<String> {
        self.sent.lock().expect("rpc list mutex poisoned").clone()
    }
}

#[async_trait]
impl ProductStorage for MockWalletPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        self.inner.read(key).await
    }
    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        self.inner.write(key, value).await
    }
    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        self.inner.clear(key).await
    }
}

#[async_trait]
impl CoreStorage for MockWalletPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.inner.read_core_storage(key).await
    }
    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.inner.write_core_storage(key, value).await
    }
    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        self.inner.clear_core_storage(key).await
    }
}

#[async_trait]
impl Navigation for MockWalletPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.inner.navigate_to(url).await
    }
}

#[async_trait]
impl Notifications for MockWalletPlatform {
    async fn push_notification(
        &self,
        request: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.inner.push_notification(request).await
    }
    async fn cancel_notification(&self, id: v01::NotificationId) -> Result<(), v01::GenericError> {
        self.inner.cancel_notification(id).await
    }
}

#[async_trait]
impl Permissions for MockWalletPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        self.inner.device_permission(request).await
    }
    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        self.inner.remote_permission(request).await
    }
}

#[async_trait]
impl Features for MockWalletPlatform {
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        self.inner.feature_supported(request).await
    }
}

#[async_trait]
impl ChainProvider for MockWalletPlatform {
    async fn connect(
        &self,
        _genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        Ok(Box::new(MockWalletConnection {
            sent: self.sent.clone(),
            responses: self.config.responses.clone(),
            auth_states: self.auth_states.clone(),
            login: self.config.login,
        }))
    }
}

impl AuthPresenter for MockWalletPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        self.auth_states
            .lock()
            .expect("auth state list mutex poisoned")
            .push(state.clone());
        self.inner.auth_state_changed(state);
    }
}

#[async_trait]
impl UserConfirmation for MockWalletPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        self.inner.confirm_user_action(review).await
    }
}

impl ThemeHost for MockWalletPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        self.inner.subscribe_theme()
    }
}

#[async_trait]
impl PreimageHost for MockWalletPlatform {
    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        self.inner.submit_preimage(value).await
    }
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        self.inner.lookup_preimage(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::PlatformRuntimeHost;
    use crate::test_support::{account_id, runtime_config, sign_payload_data, test_spawner};
    use futures::executor::block_on;
    use truapi::CallContext;
    use truapi::api::{Account, Signing};
    use truapi::versioned::account::{HostRequestLoginRequest, HostRequestLoginResponse};
    use truapi::versioned::signing::{HostSignPayloadRequest, HostSignPayloadResponse};

    /// The mock wallet completes the SSO pairing handshake in-process: the core's
    /// `request_login` reaches an authenticated session with no device.
    #[test]
    fn mock_wallet_completes_login() {
        let platform = Arc::new(MockWalletPlatform::for_login());
        let host = PlatformRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = block_on(host.request_login(&cx, request)).expect("login should succeed");
        assert!(matches!(response, HostRequestLoginResponse::V1(_)));
    }

    /// The mock wallet answers a `sign_payload` over the SSO channel: the core's
    /// confirm gate passes, the request is submitted, and the wallet's signature
    /// flows back — all in-process.
    #[test]
    fn mock_wallet_signs_payload() {
        let session = mock_wallet_session();
        let responses = sso_success_responses(
            &session,
            "sign-payload-1",
            sign_response_message("sign-payload-1", vec![8, 8], Some(vec![0xab, 0xcd])),
        );
        let platform = Arc::new(MockWalletPlatform::for_signing(responses));
        let host = PlatformRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session);
        let cx = CallContext::with_request_id("sign-payload-1".to_string());
        let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account: account_id("myapp.dot", 0),
            payload: sign_payload_data(),
        });
        let response = block_on(host.sign_payload(&cx, request)).expect("sign should succeed");
        let HostSignPayloadResponse::V1(inner) = response;
        assert_eq!(inner.signature, vec![8, 8]);
        assert_eq!(inner.signed_transaction, Some(vec![0xab, 0xcd]));
    }

    /// With `confirm_user_actions = false` the core rejects at the confirmation
    /// gate, before the request ever reaches the wallet — so no wallet response
    /// is needed and no signature is produced.
    #[test]
    fn mock_wallet_rejects_sign_when_confirmation_denied() {
        let session = mock_wallet_session();
        let platform = Arc::new(MockWalletPlatform::new(
            MockConfig {
                confirm_user_actions: false,
                ..Default::default()
            },
            MockWalletConfig::default(),
        ));
        let host = PlatformRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session);
        let cx = CallContext::with_request_id("sign-payload-2".to_string());
        let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account: account_id("myapp.dot", 0),
            payload: sign_payload_data(),
        });
        block_on(host.sign_payload(&cx, request)).expect_err("denied confirmation must reject");
    }
}
