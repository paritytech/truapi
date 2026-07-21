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

use std::collections::HashSet;
use std::sync::Arc;

use parity_scale_codec::Encode;
use tracing::{debug, info, instrument, trace, warn};
use truapi::{CallContext, latest as api};
use truapi_platform::{
    CreateTransactionReview, SignPayloadReview, SignRawReview, UserConfirmationReview,
};

use super::SigningHost;
use crate::host_logic::entropy::root_entropy_source;
#[cfg(not(target_arch = "wasm32"))]
use crate::host_logic::product_account::ProductAccountError;
use crate::host_logic::product_account::{
    derive_root_keypair_from_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::messages::{
    self, CreateTransactionPayload, IncomingSsoRequest, OnExistingAllowancePolicy, RemoteMessage,
    RemoteMessageData, ResourceAllocationResponse, RingVrfAliasResponse, RingVrfError,
    RingVrfProofResponse, SignRawLegacyResponse, SigningPayloadResponseData, SigningRequest,
    SigningResponse, SsoAllocatableResource, SsoAllocatedResource, SsoAllocationOutcome,
    StatementStoreProductSignResponse, build_outgoing_request_statement,
    build_signed_session_response_statement, decode_incoming_sso_request, v1,
};
use crate::host_logic::sso::pairing::{
    ResponderIdentity, VersionedHandshakeProposal, bootstrap_topic, decode_pairing_deeplink,
    derive_p256_keypair_from_entropy, encrypt_v2_handshake_response,
    establish_responder_session_info, v2,
};
use crate::host_logic::statement_store::{
    build_signed_statement, parse_new_statements_result,
    validate_unsigned_statement_signing_payload,
};
use crate::runtime::authority::{
    AccountAliasAuthorityRequest, CreateProofAuthorityRequest, CreateTransactionAuthorityRequest,
    ProductAuthority, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
};
use crate::runtime::services::RuntimeServices;
use crate::runtime::sso_remote::fresh_statement_expiry;
use crate::runtime::statement_store_rpc;

/// Domain label for the responder's persistent P-256 encryption key.
const SSO_ENCRYPTION_KEY_LABEL: &[u8] = b"sso-encryption";
/// Domain label for the identity chat key shared in the handshake payload.
const CHAT_KEY_LABEL: &[u8] = b"chat-encryption";

/// Terminal outcome of one responder serve loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponderExit {
    /// The pairing host announced `Disconnected`.
    PeerDisconnected,
    /// The statement subscription ended without a disconnect message.
    SubscriptionEnded,
}

/// Answer `deeplink` and serve the resulting SSO session until it ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.respond_to_pairing"))]
pub(crate) async fn respond_to_pairing(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    deeplink: &str,
) -> Result<ResponderExit, String> {
    let VersionedHandshakeProposal::V2(proposal) = decode_pairing_deeplink(deeplink)?;
    let entropy = signing_host
        .root_entropy()
        .map_err(|err| format!("signing host has no active local session: {err}"))?;
    // Product accounts derive from the canonical root key. The SSO statement
    // identity keeps its dedicated hard-derived key.
    let root = derive_root_keypair_from_entropy(&entropy)
        .map_err(|err| format!("root account derivation failed: {err}"))?;
    let statement = derive_sr25519_hard_path(&entropy, &["wallet", "sso"])
        .map_err(|err| format!("//wallet//sso derivation failed: {err}"))?;
    let (encryption_secret_key, encryption_public_key) =
        derive_p256_keypair_from_entropy(&entropy, SSO_ENCRYPTION_KEY_LABEL)
            .map_err(|err| format!("responder P-256 derivation failed: {err}"))?;
    let (identity_chat_private_key, _) = derive_p256_keypair_from_entropy(&entropy, CHAT_KEY_LABEL)
        .map_err(|err| format!("responder chat-key derivation failed: {err}"))?;
    let identity = ResponderIdentity {
        statement_secret: statement.secret.to_bytes(),
        statement_public_key: statement.public.to_bytes(),
        encryption_secret_key,
        encryption_public_key,
    };
    let session = establish_responder_session_info(
        &identity,
        proposal.device.statement_account_id,
        proposal.device.encryption_public_key,
    )?;

    let success = v2::EncryptedResponse::Success(Box::new(v2::Success {
        identity_account_id: identity.statement_public_key,
        root_account_id: root.public.to_bytes(),
        identity_chat_private_key,
        sso_enc_pub_key: identity.encryption_public_key,
        device_enc_pub_key: identity.encryption_public_key,
        root_entropy_source: root_entropy_source(&entropy),
    }));
    let handshake = encrypt_v2_handshake_response(proposal.device.encryption_public_key, &success)?;
    let topic = bootstrap_topic(
        proposal.device.statement_account_id,
        proposal.device.encryption_public_key,
    );
    let statement = build_signed_statement(
        &session,
        topic,
        topic,
        handshake.encode(),
        fresh_statement_expiry(),
    )?;
    services
        .statement_store
        .submit(statement, "sso-responder handshake")
        .await?;
    info!("answered pairing handshake, serving SSO session");

    serve_session(services, signing_host, session).await
}

/// Serve inbound session statements until the session ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.serve_session"))]
async fn serve_session(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    session: SsoSessionInfo,
) -> Result<ResponderExit, String> {
    let rpc_client = services
        .statement_store
        .client("sso-responder session")
        .await?;
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("sso-responder subscribe failed: {err}"))?;
    let mut served_request_ids = HashSet::new();

    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("sso-responder subscription failed: {err}"))?;
        let page = parse_new_statements_result("sso-responder".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            let incoming = match decode_incoming_sso_request(&session, &statement) {
                Ok(Some(incoming)) => incoming,
                Ok(None) => continue,
                Err(reason) => {
                    let prefix = hex::encode(&statement[..statement.len().min(16)]);
                    warn!(
                        %reason,
                        statement_bytes = statement.len(),
                        statement_prefix = %prefix,
                        "ignoring undecodable SSO session statement"
                    );
                    continue;
                }
            };
            for message in &incoming.messages {
                let cli_summary = format!(
                    "Incoming SSO request · {}\nstatement_request_id={}\nremote_message_id={}",
                    remote_message_name(&message.data),
                    incoming.request_id,
                    message.message_id
                );
                tracing::event!(
                    target: "truapi_server::incoming_sso_request",
                    tracing::Level::INFO,
                    cli_summary = cli_summary.as_str(),
                    statement_request_id = %incoming.request_id,
                    remote_message_id = %message.message_id,
                    remote_message = remote_message_name(&message.data),
                );
                debug!(
                    statement_request_id = %incoming.request_id,
                    remote_message_id = %message.message_id,
                    remote_message = ?message.data,
                    "decoded SSO request"
                );
                trace!(
                    statement_request_id = %incoming.request_id,
                    remote_message_id = %message.message_id,
                    remote_message = ?message.data,
                    "received SSO message"
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
        0,
        fresh_statement_expiry(),
    )?;
    services
        .statement_store
        .submit(ack, "sso-responder ack")
        .await?;

    for message in incoming.messages {
        let RemoteMessageData::V1(request) = message.data;
        if matches!(request, v1::RemoteMessage::Disconnected) {
            info!("pairing host disconnected the SSO session");
            return Ok(Some(ResponderExit::PeerDisconnected));
        }
        let Some(response) =
            answer_remote_message(services, signing_host, message.message_id, request).await
        else {
            continue;
        };
        let statement_request_id = format!("resp:{}", response.message_id);
        let statement = build_outgoing_request_statement(
            session,
            statement_request_id,
            vec![response],
            fresh_statement_expiry(),
        )?;
        services
            .statement_store
            .submit(statement, "sso-responder response")
            .await?;
    }
    Ok(None)
}

fn remote_message_name(message: &RemoteMessageData) -> &'static str {
    match message {
        RemoteMessageData::V1(message) => match message {
            v1::RemoteMessage::Disconnected => "disconnected",
            v1::RemoteMessage::SignRequest(_) => "sign_request",
            v1::RemoteMessage::SignResponse(_) => "sign_response",
            v1::RemoteMessage::RingVrfAliasRequest(_) => "get_account_alias",
            v1::RemoteMessage::RingVrfAliasResponse(_) => "get_account_alias_response",
            v1::RemoteMessage::ResourceAllocationRequest(_) => "resource_allocation",
            v1::RemoteMessage::ResourceAllocationResponse(_) => "resource_allocation_response",
            v1::RemoteMessage::CreateTransactionRequest(_) => "create_transaction",
            v1::RemoteMessage::CreateTransactionResponse(_) => "create_transaction_response",
            v1::RemoteMessage::CreateTransactionLegacyRequest(_) => "create_transaction_legacy",
            v1::RemoteMessage::SignRawLegacyRequest(_) => "sign_raw_legacy",
            v1::RemoteMessage::SignRawLegacyResponse(_) => "sign_raw_legacy_response",
            v1::RemoteMessage::RingVrfProofRequest(_) => "create_account_proof",
            v1::RemoteMessage::RingVrfProofResponse(_) => "create_account_proof_response",
            v1::RemoteMessage::StatementStoreProductSignRequest(_) => {
                "statement_store_product_sign"
            }
            v1::RemoteMessage::StatementStoreProductSignResponse(_) => {
                "statement_store_product_sign_response"
            }
        },
    }
}

/// Answer one application-level request message; `None` for message kinds
/// that take no response (responses echoed by the peer, unknown variants).
async fn answer_remote_message(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    message_id: String,
    request: v1::RemoteMessage,
) -> Option<RemoteMessage> {
    let response_id = format!("{message_id}:response");
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
            let payload = resource_allocation_response(services, signing_host, request).await;
            if let Err(reason) = &payload {
                warn!(%reason, "resource allocation request failed");
            }
            v1::RemoteMessage::ResourceAllocationResponse(ResourceAllocationResponse {
                responding_to: message_id,
                payload,
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
        v1::RemoteMessage::CreateTransactionLegacyRequest(_) => {
            v1::RemoteMessage::CreateTransactionResponse(messages::CreateTransactionResponse {
                responding_to: message_id,
                signed_transaction: Err(
                    "signing host: legacy-account transactions are not supported".to_string(),
                ),
            })
        }
        v1::RemoteMessage::SignRawLegacyRequest(_) => {
            v1::RemoteMessage::SignRawLegacyResponse(SignRawLegacyResponse {
                responding_to: message_id,
                signature: Err(
                    "signing host: legacy-account raw signing is not supported".to_string()
                ),
            })
        }
        v1::RemoteMessage::StatementStoreProductSignRequest(request) => {
            let signature =
                statement_store_product_sign_response(services, signing_host, request).await;
            v1::RemoteMessage::StatementStoreProductSignResponse(
                StatementStoreProductSignResponse {
                    responding_to: message_id,
                    signature,
                },
            )
        }
        v1::RemoteMessage::Disconnected
        | v1::RemoteMessage::SignResponse(_)
        | v1::RemoteMessage::RingVrfAliasResponse(_)
        | v1::RemoteMessage::RingVrfProofResponse(_)
        | v1::RemoteMessage::ResourceAllocationResponse(_)
        | v1::RemoteMessage::CreateTransactionResponse(_)
        | v1::RemoteMessage::SignRawLegacyResponse(_)
        | v1::RemoteMessage::StatementStoreProductSignResponse(_) => return None,
    };
    Some(RemoteMessage {
        message_id: response_id,
        data: RemoteMessageData::V1(data),
    })
}

async fn resource_allocation_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    request: messages::ResourceAllocationRequest,
) -> Result<Vec<SsoAllocationOutcome>, String> {
    confirm(
        services,
        UserConfirmationReview::ResourceAllocation(api::HostRequestResourceAllocationRequest {
            resources: request
                .resources
                .iter()
                .map(public_allocatable_resource)
                .collect(),
        }),
    )
    .await?;

    let mut outcomes = Vec::with_capacity(request.resources.len());
    for resource in request.resources {
        let outcome = match resource {
            SsoAllocatableResource::StatementStoreAllowance => allocate_statement_store_allowance(
                services,
                signing_host,
                &request.calling_product_id,
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
            SsoAllocatableResource::SmartContractAllowance(_)
            | SsoAllocatableResource::AutoSigning => Ok(SsoAllocationOutcome::NotAvailable),
        };
        match outcome {
            Ok(outcome) => outcomes.push(outcome),
            Err(reason) => {
                warn!(%reason, "resource allocation item failed");
                outcomes.push(SsoAllocationOutcome::Rejected);
            }
        }
    }
    Ok(outcomes)
}

fn public_allocatable_resource(resource: &SsoAllocatableResource) -> api::AllocatableResource {
    match resource {
        SsoAllocatableResource::StatementStoreAllowance => {
            api::AllocatableResource::StatementStoreAllowance
        }
        SsoAllocatableResource::BulletinAllowance => api::AllocatableResource::BulletinAllowance,
        SsoAllocatableResource::SmartContractAllowance(index) => {
            api::AllocatableResource::SmartContractAllowance(*index)
        }
        SsoAllocatableResource::AutoSigning => api::AllocatableResource::AutoSigning,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn allocate_statement_store_allowance(
    services: &Arc<RuntimeServices>,
    signing_host: &SigningHost,
    product_id: &str,
) -> Result<Vec<u8>, String> {
    use crate::runtime::statement_allowance::{
        self, fetch_chain_state, fetch_metadata, find_including_ring, register_statement_account,
    };

    let entropy = signing_host.root_entropy().map_err(|err| err.reason())?;
    let allowance =
        derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", product_id])
            .map_err(product_account_error)?;
    let target = allowance.public.to_bytes();
    let bandersnatch = statement_allowance::bandersnatch_entropy(&entropy);
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
        .ok_or_else(|| {
            "signing account is not a LitePeople ring member; cannot grant statement-store allowance"
                .to_string()
        })?;
    let period = statement_allowance::slot::current_period(current_unix_secs()?);
    let outcome = register_statement_account(
        &rpc,
        &metadata,
        &chain_state,
        bandersnatch,
        &target,
        period,
        &ring,
    )
    .await?;
    match outcome {
        statement_allowance::RegistrationOutcome::Registered {
            block_hash,
            seq,
            ring_index,
        } => {
            info!(
                %product_id,
                %block_hash,
                seq,
                ring_index,
                "registered statement-store allowance"
            );
        }
        statement_allowance::RegistrationOutcome::AlreadyAllocated { seq } => {
            info!(
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
) -> Result<Vec<u8>, String> {
    use crate::runtime::statement_allowance::{
        self, claim_long_term_storage, fetch_bulletin_allowance, fetch_chain_state, fetch_metadata,
        find_including_ring, wait_bulletin_authorization,
    };

    const AUTHORIZATION_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

    let entropy = signing_host.root_entropy().map_err(|err| err.reason())?;
    let allowance = derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id])
        .map_err(product_account_error)?;
    let target = allowance.public.to_bytes();

    let bulletin_rpc = statement_allowance::rpc::RpcClient::new(
        services
            .bulletin
            .client("bulletin allowance")
            .await
            .map_err(|err| err.reason())?,
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
    let bandersnatch = statement_allowance::bandersnatch_entropy(&entropy);
    let current = statement_allowance::ring::read_current_ring_index(&people_rpc).await?;
    let ring = find_including_ring(&people_rpc, &metadata, bandersnatch, current)
        .await?
        .ok_or_else(|| {
            "signing account is not a LitePeople ring member; cannot grant Bulletin allowance"
                .to_string()
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
    info!(
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
        AUTHORIZATION_WAIT,
    )
    .await?;
    info!(
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
) -> Result<Vec<u8>, String> {
    Err("signing host: statement-store allowance allocation is native-only".to_string())
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn allocate_bulletin_allowance(
    _services: &Arc<RuntimeServices>,
    _signing_host: &SigningHost,
    _product_id: &str,
    _policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, String> {
    Err("signing host: Bulletin allowance allocation is native-only".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn current_unix_secs() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "system clock before UNIX epoch".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn product_account_error(err: ProductAccountError) -> String {
    err.to_string()
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
    let cx = CallContext::new();
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
    .map_err(|err| err.reason())?;
    Ok(SigningPayloadResponseData {
        signature: response.signature,
        signed_transaction: response.signed_transaction,
    })
}

async fn statement_store_product_sign_response(
    services: &Arc<RuntimeServices>,
    signing_host: &Arc<SigningHost>,
    request: messages::StatementStoreProductSignRequest,
) -> Result<Vec<u8>, String> {
    validate_unsigned_statement_signing_payload(&request.payload)?;
    confirm(
        services,
        UserConfirmationReview::SignRaw(SignRawReview::Product(api::HostSignRawRequest {
            account: request.product_account_id.clone(),
            payload: api::RawPayload::Bytes {
                bytes: request.payload.clone(),
            },
        })),
    )
    .await?;
    let session = signing_host
        .current_session()
        .ok_or_else(|| "signing host session is not active".to_string())?;
    let cx = CallContext::new();
    signing_host
        .sign_statement_store_product_payload(
            &cx,
            &session,
            request.product_account_id,
            request.payload,
        )
        .await
        .map(|signature| signature.to_vec())
        .map_err(|err| err.reason())
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
    let cx = CallContext::new();
    signing_host
        .create_transaction(&cx, &session, request)
        .await
        .map(|response| response.transaction)
        .map_err(|err| err.reason())
}

async fn account_alias_response(
    signing_host: &Arc<SigningHost>,
    request: messages::RingVrfAliasRequest,
) -> Result<api::HostAccountGetAliasResponse, RingVrfError> {
    let session = signing_host
        .current_session()
        .ok_or_else(disconnected_ring_vrf)?;
    let cx = CallContext::new();
    signing_host
        .account_alias(
            &cx,
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
    let cx = CallContext::new();
    signing_host
        .create_proof(
            &cx,
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
    use crate::host_logic::statement_store::{StatementField, unsigned_statement_signing_payload};
    use crate::runtime::services::RuntimeServices;
    use crate::test_support::{StubPlatform, test_spawner};
    use std::sync::Arc;
    use truapi_platform::{HostInfo, Platform, PlatformInfo, SigningHostConfig};

    const ENTROPY: [u8; 16] = [0xab; 16];

    fn product_account(product_id: &str) -> api::ProductAccountId {
        api::ProductAccountId {
            dot_ns_identifier: product_id.to_string(),
            derivation_index: 0,
        }
    }

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

    fn statement_sign_request(payload: Vec<u8>) -> v1::RemoteMessage {
        v1::RemoteMessage::StatementStoreProductSignRequest(
            messages::StatementStoreProductSignRequest {
                product_account_id: product_account("myapp.dot"),
                payload,
            },
        )
    }

    fn statement_payload() -> Vec<u8> {
        unsigned_statement_signing_payload(vec![
            StatementField::Expiry(42),
            StatementField::Topic1([1; 32]),
            StatementField::Data(vec![0xde, 0xad]),
        ])
        .expect("valid statement payload")
    }

    fn response_payload(response: RemoteMessage) -> v1::RemoteMessage {
        let RemoteMessageData::V1(data) = response.data;
        data
    }

    #[test]
    fn statement_store_product_sign_rejects_non_statement_payload() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            ..StubPlatform::default()
        }));

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "request-1".to_string(),
            statement_sign_request(vec![0, 0, 1, 2, 3]),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::StatementStoreProductSignResponse(response) =
            response_payload(response)
        else {
            panic!("expected statement sign response");
        };
        assert_eq!(response.responding_to, "request-1");
        assert!(
            response
                .signature
                .unwrap_err()
                .contains("invalid statement signing payload")
        );
    }

    #[test]
    fn statement_store_product_sign_requires_confirmation() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

        let response = futures::executor::block_on(answer_remote_message(
            &services,
            &signing_host,
            "request-1".to_string(),
            statement_sign_request(statement_payload()),
        ))
        .expect("response is emitted");

        let v1::RemoteMessage::StatementStoreProductSignResponse(response) =
            response_payload(response)
        else {
            panic!("expected statement sign response");
        };
        assert_eq!(response.signature.unwrap_err(), "Rejected");
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
                    suffix: vec![],
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
    fn resource_allocation_requires_confirmation_before_allocation() {
        let (services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

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
        assert_eq!(response.payload.unwrap_err(), "Rejected");
    }
}
