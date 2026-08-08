//! Signing-host responder half of the host-spec §B pairing protocol.
//!
//! Answers a pairing host's handshake proposal (QR/deeplink) with an
//! encrypted `Success` statement, then serves the encrypted SSO session:
//! acks every inbound request statement, dispatches the batched
//! [`v1::RemoteMessage`] requests onto the local signing authority, and posts
//! the response statements the pairing host is waiting for. Runs until the
//! peer sends `Disconnected`, the local session ends, or the transport fails.
//!
//! Sensitive operations consult [`truapi_platform::UserConfirmation`], the
//! same seam browser hosts use for their confirmation modals; a headless host
//! implements it with its approval policy.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use parity_scale_codec::Encode;
use tracing::{debug, instrument, warn};
use truapi::{CallContext, latest as api, v01};
use truapi_platform::{
    CreateTransactionReview, ResourceAllocationReview, SignPayloadReview, SignRawReview,
    UserConfirmationReview,
};

use super::SigningHost;
#[cfg(not(target_arch = "wasm32"))]
use crate::chain_runtime::RuntimeFailure;
use crate::host_logic::entropy::root_entropy_source;
#[cfg(not(target_arch = "wasm32"))]
use crate::host_logic::product_account::{
    ProductAccountError, derive_lite_person_ring_vrf_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::product_account::{
    derive_identity_keypair, derive_root_keypair_from_entropy, product_public_key_to_address,
};
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::messages::{
    self, CreateTransactionPayload, IncomingSsoRequest, OnExistingAllowancePolicy, RemoteMessage,
    RemoteMessageData, ResourceAllocationResponse, RingVrfAliasResponse, RingVrfError,
    RingVrfProofResponse, SignRawLegacyResponse, SignVrfResponse, SigningPayloadResponseData,
    SigningRequest, SigningResponse, SsoAllocatableResource, SsoAllocatedResource,
    SsoAllocationOutcome, SsoResponseCode, build_outgoing_request_statement,
    build_signed_session_response_statement, decode_incoming_sso_request, v1,
};
use crate::host_logic::sso::pairing::{
    ResponderIdentity, VersionedHandshakeProposal, bootstrap_channel, bootstrap_topic,
    decode_pairing_deeplink, derive_x25519_keypair_from_entropy, encrypt_v2_handshake_response,
    establish_responder_session_info, v2,
};
use crate::host_logic::statement_store::{build_signed_statement, parse_new_statements_result};
use crate::runtime::authority::{
    AccountAliasAuthorityRequest, AuthorityError, CreateProofAuthorityRequest,
    CreateTransactionAuthorityRequest, ProductAuthority, SignPayloadAuthorityRequest,
    SignRawAuthorityRequest,
};
use crate::runtime::services::RuntimeServices;
use crate::runtime::sso_remote::fresh_statement_expiry;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::statement_allowance::StatementAllowanceError;
use crate::runtime::statement_store_rpc;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::statement_store_rpc::StatementStoreRpcClientError;

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";
/// RFC-0022 domain for the identity chat X25519 key shared in the handshake.
const CHAT_ENCRYPTION_DOMAIN: &[u8] = b"chat";
/// Leave the product runtime one minute to receive and process the SSO response
/// before its 300-second remote-authority deadline expires.
#[cfg(not(target_arch = "wasm32"))]
const BULLETIN_AUTHORIZATION_WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// Upper bound on remembered request ids for replay dedup within a serve loop.
/// A peer that holds the session open cannot grow this without bound; requests
/// older than the eviction window are past their statement expiry and can no
/// longer be validly replayed.
const MAX_SERVED_REQUEST_IDS: usize = 1024;

fn derive_responder_identity(entropy: &[u8]) -> Result<(ResponderIdentity, [u8; 32]), String> {
    let statement = derive_identity_keypair(entropy)
        .map_err(|err| format!("identity account derivation failed: {err}"))?;
    let (encryption_secret_key, encryption_public_key) =
        derive_x25519_keypair_from_entropy(entropy, SSO_ENCRYPTION_DOMAIN);
    let (identity_chat_private_key, _) =
        derive_x25519_keypair_from_entropy(entropy, CHAT_ENCRYPTION_DOMAIN);
    Ok((
        ResponderIdentity {
            statement_secret: statement.secret.to_bytes(),
            statement_public_key: statement.public.to_bytes(),
            encryption_secret_key,
            encryption_public_key,
        },
        identity_chat_private_key,
    ))
}

/// Bounded set of served request ids for replay dedup. Evicts the oldest id
/// once the capacity is reached so a peer cannot force unbounded memory growth
/// by streaming fresh request ids.
struct ServedRequestIds {
    seen: HashSet<String>,
    order: VecDeque<String>,
}

impl ServedRequestIds {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    /// Record `request_id`, returning `true` if it was not already served.
    /// Evicts the oldest id when the capacity is exceeded.
    fn insert(&mut self, request_id: String) -> bool {
        if !self.seen.insert(request_id.clone()) {
            return false;
        }
        self.order.push_back(request_id);
        if self.order.len() > MAX_SERVED_REQUEST_IDS
            && let Some(evicted) = self.order.pop_front()
        {
            self.seen.remove(&evicted);
        }
        true
    }
}

/// Terminal outcome of one responder serve loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponderExit {
    /// The pairing host announced `Disconnected`.
    PeerDisconnected,
    /// The statement subscription ended without a disconnect message.
    SubscriptionEnded,
}

/// Pairing-host identity needed to restore an SSO responder session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResponderPeer {
    /// Pairing host's sr25519 Statement Store account id.
    pub statement_account_id: [u8; 32],
    /// Pairing host's raw X25519 public key.
    pub encryption_public_key: [u8; 32],
}

struct ResponderMaterial {
    identity: ResponderIdentity,
    identity_chat_private_key: [u8; 32],
    root_account_id: [u8; 32],
    root_entropy_source: [u8; 32],
}

/// Failure while deriving or allocating a Statement Store/Bulletin allowance.
#[derive(Debug, thiserror::Error)]
pub(super) enum AllowanceAllocationError {
    /// Signing host session or authority state was unavailable.
    #[error("{0}")]
    Authority(AuthorityError),
    /// Product-account key derivation failed.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("{0}")]
    ProductAccount(ProductAccountError),
    /// Chain state, metadata, ring, slot, proof, or extrinsic allocation failed.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("{0}")]
    StatementAllowance(#[from] StatementAllowanceError),
    /// Runtime service could not open the required Statement Store RPC client.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("{0}")]
    StatementStoreRpcClient(#[from] StatementStoreRpcClientError),
    /// Runtime service could not open the required Bulletin RPC client.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("{context}: {source}")]
    BulletinRpcClient {
        /// Client context.
        context: &'static str,
        /// Chain runtime failure.
        #[source]
        source: RuntimeFailure,
    },
    /// Allocation helper is unavailable for this target.
    #[cfg(target_arch = "wasm32")]
    #[error("signing host: {resource} allowance allocation is native-only")]
    NativeOnly {
        /// Resource name.
        resource: &'static str,
    },
    /// System time cannot be converted into a UNIX timestamp.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("system clock before UNIX epoch")]
    SystemClockBeforeUnixEpoch,
    /// The signing account is not in the required LitePeople ring.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("signing account is not a LitePeople ring member; cannot grant {resource} allowance")]
    MissingLitePeopleMembership {
        /// Resource name.
        resource: &'static str,
    },
}

impl From<AuthorityError> for AllowanceAllocationError {
    fn from(err: AuthorityError) -> Self {
        Self::Authority(err)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ProductAccountError> for AllowanceAllocationError {
    fn from(err: ProductAccountError) -> Self {
        Self::ProductAccount(err)
    }
}

impl AllowanceAllocationError {
    pub(super) fn into_authority_error(self) -> AuthorityError {
        match self {
            Self::Authority(err) => err,
            other => AuthorityError::Unavailable {
                reason: other.to_string(),
            },
        }
    }
}

/// Answer `deeplink` and serve the resulting SSO session until it ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.respond_to_pairing"))]
pub(crate) async fn respond_to_pairing(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    deeplink: &str,
) -> Result<ResponderExit, String> {
    let (_, session) = answer_pairing(services.clone(), signing_host.clone(), deeplink).await?;
    serve_session(services, signing_host, session).await
}

/// Answer one pairing handshake and return the session material used by the
/// long-running responder loop.
///
/// Native shells use this split form so their approval UI can complete once
/// the handshake is actually on the Statement Store, while the core keeps the
/// session subscription alive in a background task.
pub(crate) async fn answer_pairing(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    deeplink: &str,
) -> Result<(ResponderPeer, SsoSessionInfo), String> {
    let VersionedHandshakeProposal::V2(proposal) =
        decode_pairing_deeplink(deeplink).map_err(|err| err.to_string())?;
    let peer = ResponderPeer {
        statement_account_id: proposal.device.statement_account_id,
        encryption_public_key: proposal.device.encryption_public_key,
    };
    let material = responder_material(&signing_host)?;
    // Product accounts and the SSO statement identity derive from the
    // canonical root key; the identity is the RFC-0022 uid.dot default account.
    let session = establish_responder_session_info(
        &material.identity,
        peer.statement_account_id,
        peer.encryption_public_key,
    )?;

    match submit_pairing_answer(&services, &material, &session, &peer).await {
        Ok(()) => Ok((peer, session)),
        Err(reason) => {
            let failed = v2::EncryptedResponse::Failed(reason.clone());
            if let Err(status_error) = submit_handshake_response(
                &services,
                &session,
                &peer,
                &failed,
                "sso-responder failed status",
            )
            .await
            {
                warn!(%status_error, "failed to post pairing Failed status");
            }
            Err(reason)
        }
    }
}

/// Post the wallet-side pairing answer sequence: a `Pending` status while the
/// session is being prepared, then the `Success` payload.
async fn submit_pairing_answer(
    services: &Arc<RuntimeServices>,
    material: &ResponderMaterial,
    session: &SsoSessionInfo,
    peer: &ResponderPeer,
) -> Result<(), String> {
    let pending = v2::EncryptedResponse::Pending(v2::Status::AllowanceAllocation);
    if let Err(status_error) = submit_handshake_response(
        services,
        session,
        peer,
        &pending,
        "sso-responder pending status",
    )
    .await
    {
        warn!(%status_error, "failed to post pairing Pending status");
    }

    let success = v2::EncryptedResponse::Success(Box::new(v2::Success {
        identity_account_id: material.identity.statement_public_key,
        root_account_id: material.root_account_id,
        identity_chat_private_key: material.identity_chat_private_key,
        sso_enc_pub_key: material.identity.encryption_public_key,
        // The headless responder has no separate device chat key; chat
        // envelopes addressed to the device land on the session key.
        device_enc_pub_key: material.identity.encryption_public_key,
        root_entropy_source: material.root_entropy_source,
    }));
    submit_handshake_response(services, session, peer, &success, "sso-responder handshake").await?;
    debug!("answered pairing handshake");
    Ok(())
}

/// Encrypt one handshake response to the peer and post it on the pairing
/// topic/channel. Each statement uses fresh ephemeral encryption; the shared
/// channel makes later responses replace earlier statuses in the store.
async fn submit_handshake_response(
    services: &Arc<RuntimeServices>,
    session: &SsoSessionInfo,
    peer: &ResponderPeer,
    response: &v2::EncryptedResponse,
    label: &'static str,
) -> Result<(), String> {
    let handshake = encrypt_v2_handshake_response(peer.encryption_public_key, response)?;
    let topic = bootstrap_topic(peer.statement_account_id, peer.encryption_public_key);
    let channel = bootstrap_channel(peer.statement_account_id, peer.encryption_public_key);
    let statement = build_signed_statement(
        session,
        channel,
        topic,
        handshake.encode(),
        fresh_statement_expiry(),
    )?;
    services.statement_store.submit(statement, label).await
}

/// Reconstruct responder session material for a previously persisted peer.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn session_for_peer(
    signing_host: &Arc<SigningHost>,
    peer: &ResponderPeer,
) -> Result<SsoSessionInfo, String> {
    let material = responder_material(signing_host)?;
    establish_responder_session_info(
        &material.identity,
        peer.statement_account_id,
        peer.encryption_public_key,
    )
}

fn responder_material(signing_host: &Arc<SigningHost>) -> Result<ResponderMaterial, String> {
    let entropy = signing_host
        .root_entropy()
        .map_err(|err| format!("signing host has no active local session: {err}"))?;
    let root = derive_root_keypair_from_entropy(&entropy)
        .map_err(|err| format!("root account derivation failed: {err}"))?;
    let (identity, identity_chat_private_key) = derive_responder_identity(&entropy)
        .map_err(|err| format!("responder identity derivation failed: {err}"))?;
    Ok(ResponderMaterial {
        identity,
        identity_chat_private_key,
        root_account_id: root.public.to_bytes(),
        root_entropy_source: root_entropy_source(&entropy),
    })
}

/// Best-effort notification that the signing host ended one paired session.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn submit_disconnected(
    services: &Arc<RuntimeServices>,
    session: &SsoSessionInfo,
) -> Result<(), String> {
    let message_id = "truapi:sso:disconnect".to_string();
    let statement = build_outgoing_request_statement(
        session,
        message_id.clone(),
        vec![RemoteMessage {
            message_id,
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        }],
        fresh_statement_expiry(),
    )?;
    services
        .statement_store
        .submit_fire_and_forget(statement, "sso-responder disconnect")
        .await
        .map_err(|err| format!("SSO disconnect submit failed: {err}"))
}

/// Serve inbound session statements until the session ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.serve_session"))]
pub(crate) async fn serve_session(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    session: SsoSessionInfo,
) -> Result<ResponderExit, String> {
    let rpc_client = services
        .statement_store
        .client("sso-responder session")
        .await
        .map_err(|err| err.to_string())?;
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("sso-responder subscribe failed: {err}"))?;
    let mut served_request_ids = ServedRequestIds::new();

    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("sso-responder subscription failed: {err}"))?;
        let page = parse_new_statements_result("sso-responder".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            let incoming = match decode_incoming_sso_request(&session, &statement) {
                Ok(Some(incoming)) => incoming,
                Ok(None) => continue,
                Err(error) => {
                    let prefix = hex::encode(&statement[..statement.len().min(16)]);
                    warn!(
                        reason = %error.reason,
                        statement_bytes = statement.len(),
                        statement_prefix = %prefix,
                        "ignoring undecodable SSO session statement"
                    );
                    // Ack a decodable envelope whose messages did not decode
                    // so the peer fails fast instead of waiting out its
                    // response deadline.
                    if let Some(request_id) = error.request_id
                        && served_request_ids.insert(request_id.clone())
                    {
                        let ack = build_signed_session_response_statement(
                            &session,
                            request_id,
                            SsoResponseCode::DecodingFailed as u8,
                            fresh_statement_expiry(),
                        )?;
                        services
                            .statement_store
                            .submit_sso(ack, "sso-responder decode-failed ack")
                            .await?;
                    }
                    continue;
                }
            };
            for message in &incoming.messages {
                let cli_summary = format!(
                    "Incoming SSO request · {}\nstatement_request_id={}\nremote_message_id={}",
                    message, incoming.request_id, message.message_id
                );
                tracing::event!(
                    target: "truapi_server::sso_transcript",
                    tracing::Level::DEBUG,
                    cli_summary = cli_summary.as_str(),
                    cli_event = "request_received",
                    request = %message,
                    statement_request_id = %incoming.request_id,
                    remote_message_id = %message.message_id,
                );
            }
            if !served_request_ids.insert(incoming.request_id.clone()) {
                continue;
            }
            if let Some(exit) = serve_request(&services, &signing_host, &session, incoming).await? {
                return Ok(exit);
            }
        }
    }
    Ok(ResponderExit::SubscriptionEnded)
}

/// Ack one inbound request statement and answer its batched messages.
async fn serve_request(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    session: &SsoSessionInfo,
    incoming: IncomingSsoRequest,
) -> Result<Option<ResponderExit>, String> {
    let ack = build_signed_session_response_statement(
        session,
        incoming.request_id.clone(),
        SsoResponseCode::Success as u8,
        fresh_statement_expiry(),
    )?;
    services
        .statement_store
        .submit_sso(ack, "sso-responder ack")
        .await?;

    for message in incoming.messages {
        let RemoteMessageData::V1(request) = message.data;
        if matches!(request, v1::RemoteMessage::Disconnected) {
            debug!("pairing host disconnected the SSO session");
            return Ok(Some(ResponderExit::PeerDisconnected));
        }
        let request_name = request.to_string();
        let responding_to = message.message_id.clone();
        let started = Instant::now();
        let Some(answer) =
            answer_remote_message(services, signing_host, message.message_id, request).await
        else {
            continue;
        };
        let response = answer.response;
        let response_message_id = response.message_id.clone();
        let response_result = answer
            .response_result
            .unwrap_or_else(|| remote_response_result(&response.data));
        let statement_request_id = format!("resp:{}", response.message_id);
        let statement = build_outgoing_request_statement(
            session,
            statement_request_id,
            vec![response],
            fresh_statement_expiry(),
        )?;
        let publish_result = services
            .statement_store
            .submit_sso(statement, "sso-responder response")
            .await;
        let elapsed_ms = started.elapsed().as_millis();
        match publish_result {
            Ok(()) => {
                let cli_summary = response_cli_summary(
                    "SSO response sent",
                    &request_name,
                    &incoming.request_id,
                    &responding_to,
                    &response_message_id,
                    &response_result,
                    elapsed_ms,
                );
                tracing::event!(
                    target: "truapi_server::sso_transcript",
                    tracing::Level::DEBUG,
                    cli_summary = cli_summary.as_str(),
                    cli_event = "response_sent",
                    request = request_name.as_str(),
                    statement_request_id = %incoming.request_id,
                    responding_to = %responding_to,
                    %response_message_id,
                    outcome = response_result.outcome,
                    reason = response_result.reason.as_deref().unwrap_or_default(),
                    elapsed_ms = elapsed_ms as u64,
                );
            }
            Err(reason) => {
                let failure = ResponseResult {
                    outcome: "publish_failed",
                    reason: Some(reason.clone()),
                };
                let cli_summary = response_cli_summary(
                    "SSO response failed",
                    &request_name,
                    &incoming.request_id,
                    &responding_to,
                    &response_message_id,
                    &failure,
                    elapsed_ms,
                );
                tracing::event!(
                    target: "truapi_server::sso_transcript",
                    tracing::Level::WARN,
                    cli_summary = cli_summary.as_str(),
                    cli_event = "response_failed",
                    request = request_name.as_str(),
                    statement_request_id = %incoming.request_id,
                    responding_to = %responding_to,
                    %response_message_id,
                    outcome = failure.outcome,
                    reason = %reason,
                    elapsed_ms = elapsed_ms as u64,
                );
                return Err(reason);
            }
        }
    }
    Ok(None)
}

struct ResponseResult {
    outcome: &'static str,
    reason: Option<String>,
}

struct AnsweredRemoteMessage {
    response: RemoteMessage,
    response_result: Option<ResponseResult>,
}

struct ResourceAllocationAnswer {
    payload: Result<Vec<SsoAllocationOutcome>, String>,
    item_failures: Vec<String>,
}

fn remote_response_result(message: &RemoteMessageData) -> ResponseResult {
    let RemoteMessageData::V1(message) = message;
    let error = match message {
        v1::RemoteMessage::SignResponse(response) => response.payload.as_ref().err().cloned(),
        v1::RemoteMessage::RingVrfAliasResponse(response) => {
            response.payload.as_ref().err().map(ring_vrf_error_reason)
        }
        v1::RemoteMessage::RingVrfProofResponse(response) => {
            response.payload.as_ref().err().map(ring_vrf_error_reason)
        }
        v1::RemoteMessage::ResourceAllocationResponse(response) => {
            return resource_allocation_payload_result(&response.payload, &[]);
        }
        v1::RemoteMessage::CreateTransactionResponse(response) => {
            response.signed_transaction.as_ref().err().cloned()
        }
        v1::RemoteMessage::SignRawLegacyResponse(response) => {
            response.signature.as_ref().err().cloned()
        }
        v1::RemoteMessage::SignVrfResponse(response) => {
            response.payload.as_ref().err().map(sign_vrf_error_reason)
        }
        _ => None,
    };
    ResponseResult {
        outcome: if error.is_some() { "error" } else { "ok" },
        reason: error,
    }
}

fn resource_allocation_payload_result(
    payload: &Result<Vec<SsoAllocationOutcome>, String>,
    item_failures: &[String],
) -> ResponseResult {
    let outcomes = match payload {
        Ok(outcomes) => outcomes,
        Err(reason) => {
            return ResponseResult {
                outcome: "error",
                reason: Some(reason.clone()),
            };
        }
    };
    if outcomes.is_empty() {
        return ResponseResult {
            outcome: "ok",
            reason: None,
        };
    }

    let allocated = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, SsoAllocationOutcome::Allocated(_)))
        .count();
    let rejected = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, SsoAllocationOutcome::Rejected))
        .count();
    let unavailable = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, SsoAllocationOutcome::NotAvailable))
        .count();
    let total = outcomes.len();

    if allocated == total {
        return ResponseResult {
            outcome: "ok",
            reason: None,
        };
    }
    if allocated > 0 {
        let mut reason = format!("{allocated} of {total} requested resources allocated");
        if rejected > 0 {
            reason.push_str(&format!("; {rejected} rejected"));
        }
        if unavailable > 0 {
            reason.push_str(&format!("; {unavailable} unavailable"));
        }
        return allocation_result_with_failures(
            ResponseResult {
                outcome: "partial",
                reason: Some(reason),
            },
            item_failures,
        );
    }
    if rejected > 0 {
        let reason = if rejected == total {
            if total == 1 {
                "Requested resource was rejected".to_string()
            } else {
                format!("All {total} requested resources were rejected")
            }
        } else {
            format!("No resources allocated; {rejected} rejected; {unavailable} unavailable")
        };
        return allocation_result_with_failures(
            ResponseResult {
                outcome: "rejected",
                reason: Some(reason),
            },
            item_failures,
        );
    }

    allocation_result_with_failures(
        ResponseResult {
            outcome: "not_available",
            reason: Some(if total == 1 {
                "Requested resource is not available".to_string()
            } else {
                format!("None of the {total} requested resources are available")
            }),
        },
        item_failures,
    )
}

fn allocation_result_with_failures(
    mut result: ResponseResult,
    item_failures: &[String],
) -> ResponseResult {
    if !item_failures.is_empty() {
        let details = item_failures.join("; ").replace(['\r', '\n'], " ");
        result.reason = Some(match result.reason {
            Some(summary) => format!("{summary}: {details}"),
            None => details,
        });
    }
    result
}

fn ring_vrf_error_reason(error: &RingVrfError) -> String {
    match error {
        RingVrfError::RingNotFound => "RingNotFound".to_string(),
        RingVrfError::NotMember => "NotMember".to_string(),
        RingVrfError::Rejected => "Rejected".to_string(),
        RingVrfError::Unknown { reason } => format!("Unknown: {reason}"),
    }
}

fn response_cli_summary(
    heading: &str,
    request_name: &str,
    statement_request_id: &str,
    responding_to: &str,
    response_message_id: &str,
    result: &ResponseResult,
    elapsed_ms: u128,
) -> String {
    let mut summary = format!(
        "{heading} · {request_name} · {}\nstatement_request_id={statement_request_id}\nresponding_to={responding_to}\nresponse_message_id={response_message_id}\nelapsed_ms={elapsed_ms}",
        result.outcome
    );
    if let Some(reason) = &result.reason {
        summary.push_str("\nreason=");
        summary.push_str(&reason.replace(['\r', '\n'], " "));
    }
    summary
}

/// Answer one application-level request message; `None` for message kinds
/// that take no response (responses echoed by the peer, unknown variants).
async fn answer_remote_message(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    message_id: String,
    request: v1::RemoteMessage,
) -> Option<AnsweredRemoteMessage> {
    let response_id = format!("{message_id}:response");
    let mut response_result = None;
    let data = match request {
        v1::RemoteMessage::SignRequest(request) => v1::RemoteMessage::SignResponse(
            sign_response(services, signing_host, &message_id, *request).await,
        ),
        v1::RemoteMessage::RingVrfAliasRequest(request) => {
            let payload = account_alias_response(signing_host, request).await;
            v1::RemoteMessage::RingVrfAliasResponse(RingVrfAliasResponse {
                responding_to: message_id,
                payload,
            })
        }
        v1::RemoteMessage::RingVrfProofRequest(request) => {
            let payload = create_proof_response(signing_host, request).await;
            v1::RemoteMessage::RingVrfProofResponse(RingVrfProofResponse {
                responding_to: message_id,
                payload,
            })
        }
        v1::RemoteMessage::ResourceAllocationRequest(request) => {
            let answer = resource_allocation_response(services, signing_host, request).await;
            if let Err(reason) = &answer.payload {
                warn!(%reason, "resource allocation request failed");
            }
            response_result = Some(resource_allocation_payload_result(
                &answer.payload,
                &answer.item_failures,
            ));
            v1::RemoteMessage::ResourceAllocationResponse(ResourceAllocationResponse {
                responding_to: message_id,
                payload: answer.payload,
            })
        }
        v1::RemoteMessage::CreateTransactionRequest(request) => {
            let CreateTransactionPayload::V1(payload) = request.payload;
            let signed_transaction = create_transaction_response(
                services,
                signing_host,
                CreateTransactionReview::Product(payload.clone()),
                CreateTransactionAuthorityRequest::Product(payload),
            )
            .await;
            v1::RemoteMessage::CreateTransactionResponse(messages::CreateTransactionResponse {
                responding_to: message_id,
                signed_transaction,
            })
        }
        v1::RemoteMessage::CreateTransactionLegacyRequest(request) => {
            let messages::CreateTransactionLegacyPayload::V1(payload) = request.payload;
            let signed_transaction =
                create_sso_identity_transaction_response(services, signing_host, payload).await;
            v1::RemoteMessage::CreateTransactionResponse(messages::CreateTransactionResponse {
                responding_to: message_id,
                signed_transaction,
            })
        }
        v1::RemoteMessage::SignRawLegacyRequest(request) => {
            let signature = sign_raw_legacy_response(services, signing_host, request).await;
            v1::RemoteMessage::SignRawLegacyResponse(SignRawLegacyResponse {
                responding_to: message_id,
                signature,
            })
        }
        v1::RemoteMessage::SignVrfRequest(request) => {
            let payload = sign_vrf_response(signing_host, message_id.clone(), request).await;
            v1::RemoteMessage::SignVrfResponse(SignVrfResponse {
                responding_to: message_id,
                payload,
            })
        }
        v1::RemoteMessage::ProductSubtreeRequest(request) => {
            let product_public_key = match signing_host.current_session() {
                Some(session) => signing_host
                    .product_subtree_public_key(
                        &CallContext::with_request_id(message_id.clone()),
                        &session,
                        request.product_id,
                    )
                    .await
                    .map_err(|err| err.to_string()),
                None => Err("signing host is disconnected".to_string()),
            };
            v1::RemoteMessage::ProductSubtreeResponse(messages::ProductSubtreeResponse {
                responding_to: message_id,
                product_public_key,
            })
        }
        v1::RemoteMessage::Disconnected
        | v1::RemoteMessage::SignResponse(_)
        | v1::RemoteMessage::RingVrfAliasResponse(_)
        | v1::RemoteMessage::RingVrfProofResponse(_)
        | v1::RemoteMessage::ResourceAllocationResponse(_)
        | v1::RemoteMessage::CreateTransactionResponse(_)
        | v1::RemoteMessage::SignRawLegacyResponse(_)
        | v1::RemoteMessage::ProductSubtreeResponse(_)
        | v1::RemoteMessage::SignVrfResponse(_) => return None,
    };
    Some(AnsweredRemoteMessage {
        response: RemoteMessage {
            message_id: response_id,
            data: RemoteMessageData::V1(data),
        },
        response_result,
    })
}

async fn resource_allocation_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    request: messages::ResourceAllocationRequest,
) -> ResourceAllocationAnswer {
    let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
        calling_product_id: request.calling_product_id.clone(),
        resources: request
            .resources
            .iter()
            .map(public_allocatable_resource)
            .collect(),
    });
    match services.platform.confirm_user_action(review).await {
        Ok(true) => {}
        Ok(false) => {
            return ResourceAllocationAnswer {
                payload: Ok(vec![
                    SsoAllocationOutcome::Rejected;
                    request.resources.len()
                ]),
                item_failures: Vec::new(),
            };
        }
        Err(err) => {
            return ResourceAllocationAnswer {
                payload: Err(format!("confirmation failed: {}", err.reason)),
                item_failures: Vec::new(),
            };
        }
    }

    let mut outcomes = Vec::with_capacity(request.resources.len());
    let mut item_failures = Vec::new();
    for resource in request.resources {
        let outcome = match resource {
            SsoAllocatableResource::StatementStoreAllowance => allocate_statement_store_allowance(
                services,
                signing_host,
                &request.calling_product_id,
                request.on_existing,
            )
            .await
            .map(|slot_account_key| {
                SsoAllocationOutcome::Allocated(SsoAllocatedResource::StatementStoreAllowance {
                    slot_account_key,
                })
            }),
            SsoAllocatableResource::BulletinAllowance => allocate_bulletin_allowance(
                services,
                signing_host,
                &request.calling_product_id,
                request.on_existing,
            )
            .await
            .map(|slot_account_key| {
                SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
                    slot_account_key,
                })
            }),
            SsoAllocatableResource::SmartContractAllowance(_) => {
                Ok(SsoAllocationOutcome::NotAvailable)
            }
            SsoAllocatableResource::AutoSigning => signing_host
                .product_subtree_secret(&request.calling_product_id)
                .map(|product_root_private_key| {
                    SsoAllocationOutcome::Allocated(SsoAllocatedResource::AutoSigning {
                        product_root_private_key,
                    })
                })
                .map_err(AllowanceAllocationError::Authority),
        };
        match outcome {
            Ok(outcome) => outcomes.push(outcome),
            Err(err) => {
                let reason = err.to_string();
                warn!(%reason, "resource allocation item failed");
                item_failures.push(reason);
                outcomes.push(SsoAllocationOutcome::NotAvailable);
            }
        }
    }
    ResourceAllocationAnswer {
        payload: Ok(outcomes),
        item_failures,
    }
}

fn public_allocatable_resource(resource: &SsoAllocatableResource) -> api::AllocatableResource {
    match resource {
        SsoAllocatableResource::StatementStoreAllowance => {
            api::AllocatableResource::StatementStoreAllowance
        }
        SsoAllocatableResource::BulletinAllowance => api::AllocatableResource::BulletinAllowance,
        SsoAllocatableResource::SmartContractAllowance(index) => {
            api::AllocatableResource::SmartContractAllowance(index.clone())
        }
        SsoAllocatableResource::AutoSigning => api::AllocatableResource::AutoSigning,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn allocate_statement_store_allowance(
    services: &Arc<RuntimeServices>,
    signing_host: &SigningHost,
    product_id: &str,
    policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
    use crate::runtime::statement_allowance::{
        self, RegistrationParams, fetch_chain_state, fetch_metadata, find_including_ring,
        register_statement_account,
    };

    let entropy = signing_host.root_entropy()?;
    let allowance =
        derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", product_id])?;
    let target = allowance.public.to_bytes();
    let bandersnatch = derive_lite_person_ring_vrf_entropy(&entropy);
    let rpc = statement_allowance::rpc::RpcClient::new(
        services
            .statement_store
            .client("statement-store allowance")
            .await?,
    );
    let metadata = fetch_metadata(&rpc).await?;
    let chain_state = fetch_chain_state(&rpc).await?;
    let current = statement_allowance::ring::read_current_ring_index(&rpc).await?;
    let ring = find_including_ring(&rpc, &metadata, bandersnatch, current)
        .await?
        .ok_or(AllowanceAllocationError::MissingLitePeopleMembership {
            resource: "statement-store",
        })?;
    let period = statement_allowance::slot::current_period(current_unix_secs()?);
    let outcome = register_statement_account(
        &rpc,
        &metadata,
        &chain_state,
        bandersnatch,
        RegistrationParams {
            target: &target,
            period,
            ring: &ring,
            reuse_existing: matches!(policy, OnExistingAllowancePolicy::Ignore),
        },
    )
    .await?;
    match outcome {
        statement_allowance::RegistrationOutcome::Registered {
            block_hash,
            seq,
            ring_index,
        } => {
            debug!(
                %product_id,
                %block_hash,
                seq,
                ring_index,
                "registered statement-store allowance"
            );
        }
        statement_allowance::RegistrationOutcome::AlreadyAllocated { seq } => {
            debug!(
                %product_id,
                seq,
                "statement-store allowance already allocated"
            );
        }
    }
    Ok(allowance.secret.to_bytes().to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn allocate_bulletin_allowance(
    services: &Arc<RuntimeServices>,
    signing_host: &SigningHost,
    product_id: &str,
    policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
    use crate::runtime::statement_allowance::{
        self, claim_long_term_storage, fetch_bulletin_allowance, fetch_chain_state, fetch_metadata,
        find_including_ring, wait_bulletin_authorization,
    };

    let entropy = signing_host.root_entropy()?;
    let allowance = derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id])?;
    let target = allowance.public.to_bytes();

    let bulletin_rpc = statement_allowance::rpc::RpcClient::new(
        services
            .bulletin
            .client("bulletin allowance")
            .await
            .map_err(|source| AllowanceAllocationError::BulletinRpcClient {
                context: "bulletin allowance client",
                source,
            })?,
    );
    let current_allowance = fetch_bulletin_allowance(&bulletin_rpc, &target).await?;
    if matches!(policy, OnExistingAllowancePolicy::Ignore)
        && current_allowance.is_some_and(|allowance| allowance.available())
    {
        return Ok(allowance.secret.to_bytes().to_vec());
    }

    let people_rpc = statement_allowance::rpc::RpcClient::new(
        services
            .statement_store
            .client("bulletin allowance claim")
            .await?,
    );
    let metadata = fetch_metadata(&people_rpc).await?;
    let chain_state = fetch_chain_state(&people_rpc).await?;
    let bandersnatch = derive_lite_person_ring_vrf_entropy(&entropy);
    let current = statement_allowance::ring::read_current_ring_index(&people_rpc).await?;
    let ring = find_including_ring(&people_rpc, &metadata, bandersnatch, current)
        .await?
        .ok_or(AllowanceAllocationError::MissingLitePeopleMembership {
            resource: "Bulletin",
        })?;
    let period_duration = statement_allowance::slot::long_term_storage_period_duration(&metadata)?;
    let period = statement_allowance::slot::current_long_term_storage_period(
        current_unix_secs()?,
        period_duration,
    )?;
    let outcome = claim_long_term_storage(
        &people_rpc,
        &metadata,
        &chain_state,
        bandersnatch,
        &target,
        period,
        &ring,
    )
    .await?;
    let statement_allowance::LongTermStorageOutcome::Claimed {
        block_hash,
        counter,
        ring_index,
    } = outcome;
    debug!(
        %product_id,
        %block_hash,
        counter,
        ring_index,
        "claimed Bulletin long-term storage allowance"
    );

    let authorization = wait_bulletin_authorization(
        &bulletin_rpc,
        &target,
        current_allowance,
        BULLETIN_AUTHORIZATION_WAIT,
    )
    .await?;
    debug!(
        %product_id,
        remained_size = authorization.remained_size,
        remained_transactions = authorization.remained_transactions,
        "Bulletin authorization visible"
    );
    Ok(allowance.secret.to_bytes().to_vec())
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn allocate_statement_store_allowance(
    _services: &Arc<RuntimeServices>,
    _signing_host: &SigningHost,
    _product_id: &str,
    _policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
    Err(AllowanceAllocationError::NativeOnly {
        resource: "statement-store",
    })
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn allocate_bulletin_allowance(
    _services: &Arc<RuntimeServices>,
    _signing_host: &SigningHost,
    _product_id: &str,
    _policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
    Err(AllowanceAllocationError::NativeOnly {
        resource: "Bulletin",
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn current_unix_secs() -> Result<u64, AllowanceAllocationError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AllowanceAllocationError::SystemClockBeforeUnixEpoch)
}

/// Confirm and serve a payload or raw signing request.
async fn sign_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    message_id: &str,
    request: SigningRequest,
) -> SigningResponse {
    let payload = serve_sign_request(services, signing_host, request).await;
    if let Err(reason) = &payload {
        warn!(%reason, "sign request failed");
    }
    SigningResponse {
        responding_to: message_id.to_string(),
        payload,
    }
}

async fn serve_sign_request(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    request: SigningRequest,
) -> Result<SigningPayloadResponseData, String> {
    let session = signing_host
        .current_session()
        .ok_or_else(|| "signing host session is not active".to_string())?;
    let cx = CallContext::default();
    let response = match request {
        SigningRequest::Payload(request) => {
            let request: api::HostSignPayloadRequest = (*request).into();
            confirm(
                services,
                UserConfirmationReview::SignPayload(SignPayloadReview::Product(request.clone())),
            )
            .await?;
            signing_host
                .sign_payload(&cx, &session, SignPayloadAuthorityRequest::Product(request))
                .await
        }
        SigningRequest::Raw(request) => {
            let request: api::HostSignRawRequest = request.into();
            confirm(
                services,
                UserConfirmationReview::SignRaw(SignRawReview::Product(request.clone())),
            )
            .await?;
            signing_host
                .sign_raw(&cx, &session, SignRawAuthorityRequest::Product(request))
                .await
        }
    }
    .map_err(|err| err.to_string())?;
    Ok(SigningPayloadResponseData {
        signature: response.signature,
        signed_transaction: response.signed_transaction,
    })
}

async fn sign_raw_legacy_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    request: messages::SignRawLegacyRequest,
) -> Result<Vec<u8>, String> {
    let public_request = api::HostSignRawWithLegacyAccountRequest {
        signer: product_public_key_to_address(request.account),
        payload: request.data.into(),
    };
    confirm(
        services,
        UserConfirmationReview::SignRaw(SignRawReview::LegacyAccount(public_request.clone())),
    )
    .await?;
    let session = signing_host
        .current_session()
        .ok_or_else(|| "signing host session is not active".to_string())?;
    signing_host
        .sign_sso_raw_identity(&session, request.account, public_request.payload)
        .map(|response| response.signature)
        .map_err(|err| err.to_string())
}

fn sign_vrf_error_reason(error: &v01::HostAccountSignVrfError) -> String {
    match error {
        v01::HostAccountSignVrfError::NotConnected => "NotConnected".to_string(),
        v01::HostAccountSignVrfError::Rejected => "Rejected".to_string(),
        v01::HostAccountSignVrfError::Unknown { reason } => reason.clone(),
    }
}

async fn sign_vrf_response(
    signing_host: &Arc<SigningHost>,
    message_id: String,
    request: messages::SignVrfRequest,
) -> Result<v01::VrfSignature, v01::HostAccountSignVrfError> {
    let session = signing_host
        .current_session()
        .ok_or(v01::HostAccountSignVrfError::NotConnected)?;
    signing_host
        .sign_vrf(
            &CallContext::with_request_id(message_id),
            &session,
            request.calling_product_id,
            request.payload,
        )
        .await
        .map_err(|err| match err {
            AuthorityError::Disconnected => v01::HostAccountSignVrfError::NotConnected,
            AuthorityError::Rejected => v01::HostAccountSignVrfError::Rejected,
            AuthorityError::Cancelled(err) => v01::HostAccountSignVrfError::Unknown {
                reason: err.to_string(),
            },
            AuthorityError::Unavailable { reason }
            | AuthorityError::NotSupported { reason }
            | AuthorityError::Unknown { reason } => {
                v01::HostAccountSignVrfError::Unknown { reason }
            }
        })
}

/// Confirm and serve a transaction-creation request.
async fn create_transaction_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    review: CreateTransactionReview,
    request: CreateTransactionAuthorityRequest,
) -> Result<Vec<u8>, String> {
    let session = signing_host
        .current_session()
        .ok_or_else(|| "signing host session is not active".to_string())?;
    confirm(services, UserConfirmationReview::CreateTransaction(review)).await?;
    let cx = CallContext::default();
    signing_host
        .create_transaction(&cx, &session, request)
        .await
        .map(|response| response.transaction)
        .map_err(|err| err.to_string())
}

async fn create_sso_identity_transaction_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    request: api::LegacyAccountTxPayload,
) -> Result<Vec<u8>, String> {
    let session = signing_host
        .current_session()
        .ok_or_else(|| "signing host session is not active".to_string())?;
    confirm(
        services,
        UserConfirmationReview::CreateTransaction(CreateTransactionReview::LegacyAccount(
            request.clone(),
        )),
    )
    .await?;
    signing_host
        .create_sso_identity_transaction(&session, request)
        .await
        .map(|response| response.transaction)
        .map_err(|err| err.to_string())
}

async fn account_alias_response(
    signing_host: &Arc<SigningHost>,
    request: messages::RingVrfAliasRequest,
) -> Result<api::HostAccountGetAliasResponse, RingVrfError> {
    let session = signing_host
        .current_session()
        .ok_or_else(disconnected_ring_vrf)?;
    signing_host
        .sso_account_alias(
            &session,
            AccountAliasAuthorityRequest {
                calling_product_id: request.calling_product_id,
                context: request.context,
                ring_location: request.ring_location,
            },
        )
        .await
}

async fn create_proof_response(
    signing_host: &Arc<SigningHost>,
    request: messages::RingVrfProofRequest,
) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
    let session = signing_host
        .current_session()
        .ok_or_else(disconnected_ring_vrf)?;
    signing_host
        .sso_create_proof(
            &session,
            CreateProofAuthorityRequest {
                calling_product_id: request.calling_product_id,
                context: request.context,
                ring_location: request.ring_location,
                message: request.message,
            },
        )
        .await
}

fn disconnected_ring_vrf() -> RingVrfError {
    RingVrfError::Unknown {
        reason: "signing host session is not active".to_string(),
    }
}

/// Run the platform confirmation seam; rejection and failure both refuse the
/// operation with an opaque reason (host-spec B.7).
async fn confirm(
    services: &Arc<RuntimeServices>,
    review: UserConfirmationReview,
) -> Result<(), String> {
    match services.platform.confirm_user_action(review).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Rejected".to_string()),
        Err(err) => Err(format!("confirmation failed: {}", err.reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::LocalActivation;
    use super::*;
    use crate::host_logic::extrinsic::tests::split_v4;
    use crate::host_logic::statement_store::decode_verified_statement_data;
    use crate::runtime::services::RuntimeServices;
    use crate::test_support::{StubPlatform, test_spawner};
    use std::sync::Arc;
    use truapi_platform::{HostInfo, Platform, PlatformInfo, SigningHostConfig};

    const ENTROPY: [u8; 16] = [0xab; 16];

    fn signing_fixture(platform: Arc<StubPlatform>) -> (Arc<RuntimeServices>, Arc<SigningHost>) {
        let platform: Arc<dyn Platform> = platform;
        let config = SigningHostConfig::new(
            HostInfo {
                name: "Polkadot Mobile".to_string(),
                icon: None,
                version: None,
            },
            PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
        )
        .expect("signing host config is valid");
        let services = RuntimeServices::new(
            platform.clone(),
            config.people_chain_genesis_hash,
            config.bulletin_chain_genesis_hash,
            test_spawner(),
        );
        let signing_host = SigningHost::new(services.clone());
        futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        (services, signing_host)
    }

    #[test]
    fn responder_advertises_and_signs_with_the_rfc0022_identity() {
        let (_services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
        let rfc_identity = signing_host
            .current_session()
            .unwrap()
            .identity_account_id
            .unwrap();
        let (identity, _) = derive_responder_identity(&ENTROPY).unwrap();
        assert_eq!(identity.statement_public_key, rfc_identity);

        let (_, host_encryption_public_key) =
            derive_x25519_keypair_from_entropy(&[0x42; 16], b"sso");
        let session =
            establish_responder_session_info(&identity, [0x55; 32], host_encryption_public_key)
                .unwrap();
        let statement = build_signed_statement(
            &session,
            [0x66; 32],
            [0x77; 32],
            b"handshake".to_vec(),
            fresh_statement_expiry(),
        )
        .unwrap();
        let verified =
            decode_verified_statement_data(&statement, Some(identity.statement_public_key))
                .unwrap();
        assert_eq!(verified.signer, rfc_identity);
    }

    fn response_payload(answer: AnsweredRemoteMessage) -> v1::RemoteMessage {
        let RemoteMessageData::V1(data) = answer.response.data;
        data
    }

    #[test]
    fn account_alias_requires_confirmation_for_cross_product_request() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "alias-1".to_string(),
            v1::RemoteMessage::RingVrfAliasRequest(messages::RingVrfAliasRequest {
                calling_product_id: "myapp.dot".to_string(),
                context: api::ProductProofContext {
                    product_id: "other.dot".to_string(),
                    suffix: api::DerivationIndex::Index(0),
                },
                ring_location: api::RingLocation {
                    chain_id: [0; 32],
                    junctions: vec![],
                },
            }),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::RingVrfAliasResponse(response) = response_payload(response) else {
            panic!("expected alias response");
        };
        assert_eq!(response.payload.unwrap_err(), RingVrfError::Rejected);
    }

    #[test]
    fn response_summary_reports_protocol_errors_without_multiline_output() {
        let response = RemoteMessageData::V1(v1::RemoteMessage::RingVrfAliasResponse(
            RingVrfAliasResponse {
                responding_to: "alias-1".to_string(),
                payload: Err(RingVrfError::Unknown {
                    reason: "chain RPC\ntimed out".to_string(),
                }),
            },
        ));

        let result = remote_response_result(&response);
        let summary = response_cli_summary(
            "SSO response sent",
            "get_account_alias",
            "statement-1",
            "alias-1",
            "alias-1:response",
            &result,
            42,
        );

        assert_eq!(result.outcome, "error");
        assert_eq!(
            summary,
            "SSO response sent · get_account_alias · error\n\
             statement_request_id=statement-1\n\
             responding_to=alias-1\n\
             response_message_id=alias-1:response\n\
             elapsed_ms=42\n\
            reason=Unknown: chain RPC timed out"
        );
    }

    #[test]
    fn resource_allocation_summary_reflects_per_resource_outcomes() {
        let result =
            resource_allocation_payload_result(&Ok(vec![SsoAllocationOutcome::Rejected]), &[]);
        assert_eq!(result.outcome, "rejected");
        assert_eq!(
            result.reason.as_deref(),
            Some("Requested resource was rejected")
        );

        let result = resource_allocation_payload_result(
            &Ok(vec![
                SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
                    slot_account_key: vec![1; 64],
                }),
                SsoAllocationOutcome::Rejected,
                SsoAllocationOutcome::NotAvailable,
            ]),
            &[],
        );
        assert_eq!(result.outcome, "partial");
        assert_eq!(
            result.reason.as_deref(),
            Some("1 of 3 requested resources allocated; 1 rejected; 1 unavailable")
        );

        let result = resource_allocation_payload_result(
            &Ok(vec![SsoAllocationOutcome::NotAvailable]),
            &["timed out waiting for Bulletin authorization".to_string()],
        );
        assert_eq!(result.outcome, "not_available");
        assert_eq!(
            result.reason.as_deref(),
            Some(
                "Requested resource is not available: timed out waiting for Bulletin authorization"
            )
        );
    }

    #[test]
    fn response_summary_classifies_resource_allocation_batches() {
        let response = RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(
            ResourceAllocationResponse {
                responding_to: "allocation-1".to_string(),
                payload: Ok(vec![
                    SsoAllocationOutcome::Rejected,
                    SsoAllocationOutcome::NotAvailable,
                ]),
            },
        ));

        let result = remote_response_result(&response);

        assert_eq!(result.outcome, "rejected");
        assert_eq!(
            result.reason.as_deref(),
            Some("No resources allocated; 1 rejected; 1 unavailable")
        );
    }

    #[test]
    fn resource_allocation_requires_confirmation_before_allocation() {
        let platform = Arc::new(StubPlatform::default());
        let (services, signing_host) = signing_fixture(platform.clone());

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "alloc-1".to_string(),
            v1::RemoteMessage::ResourceAllocationRequest(messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![SsoAllocatableResource::StatementStoreAllowance],
                on_existing: messages::OnExistingAllowancePolicy::Ignore,
            }),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::ResourceAllocationResponse(response) = response_payload(response)
        else {
            panic!("expected resource allocation response");
        };
        assert_eq!(
            response.payload.unwrap(),
            vec![SsoAllocationOutcome::Rejected]
        );

        // The confirmation review names the beneficiary product so the user
        // knows which product receives the delegated allowance key.
        let reviews = platform
            .resource_allocation_reviews
            .lock()
            .expect("resource allocation review list mutex poisoned");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].calling_product_id, "myapp.dot");
    }

    #[test]
    fn auto_signing_allocation_returns_the_product_subtree_secret() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (services, signing_host) = signing_fixture(platform);
        let expected_secret = signing_host
            .product_subtree_secret("myapp.dot")
            .expect("product subtree secret derives");

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "alloc-auto-signing".to_string(),
            v1::RemoteMessage::ResourceAllocationRequest(messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![SsoAllocatableResource::AutoSigning],
                on_existing: messages::OnExistingAllowancePolicy::Ignore,
            }),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::ResourceAllocationResponse(response) = response_payload(response)
        else {
            panic!("expected resource allocation response");
        };
        assert_eq!(
            response.payload.unwrap(),
            vec![SsoAllocationOutcome::Allocated(
                SsoAllocatedResource::AutoSigning {
                    product_root_private_key: expected_secret,
                }
            )]
        );
    }

    #[test]
    fn legacy_transaction_request_uses_the_controlled_identity_account() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform {
            create_transaction_confirmed: true,
            ..StubPlatform::default()
        }));
        let identity = derive_identity_keypair(&ENTROPY).unwrap();
        let payload = api::LegacyAccountTxPayload {
            signer: identity.public.to_bytes(),
            genesis_hash: [0xaa; 32],
            call_data: vec![0x00, 0x00],
            extensions: vec![api::TxPayloadExtension {
                id: "CheckNonce".to_string(),
                extra: vec![1],
                additional_signed: vec![2, 3],
            }],
            tx_ext_version: 0,
        };

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "legacy-tx-1".to_string(),
            v1::RemoteMessage::CreateTransactionLegacyRequest(
                messages::CreateTransactionLegacyRequest {
                    payload: messages::CreateTransactionLegacyPayload::V1(payload),
                },
            ),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::CreateTransactionResponse(response) = response_payload(response)
        else {
            panic!("expected create transaction response");
        };
        let transaction = response
            .signed_transaction
            .expect("identity transaction succeeds");
        let (account, signature, tail) = split_v4(&transaction);
        assert_eq!(account, identity.public.to_bytes());
        assert_eq!(tail, vec![1, 0x00, 0x00]);
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        assert!(
            identity
                .public
                .verify_simple(b"substrate", &[0x00, 0x00, 1, 2, 3], &signature)
                .is_ok()
        );
    }

    #[test]
    fn legacy_raw_request_signs_with_the_rfc0022_identity() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            ..StubPlatform::default()
        }));
        let identity = derive_identity_keypair(&ENTROPY).unwrap();

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "legacy-raw-1".to_string(),
            v1::RemoteMessage::SignRawLegacyRequest(messages::SignRawLegacyRequest {
                account: identity.public.to_bytes(),
                data: messages::SigningRawPayload::Bytes(b"hello".to_vec()),
            }),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::SignRawLegacyResponse(response) = response_payload(response) else {
            panic!("expected raw-signing response");
        };
        let signature = schnorrkel::Signature::from_bytes(
            &response.signature.expect("identity raw signing succeeds"),
        )
        .unwrap();
        assert!(
            identity
                .public
                .verify_simple(b"substrate", b"<Bytes>hello</Bytes>", &signature)
                .is_ok()
        );
    }

    #[test]
    fn product_subtree_request_is_consent_free_and_hard_derived() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "subtree-1".to_string(),
            v1::RemoteMessage::ProductSubtreeRequest(messages::ProductSubtreeRequest {
                product_id: "browse.dot".to_string(),
            }),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::ProductSubtreeResponse(response) = response_payload(response) else {
            panic!("expected product subtree response");
        };
        let root =
            derive_root_keypair_from_entropy(&ENTROPY).expect("fixture entropy derives root");
        let expected =
            crate::host_logic::product_account::derive_product_subtree_keypair(&root, "browse.dot")
                .expect("fixture derives subtree")
                .public
                .to_bytes();
        assert_eq!(response.responding_to, "subtree-1");
        assert_eq!(response.product_public_key, Ok(expected));
    }

    #[test]
    fn served_request_ids_dedup_and_bound() {
        let mut served = ServedRequestIds::new();

        // First sighting is served; an immediate duplicate is rejected.
        assert!(served.insert("req-a".to_string()));
        assert!(!served.insert("req-a".to_string()));

        // Fill to capacity with distinct ids; the set never exceeds the bound.
        for i in 0..MAX_SERVED_REQUEST_IDS {
            served.insert(format!("fill-{i}"));
        }
        assert_eq!(served.seen.len(), MAX_SERVED_REQUEST_IDS);
        assert_eq!(served.order.len(), MAX_SERVED_REQUEST_IDS);

        // The oldest id ("req-a") has been evicted, so it is accepted again,
        // while a recent id is still deduped — memory stays bounded regardless
        // of how many ids a peer streams.
        assert!(served.insert("req-a".to_string()));
        assert!(!served.insert(format!("fill-{}", MAX_SERVED_REQUEST_IDS - 1)));
        assert_eq!(served.seen.len(), MAX_SERVED_REQUEST_IDS);
    }
}
