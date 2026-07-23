//! Signing-host role for wallet-local account authority.
//!
//! A signing host owns the user's keys and serves authority requests locally,
//! with no pairing flow and no SSO channel. Secret material is provided by the
//! embedding host at unlock through [`LocalActivation::activate_local_session`]
//! (the host owns its persistence, e.g. the OS keychain) and kept in memory
//! for the session, zeroized on disconnect.
//!
//! Implemented: local session lifecycle, raw-bytes signing, extrinsic-payload
//! signing, v4 transaction construction (payload fields and extensions arrive
//! pre-encoded, so no chain metadata is needed), RFC-0007 product entropy,
//! bandersnatch ring-VRF aliases and membership proofs, and product-scoped
//! Statement Store and Bulletin allowance keys (native only).

mod local_activation;
mod ring_vrf;
mod sso_responder;

use std::sync::{Arc, Mutex};

use parity_scale_codec::Encode;
use subxt::utils::{AccountId32, MultiSignature};

pub(crate) use local_activation::LocalActivation;
pub use sso_responder::ResponderExit;
pub(crate) use sso_responder::respond_to_pairing;

use super::authority::{
    AccountAliasAuthorityRequest, AuthorityError, AuthoritySession, BulletinAllowanceKey,
    CreateProofAuthorityRequest, CreateTransactionAuthorityRequest, ProductAuthority,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
    authority_session, require_current_session,
};
use super::{RuntimeServices, connected_session_ui_info};
use crate::host_logic::entropy::derive_product_entropy;
use crate::host_logic::extrinsic::{
    Sr25519Signer, build_signed_extrinsic_v4, build_signed_extrinsic_v4_with_signature,
};
use crate::host_logic::product_account::{
    ProductAccountError, SR25519_SIGNING_CONTEXT, derive_product_keypair,
    derive_root_keypair_from_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::session::SessionState;
use crate::host_logic::sso::messages::{OnExistingAllowancePolicy, RingVrfError};
use crate::host_logic::transaction::{extrinsic_payload_extensions, extrinsic_payload_preimage};
use crate::runtime::auth_state::AuthStateMachine;
use ring_vrf::{
    ChainRingResolver, MemberCandidate, PersonKey, RingResolver, alias_from_entropy, context_bytes,
    create_proof, key_for_collection, member_from_entropy, person_entropy,
};

use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};
use truapi_platform::{
    CreateProofReview, PermissionAuthorizationStatus, Platform, ProductContext,
    UserConfirmationReview, normalize_product_identifier,
};
use zeroize::Zeroizing;

const BYTES_WRAP_PREFIX: &[u8] = b"<Bytes>";
const BYTES_WRAP_SUFFIX: &[u8] = b"</Bytes>";

/// Wallet-local account authority for a signing host.
pub(crate) struct SigningHost {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    session_state: Arc<SessionState>,
    auth_state: AuthStateMachine,
    ring_resolver: Arc<dyn RingResolver>,
    /// Root BIP-39 entropy held only while a session is active.
    root_entropy: Mutex<Option<Zeroizing<Vec<u8>>>>,
}

impl SigningHost {
    pub(crate) fn new(services: Arc<RuntimeServices>) -> Arc<Self> {
        let platform = services.platform.clone();
        let ring_resolver = ChainRingResolver::new(services.chain.clone());
        Arc::new(Self {
            services,
            platform: platform.clone(),
            session_state: SessionState::new(),
            auth_state: AuthStateMachine::new(platform),
            ring_resolver,
            root_entropy: Mutex::new(None),
        })
    }

    #[cfg(test)]
    fn new_with_ring_resolver(
        platform: Arc<dyn Platform>,
        ring_resolver: Arc<dyn RingResolver>,
    ) -> Arc<Self> {
        let services = RuntimeServices::new(
            platform.clone(),
            [0; 32],
            [0xbb; 32],
            crate::test_support::test_spawner(),
        );
        Arc::new(Self {
            services,
            platform: platform.clone(),
            session_state: SessionState::new(),
            auth_state: AuthStateMachine::new(platform),
            ring_resolver,
            root_entropy: Mutex::new(None),
        })
    }

    pub(super) fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Current root entropy, or [`AuthorityError::Disconnected`] when no local
    /// session is active.
    fn root_entropy(&self) -> Result<Zeroizing<Vec<u8>>, AuthorityError> {
        self.root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned")
            .clone()
            .ok_or(AuthorityError::Disconnected)
    }

    /// Derive the product-account keypair for `account` from the root entropy.
    ///
    /// The root keypair is recomputed per call (PBKDF2, 2048 rounds, via
    /// `substrate-bip39`) rather than cached: the signing host holds only the
    /// raw, zeroizable entropy, never an expanded secret key.
    fn product_keypair(
        &self,
        account: &v01::ProductAccountId,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let product_id =
            normalize_product_identifier(&account.dot_ns_identifier).map_err(|err| {
                AuthorityError::Unavailable {
                    reason: err.to_string(),
                }
            })?;
        derive_product_keypair(&root, &product_id, account.derivation_index)
            .map_err(product_authority_error)
    }

    fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
        let entropy = self.root_entropy()?;
        derive_sr25519_hard_path(&entropy, &["wallet", "sso"]).map_err(product_authority_error)
    }

    fn person_entropy(
        &self,
        session: &AuthoritySession,
        key: PersonKey,
    ) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
        require_current_session(&self.session_state, session)?;
        let root = self.root_entropy()?;
        Ok(person_entropy(&root, key))
    }

    fn member_candidates(
        &self,
        session: &AuthoritySession,
    ) -> Result<[MemberCandidate; 2], RingVrfError> {
        let full_entropy = self.person_entropy(session, PersonKey::Full)?;
        let lite_entropy = self.person_entropy(session, PersonKey::Lite)?;
        Ok([
            MemberCandidate {
                key: PersonKey::Full,
                member: member_from_entropy(&full_entropy)?,
            },
            MemberCandidate {
                key: PersonKey::Lite,
                member: member_from_entropy(&lite_entropy)?,
            },
        ])
    }

    async fn confirm_ring_vrf_if_cross_product(
        &self,
        calling_product_id: &str,
        target_product_id: &str,
        review: UserConfirmationReview,
    ) -> Result<(), RingVrfError> {
        if calling_product_id == target_product_id {
            return Ok(());
        }
        match self.platform.confirm_user_action(review).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(RingVrfError::Rejected),
            Err(err) => Err(RingVrfError::Unknown {
                reason: format!("confirmation failed: {}", err.reason),
            }),
        }
    }
}

#[async_trait::async_trait]
impl ProductAuthority for SigningHost {
    fn current_session(&self) -> Option<AuthoritySession> {
        self.session_state.current().as_ref().map(authority_session)
    }

    fn session_state(&self) -> Arc<SessionState> {
        SigningHost::session_state(self)
    }

    async fn request_login(
        &self,
        _product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        if let Some(session) = self.session_state.current() {
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ))
        } else {
            // The host activates a local session out of band once the wallet
            // is unlocked; there is no in-core login prompt to drive.
            Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::Rejected,
            ))
        }
    }

    async fn disconnect(&self) {
        self.root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned")
            .take();
        self.session_state.clear_session();
        self.auth_state.store_disconnected();
    }

    async fn sign_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let (keypair, payload) = match request {
            SignPayloadAuthorityRequest::Product(request) => {
                (self.product_keypair(&request.account)?, request.payload)
            }
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => (self.product_keypair(&product_account)?, request.payload),
        };
        sign_extrinsic_payload(&keypair, payload)
    }

    async fn sign_raw(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: SignRawAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let (keypair, payload) = match request {
            SignRawAuthorityRequest::Product(request) => {
                (self.product_keypair(&request.account)?, request.payload)
            }
            SignRawAuthorityRequest::LegacyAccount { account, request } => {
                let keypair = self.identity_keypair()?;
                if keypair.public.to_bytes() != account {
                    return Err(AuthorityError::Unavailable {
                        reason: "signing host: the requested legacy account is not available in \
                                 this CLI wallet"
                            .to_string(),
                    });
                }
                (keypair, request.payload)
            }
        };
        require_current_session(&self.session_state, session)?;
        let message = raw_payload_bytes(payload)?;
        let signature = keypair
            .secret
            .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
            .to_bytes();
        Ok(v01::HostSignPayloadResponse {
            signature: signature.to_vec(),
            signed_transaction: None,
        })
    }

    async fn create_transaction(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        match request {
            CreateTransactionAuthorityRequest::Product(payload) => {
                // The product account is authoritative and caller-scoping is
                // enforced upstream, so the derived key defines the signer.
                let keypair = self.product_keypair(&payload.signer)?;
                build_local_transaction(
                    &keypair,
                    &payload.call_data,
                    &payload.extensions,
                    payload.tx_ext_version,
                )
            }
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => {
                let keypair = self.product_keypair(&product_account)?;
                // Defense-in-depth: the slot-zero key must match the legacy
                // signer the caller asked for (also validated upstream). Never
                // sign with a diverging key.
                if keypair.public.to_bytes() != request.signer {
                    return Err(AuthorityError::Unknown {
                        reason: "signing host: legacy signer does not match the product \
                                 slot-zero account"
                            .to_string(),
                    });
                }
                build_local_transaction(
                    &keypair,
                    &request.call_data,
                    &request.extensions,
                    request.tx_ext_version,
                )
            }
            CreateTransactionAuthorityRequest::IdentityAccount(request) => {
                let keypair = self.identity_keypair()?;
                if keypair.public.to_bytes() != request.signer {
                    return Err(AuthorityError::Unavailable {
                        reason: "signing host: the requested identity account is not available in \
                                 this CLI wallet"
                            .to_string(),
                    });
                }
                build_local_transaction(
                    &keypair,
                    &request.call_data,
                    &request.extensions,
                    request.tx_ext_version,
                )
            }
        }
    }

    async fn account_alias(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: AccountAliasAuthorityRequest,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        require_current_session(&self.session_state, session)?;
        match super::account_access_authorization(
            &self.services,
            &request.calling_product_id,
            &request.context.product_id,
        )
        .await
        {
            Ok(PermissionAuthorizationStatus::Authorized) => {}
            Ok(
                PermissionAuthorizationStatus::Denied
                | PermissionAuthorizationStatus::NotDetermined,
            ) => return Err(RingVrfError::Rejected),
            Err(reason) => return Err(RingVrfError::Unknown { reason }),
        }
        let collection = self.ring_resolver.validate(&request.ring_location).await?;
        let context = context_bytes(&request.context);
        let entropy = self.person_entropy(session, key_for_collection(&collection))?;
        let alias = alias_from_entropy(&entropy, &context)?;
        Ok(v01::ContextualAlias {
            context,
            alias: alias.to_vec(),
        })
    }

    async fn create_proof(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: CreateProofAuthorityRequest,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        require_current_session(&self.session_state, session)?;
        self.confirm_ring_vrf_if_cross_product(
            &request.calling_product_id,
            &request.context.product_id,
            UserConfirmationReview::CreateProof(CreateProofReview {
                calling_product_id: request.calling_product_id.clone(),
                context: request.context.clone(),
                ring_location: request.ring_location.clone(),
                message: request.message.clone(),
            }),
        )
        .await?;
        let candidates = self.member_candidates(session)?;
        let resolved = self
            .ring_resolver
            .resolve(&request.ring_location, &candidates)
            .await?;
        // Reject a stale request if the local session disconnected or changed
        // while its chain snapshot was being resolved.
        let entropy = self.person_entropy(session, resolved.selected.key)?;
        let context = context_bytes(&request.context);
        let (proof, alias) = create_proof(&entropy, &resolved, &context, &request.message)?;
        Ok(v01::HostAccountCreateProofResponse {
            proof,
            contextual_alias: v01::ContextualAlias {
                context,
                alias: alias.to_vec(),
            },
            ring_index: resolved.ring_index,
            ring_revision: resolved.ring_revision,
        })
    }

    async fn allocate_resources(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: v01::HostRequestResourceAllocationRequest,
    ) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let mut outcomes = Vec::with_capacity(request.resources.len());
        for resource in request.resources {
            let outcome = match resource {
                v01::AllocatableResource::StatementStoreAllowance => {
                    sso_responder::allocate_statement_store_allowance(
                        &self.services,
                        self,
                        &product_id,
                        OnExistingAllowancePolicy::Increase,
                    )
                    .await
                    .map(|_| v01::AllocationOutcome::Allocated)
                }
                v01::AllocatableResource::BulletinAllowance => {
                    sso_responder::allocate_bulletin_allowance(
                        &self.services,
                        self,
                        &product_id,
                        OnExistingAllowancePolicy::Increase,
                    )
                    .await
                    .map(|_| v01::AllocationOutcome::Allocated)
                }
                v01::AllocatableResource::SmartContractAllowance(_)
                | v01::AllocatableResource::AutoSigning => Ok(v01::AllocationOutcome::NotAvailable),
            };
            match outcome {
                Ok(outcome) => outcomes.push(outcome),
                Err(reason) => {
                    tracing::warn!(%product_id, %reason, "direct resource allocation item failed");
                    outcomes.push(v01::AllocationOutcome::NotAvailable);
                }
            }
        }
        Ok(v01::HostRequestResourceAllocationResponse { outcomes })
    }

    async fn statement_store_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let secret = sso_responder::allocate_statement_store_allowance(
            &self.services,
            self,
            &product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
        .map_err(allocation_authority_error)?;
        StatementStoreAllowanceKey::from_secret_bytes(secret)
    }

    async fn bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let secret = sso_responder::allocate_bulletin_allowance(
            &self.services,
            self,
            &product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
        .map_err(allocation_authority_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret)
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let secret = sso_responder::allocate_bulletin_allowance(
            &self.services,
            self,
            &product_id,
            OnExistingAllowancePolicy::Increase,
        )
        .await
        .map_err(allocation_authority_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret)
    }

    async fn sign_statement_store_product_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        account: v01::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let keypair = self.product_keypair(&account)?;
        Ok(keypair
            .secret
            .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
            .to_bytes())
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        require_current_session(&self.session_state, session)?;
        let entropy = self.root_entropy()?;
        derive_product_entropy(&entropy, product_id, context).map_err(|err| {
            AuthorityError::Unknown {
                reason: err.to_string(),
            }
        })
    }
}

fn sign_extrinsic_payload(
    keypair: &schnorrkel::Keypair,
    payload: v01::HostSignPayloadData,
) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
    if payload.version != 4 {
        return Err(AuthorityError::NotSupported {
            reason: format!(
                "signing host: unsupported extrinsic payload version {}; only version 4 is supported",
                payload.version
            ),
        });
    }
    let preimage = extrinsic_payload_preimage(&payload)
        .map_err(|reason| AuthorityError::Unknown { reason })?;
    let raw_signature = keypair
        .secret
        .sign_simple(SR25519_SIGNING_CONTEXT, &preimage, &keypair.public)
        .to_bytes();
    let signature = MultiSignature::Sr25519(raw_signature);
    let signed_transaction = payload.with_signed_transaction.unwrap_or(false).then(|| {
        let extensions = extrinsic_payload_extensions(&payload)
            .expect("preimage construction already validated signed extensions");
        build_signed_extrinsic_v4_with_signature(
            AccountId32(keypair.public.to_bytes()),
            &signature,
            &payload.method,
            &extensions,
        )
    });
    Ok(v01::HostSignPayloadResponse {
        signature: signature.encode(),
        signed_transaction,
    })
}

fn product_authority_error(err: ProductAccountError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: err.to_string(),
    }
}

fn allocation_authority_error(reason: String) -> AuthorityError {
    AuthorityError::Unavailable { reason }
}

/// Assemble and sign a transaction locally from caller-supplied, pre-encoded
/// parts. Only Extrinsic V4 (`tx_ext_version == 0`) is supported; the caller's
/// extension bytes carry the whole chain binding, so no metadata is consulted.
fn build_local_transaction(
    keypair: &schnorrkel::Keypair,
    call_data: &[u8],
    extensions: &[v01::TxPayloadExtension],
    tx_ext_version: u8,
) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
    if tx_ext_version != 0 {
        return Err(AuthorityError::NotSupported {
            reason: format!(
                "signing host: unsupported tx_ext_version {tx_ext_version}; only V4 \
                 (tx_ext_version = 0) is supported for local transaction construction"
            ),
        });
    }
    let signer = Sr25519Signer::from_keypair(keypair);
    let transaction = build_signed_extrinsic_v4(&signer, call_data, extensions);
    Ok(v01::HostCreateTransactionResponse { transaction })
}

/// Wrap raw sign-message bytes in the `<Bytes>…</Bytes>` envelope unless
/// already wrapped, matching the polkadot-app raw-signing convention.
///
/// String payloads follow the polkadot-app `isHex` rule: a `0x`-prefixed,
/// even-length string is decoded from hex, and a corrupt hex body is a hard
/// error (never silently signed as UTF-8); any other string is signed as its
/// UTF-8 bytes.
fn raw_payload_bytes(payload: v01::RawPayload) -> Result<Vec<u8>, AuthorityError> {
    let raw = match payload {
        v01::RawPayload::Bytes { bytes } => bytes,
        v01::RawPayload::Payload { payload } => decode_payload_string(payload)?,
    };
    if raw.starts_with(BYTES_WRAP_PREFIX) && raw.ends_with(BYTES_WRAP_SUFFIX) {
        return Ok(raw);
    }
    let mut wrapped =
        Vec::with_capacity(BYTES_WRAP_PREFIX.len() + raw.len() + BYTES_WRAP_SUFFIX.len());
    wrapped.extend_from_slice(BYTES_WRAP_PREFIX);
    wrapped.extend_from_slice(&raw);
    wrapped.extend_from_slice(BYTES_WRAP_SUFFIX);
    Ok(wrapped)
}

fn decode_payload_string(payload: String) -> Result<Vec<u8>, AuthorityError> {
    // `isHex`: `0x` prefix and even total length. Odd length is not hex and is
    // signed as UTF-8, matching polkadot-app.
    if let Some(body) = payload
        .strip_prefix("0x")
        .filter(|_| payload.len().is_multiple_of(2))
    {
        return hex::decode(body).map_err(|_| AuthorityError::Unknown {
            reason: "raw sign payload is 0x-prefixed but not valid hex".to_string(),
        });
    }
    Ok(payload.into_bytes())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::authority::{
        AccountAliasAuthorityRequest, AuthorityError, CreateProofAuthorityRequest,
        CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    };
    use super::super::{ProductAuthority, ProductRuntimeHost, RuntimeServices, SigningHostRole};
    use super::ring_vrf::{
        MemberCandidate, PersonKey, ResolvedRing, RingResolver, member_from_entropy, person_entropy,
    };
    use super::{
        BYTES_WRAP_PREFIX, BYTES_WRAP_SUFFIX, LocalActivation, RingVrfError,
        SR25519_SIGNING_CONTEXT, raw_payload_bytes,
    };
    use crate::host_logic::extrinsic::tests::split_v4;
    use crate::host_logic::product_account::{
        derive_product_keypair, derive_root_keypair_from_entropy, derive_sr25519_hard_path,
    };
    use crate::host_logic::transaction::{
        extrinsic_payload_extensions, extrinsic_payload_preimage,
    };
    use crate::test_support::{StubPlatform, test_spawner};
    use truapi::api::{Account, Entropy, Signing};
    use truapi::versioned::account::{HostAccountGetError, HostAccountGetRequest};
    use truapi::versioned::entropy::HostDeriveEntropyRequest;
    use truapi::versioned::signing::{HostSignRawError, HostSignRawRequest, HostSignRawResponse};
    use truapi::{CallContext, CallError, v01};
    use truapi_platform::{HostInfo, PlatformInfo, ProductContext, SigningHostConfig};
    use verifiable::ring::RingDomainSize;

    const ENTROPY: [u8; 16] = [0xAB; 16];

    #[derive(Clone)]
    struct StubRingResolver {
        collection: [u8; 32],
        ring: ResolvedRing,
    }

    #[async_trait::async_trait]
    impl RingResolver for StubRingResolver {
        async fn validate(&self, _location: &v01::RingLocation) -> Result<[u8; 32], RingVrfError> {
            Ok(self.collection)
        }

        async fn resolve(
            &self,
            _location: &v01::RingLocation,
            candidates: &[MemberCandidate],
        ) -> Result<ResolvedRing, RingVrfError> {
            assert!(
                candidates.contains(&self.ring.selected),
                "signing host offered the selected person key"
            );
            Ok(self.ring.clone())
        }
    }

    fn signing_runtime() -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
        // Auto-confirm raw signing so the role-neutral confirmation gate does
        // not reject before reaching the signing authority.
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            ..StubPlatform::default()
        });
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
        let signing_host = SigningHostRole::new(services.clone());
        (services, signing_host)
    }

    fn product_runtime(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
    ) -> ProductRuntimeHost {
        ProductRuntimeHost::from_services(
            services,
            authority,
            ProductContext::new("myapp.dot".to_string()).expect("valid product id"),
        )
    }

    fn product_runtime_for(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        product_id: &str,
    ) -> ProductRuntimeHost {
        ProductRuntimeHost::from_services(
            services,
            authority,
            ProductContext::new(product_id.to_string()).expect("valid product id"),
        )
    }

    fn full_person_ring_resolver() -> Arc<StubRingResolver> {
        let full_entropy = person_entropy(&ENTROPY, PersonKey::Full);
        let full_member = member_from_entropy(&full_entropy).expect("full-person member");
        Arc::new(StubRingResolver {
            collection: *b"pop:polkadot.network/people     ",
            ring: ResolvedRing {
                selected: MemberCandidate {
                    key: PersonKey::Full,
                    member: full_member,
                },
                ring_index: 7,
                ring_revision: 11,
                domain_size: RingDomainSize::Domain11,
                members: vec![full_member],
            },
        })
    }

    #[test]
    fn ring_alias_and_proof_share_the_selected_person_key() {
        let resolver = full_person_ring_resolver();
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform::default());
        let authority = SigningHostRole::new_with_ring_resolver(platform, resolver);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::new();
        let context = v01::ProductProofContext {
            product_id: "myapp.dot".to_string(),
            suffix: b"account".to_vec(),
        };
        let ring_location = v01::RingLocation {
            chain_id: [0x22; 32],
            junctions: vec![
                v01::RingLocationJunction::PalletInstance(42),
                v01::RingLocationJunction::CollectionId(
                    b"pop:polkadot.network/people     ".to_vec(),
                ),
            ],
        };

        let alias = futures::executor::block_on(authority.account_alias(
            &cx,
            &session,
            AccountAliasAuthorityRequest {
                calling_product_id: "myapp.dot".to_string(),
                context: context.clone(),
                ring_location: ring_location.clone(),
            },
        ))
        .expect("alias succeeds");
        let proof = futures::executor::block_on(authority.create_proof(
            &cx,
            &session,
            CreateProofAuthorityRequest {
                calling_product_id: "myapp.dot".to_string(),
                context,
                ring_location,
                message: b"prove me".to_vec(),
            },
        ))
        .expect("proof succeeds");

        assert!(!proof.proof.is_empty());
        assert_eq!(proof.contextual_alias, alias);
        assert_eq!(proof.ring_index, 7);
        assert_eq!(proof.ring_revision, 11);
    }

    #[test]
    fn cross_product_ring_requests_use_their_respective_authorization_paths() {
        let platform = Arc::new(StubPlatform::default());
        let authority =
            SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::new();
        let context = v01::ProductProofContext {
            product_id: "other.dot".to_string(),
            suffix: b"account".to_vec(),
        };
        let ring_location = v01::RingLocation {
            chain_id: [0x22; 32],
            junctions: vec![v01::RingLocationJunction::PalletInstance(42)],
        };

        let alias = futures::executor::block_on(authority.account_alias(
            &cx,
            &session,
            AccountAliasAuthorityRequest {
                calling_product_id: "myapp.dot".to_string(),
                context: context.clone(),
                ring_location: ring_location.clone(),
            },
        ));
        assert_eq!(alias, Err(RingVrfError::Rejected));

        let proof = futures::executor::block_on(authority.create_proof(
            &cx,
            &session,
            CreateProofAuthorityRequest {
                calling_product_id: "myapp.dot".to_string(),
                context,
                ring_location,
                message: b"prove me".to_vec(),
            },
        ));
        assert_eq!(proof, Err(RingVrfError::Rejected));
        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("account access review list mutex poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn cross_product_alias_reuses_persisted_account_access_grant() {
        let platform = Arc::new(StubPlatform {
            account_access_confirmed: true,
            ..StubPlatform::default()
        });
        let authority =
            SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::new();
        let request = AccountAliasAuthorityRequest {
            calling_product_id: "myapp.dot".to_string(),
            context: v01::ProductProofContext {
                product_id: "other.dot".to_string(),
                suffix: b"account".to_vec(),
            },
            ring_location: v01::RingLocation {
                chain_id: [0x22; 32],
                junctions: vec![v01::RingLocationJunction::CollectionId(
                    b"pop:polkadot.network/people     ".to_vec(),
                )],
            },
        };

        futures::executor::block_on(authority.account_alias(&cx, &session, request.clone()))
            .expect("first alias succeeds");
        futures::executor::block_on(authority.account_alias(&cx, &session, request))
            .expect("second alias succeeds from cached grant");

        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("account access review list mutex poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn activate_then_sign_raw_verifies_against_derived_product_key() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, activation);
        let cx = CallContext::new();

        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        });
        let HostSignRawResponse::V1(response) =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
        assert!(response.signed_transaction.is_none());

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", 0).unwrap();
        let signature =
            schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
                .is_ok(),
            "signature verifies over the <Bytes>-wrapped message",
        );
    }

    #[test]
    fn sign_payload_product_and_legacy_use_the_substrate_preimage() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::new();
        let mut payload = crate::test_support::sign_payload_data();
        payload.signed_extensions = vec![
            "CheckSpecVersion".to_string(),
            "CheckTxVersion".to_string(),
            "CheckGenesis".to_string(),
            "CheckMortality".to_string(),
            "CheckNonce".to_string(),
            "ChargeTransactionPayment".to_string(),
        ];
        payload.with_signed_transaction = Some(true);
        let preimage = extrinsic_payload_preimage(&payload).expect("preimage builds");

        let product_response = futures::executor::block_on(authority.sign_payload(
            &cx,
            &session,
            SignPayloadAuthorityRequest::Product(v01::HostSignPayloadRequest {
                account: product_account(0),
                payload: payload.clone(),
            }),
        ))
        .expect("product payload signing succeeds");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", 0).unwrap();
        assert_eq!(product_response.signature.len(), 65);
        assert_eq!(product_response.signature[0], 1);
        let signature =
            schnorrkel::Signature::from_bytes(&product_response.signature[1..]).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, &preimage, &signature)
                .is_ok()
        );
        let signed_transaction = product_response
            .signed_transaction
            .as_ref()
            .expect("requested signed transaction");
        let (account, embedded_signature, tail) = split_v4(signed_transaction);
        assert_eq!(account, keypair.public.to_bytes());
        assert_eq!(
            embedded_signature.as_slice(),
            &product_response.signature[1..]
        );
        let extensions = extrinsic_payload_extensions(&payload).unwrap();
        let expected_tail = extensions
            .iter()
            .flat_map(|extension| extension.extra.iter().copied())
            .chain(payload.method.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(tail, expected_tail);

        let legacy_response = futures::executor::block_on(authority.sign_payload(
            &cx,
            &session,
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account: product_account(0),
                request: v01::HostSignPayloadWithLegacyAccountRequest {
                    signer: format!("0x{}", hex::encode(keypair.public.to_bytes())),
                    payload,
                },
            },
        ))
        .expect("legacy payload signing succeeds");
        assert_eq!(legacy_response.signature[0], 1);
        let signature = schnorrkel::Signature::from_bytes(&legacy_response.signature[1..]).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, &preimage, &signature)
                .is_ok()
        );
        assert!(legacy_response.signed_transaction.is_some());
    }

    #[test]
    fn sign_raw_legacy_accepts_only_the_wallet_identity_key() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::new();
        let identity = derive_sr25519_hard_path(&ENTROPY, &["wallet", "sso"]).unwrap();
        let request = |account| SignRawAuthorityRequest::LegacyAccount {
            account,
            request: v01::HostSignRawWithLegacyAccountRequest {
                signer: String::new(),
                payload: v01::RawPayload::Bytes {
                    bytes: b"hello".to_vec(),
                },
            },
        };

        let response = futures::executor::block_on(authority.sign_raw(
            &cx,
            &session,
            request(identity.public.to_bytes()),
        ))
        .expect("identity raw signing succeeds");
        let signature = schnorrkel::Signature::from_bytes(&response.signature).unwrap();
        assert!(
            identity
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, b"<Bytes>hello</Bytes>", &signature)
                .is_ok()
        );

        let error =
            futures::executor::block_on(authority.sign_raw(&cx, &session, request([0xff; 32])))
                .expect_err("unknown legacy account is rejected");
        assert!(matches!(error, AuthorityError::Unavailable { .. }));
    }

    #[test]
    fn sign_raw_requires_active_session() {
        let (services, authority) = signing_runtime();
        let runtime = product_runtime(services, authority);
        let cx = CallContext::new();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            payload: v01::RawPayload::Bytes {
                bytes: vec![1, 2, 3],
            },
        });
        let err =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect_err("no session");
        assert!(matches!(err, CallError::Domain(HostSignRawError::V1(_))));
    }

    fn product_account(index: u32) -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: index,
        }
    }

    fn tx_payload(tx_ext_version: u8) -> v01::ProductAccountTxPayload {
        v01::ProductAccountTxPayload {
            signer: product_account(0),
            genesis_hash: [0xaa; 32],
            call_data: vec![0x00, 0x00],
            extensions: vec![v01::TxPayloadExtension {
                id: "CheckNonce".to_string(),
                extra: vec![1],
                additional_signed: vec![2, 3],
            }],
            tx_ext_version,
        }
    }

    #[test]
    fn create_transaction_product_builds_verifiable_v4() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::new();

        let response = futures::executor::block_on(activation.create_transaction(
            &cx,
            &session,
            CreateTransactionAuthorityRequest::Product(tx_payload(0)),
        ))
        .expect("create_transaction ok");

        let (account, signature, tail) = split_v4(&response.transaction);
        assert_eq!(tail, vec![1, 0x00, 0x00], "body tail is extra ++ call_data");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", 0).unwrap();
        assert_eq!(account, keypair.public.to_bytes());

        // Payload = call_data ++ extra ++ additional_signed (call first).
        let payload = vec![0x00, 0x00, 1, 2, 3];
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", &payload, &signature)
                .is_ok()
        );
    }

    #[test]
    fn create_transaction_rejects_nonzero_tx_ext_version() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::new();

        let err = futures::executor::block_on(activation.create_transaction(
            &cx,
            &session,
            CreateTransactionAuthorityRequest::Product(tx_payload(1)),
        ))
        .expect_err("v5 unsupported");
        assert!(
            matches!(err, AuthorityError::NotSupported { reason } if reason.contains("tx_ext_version 1"))
        );
    }

    #[test]
    fn create_transaction_legacy_signer_mismatch_errors() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::new();

        let payload = tx_payload(0);
        let request = CreateTransactionAuthorityRequest::LegacyAccount {
            product_account: product_account(0),
            request: v01::LegacyAccountTxPayload {
                signer: [0xff; 32], // does not match the derived slot-zero key
                genesis_hash: payload.genesis_hash,
                call_data: payload.call_data.clone(),
                extensions: payload.extensions.clone(),
                tx_ext_version: 0,
            },
        };
        let err =
            futures::executor::block_on(activation.create_transaction(&cx, &session, request))
                .expect_err("mismatched legacy signer");
        assert!(
            matches!(err, AuthorityError::Unknown { reason } if reason.contains("does not match"))
        );
    }

    #[test]
    fn create_transaction_legacy_builds_verifiable_v4() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::new();

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", 0).unwrap();

        let request = CreateTransactionAuthorityRequest::LegacyAccount {
            product_account: product_account(0),
            request: v01::LegacyAccountTxPayload {
                signer: keypair.public.to_bytes(), // matches the derived slot-zero key
                genesis_hash: [0xaa; 32],
                call_data: vec![0x00, 0x00],
                extensions: vec![v01::TxPayloadExtension {
                    id: "CheckNonce".to_string(),
                    extra: vec![1],
                    additional_signed: vec![2, 3],
                }],
                tx_ext_version: 0,
            },
        };
        let response =
            futures::executor::block_on(activation.create_transaction(&cx, &session, request))
                .expect("legacy create_transaction ok");

        let (account, signature, tail) = split_v4(&response.transaction);
        assert_eq!(account, keypair.public.to_bytes());
        assert_eq!(tail, vec![1, 0x00, 0x00]);
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", &[0x00, 0x00, 1, 2, 3], &signature)
                .is_ok()
        );
    }

    #[test]
    fn create_transaction_requires_active_session() {
        let (_services, activation) = signing_runtime();
        // A session snapshot cannot exist without activation, so construct the
        // request against a role that has never been activated.
        let (_s2, other) = signing_runtime();
        futures::executor::block_on(other.activate_local_session(ENTROPY.to_vec())).unwrap();
        let stale_session = other.current_session().expect("session");
        futures::executor::block_on(other.disconnect());
        let cx = CallContext::new();

        let err = futures::executor::block_on(activation.create_transaction(
            &cx,
            &stale_session,
            CreateTransactionAuthorityRequest::Product(tx_payload(0)),
        ))
        .expect_err("no active session");
        assert_eq!(err, AuthorityError::Disconnected);
    }

    #[test]
    fn derive_entropy_matches_ios_vector_over_local_session() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime_for(services, activation, "test.product.dot");
        let cx = CallContext::new();
        let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
            context: b"my-key".to_vec(),
        });
        let response =
            futures::executor::block_on(runtime.derive(&cx, request)).expect("derive ok");
        let truapi::versioned::entropy::HostDeriveEntropyResponse::V1(inner) = response;
        assert_eq!(
            hex::encode(inner.entropy),
            "479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a",
        );
    }

    #[test]
    fn get_account_gates_on_local_session() {
        let (services, authority) = signing_runtime();
        let runtime = product_runtime(services, authority);
        let cx = CallContext::new();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
        });
        let err = futures::executor::block_on(runtime.get_account(&cx, request))
            .expect_err("no session yet");
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::NotConnected
            ))
        ));
    }

    #[test]
    fn raw_payload_bytes_wraps_and_decodes() {
        let ok = |p| raw_payload_bytes(p).expect("payload ok");
        // Bytes are <Bytes>-wrapped.
        assert_eq!(
            ok(v01::RawPayload::Bytes {
                bytes: b"hi".to_vec()
            }),
            b"<Bytes>hi</Bytes>".to_vec(),
        );
        // A 0x-hex string payload decodes to bytes before wrapping.
        assert_eq!(
            ok(v01::RawPayload::Payload {
                payload: "0xdeadbeef".to_string(),
            }),
            [
                BYTES_WRAP_PREFIX,
                &[0xde, 0xad, 0xbe, 0xef],
                BYTES_WRAP_SUFFIX
            ]
            .concat(),
        );
        // A non-hex string payload is signed as UTF-8.
        assert_eq!(
            ok(v01::RawPayload::Payload {
                payload: "hello".to_string(),
            }),
            b"<Bytes>hello</Bytes>".to_vec(),
        );
        // An odd-length 0x string is not `isHex`, so it is signed as UTF-8.
        assert_eq!(
            ok(v01::RawPayload::Payload {
                payload: "0xabc".to_string(),
            }),
            b"<Bytes>0xabc</Bytes>".to_vec(),
        );
        // Already-wrapped input is left untouched (no double wrapping).
        assert_eq!(
            ok(v01::RawPayload::Bytes {
                bytes: b"<Bytes>hi</Bytes>".to_vec(),
            }),
            b"<Bytes>hi</Bytes>".to_vec(),
        );
        // An even-length 0x string that is not valid hex is a hard error,
        // never silently signed as UTF-8 (matches polkadot-app abort).
        assert!(matches!(
            raw_payload_bytes(v01::RawPayload::Payload {
                payload: "0xZZ".to_string(),
            }),
            Err(AuthorityError::Unknown { .. }),
        ));
    }

    #[test]
    fn sign_raw_leaves_already_wrapped_payload_untouched() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, activation);
        let cx = CallContext::new();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"<Bytes>hi</Bytes>".to_vec(),
            },
        });
        let HostSignRawResponse::V1(response) =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", 0).unwrap();
        let signature =
            schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", b"<Bytes>hi</Bytes>", &signature)
                .is_ok(),
            "signature verifies over the unchanged wrapped message",
        );
        assert!(
            keypair
                .public
                .verify_simple(
                    b"substrate",
                    b"<Bytes><Bytes>hi</Bytes></Bytes>",
                    &signature
                )
                .is_err(),
            "payload was not double-wrapped",
        );
    }

    #[test]
    fn reactivation_invalidates_prior_session_snapshot() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation");
        let stale = authority.current_session().expect("snapshot");

        // Re-activate with different entropy: a fresh public key, hence a
        // different validation id.
        futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
            .expect("second activation");
        assert_ne!(
            authority.current_session().expect("session").public_key,
            stale.public_key,
        );

        let cx = CallContext::new();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            payload: v01::RawPayload::Bytes {
                bytes: vec![1, 2, 3],
            },
        };
        let err = futures::executor::block_on(authority.sign_raw(
            &cx,
            &stale,
            SignRawAuthorityRequest::Product(request),
        ))
        .expect_err("stale snapshot rejected");
        assert_eq!(err, AuthorityError::Disconnected);
    }

    #[test]
    fn disconnect_clears_local_session() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");

        futures::executor::block_on(authority.disconnect());
        assert!(authority.current_session().is_none());

        let cx = CallContext::new();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            payload: v01::RawPayload::Bytes { bytes: vec![1] },
        };
        let err = futures::executor::block_on(authority.sign_raw(
            &cx,
            &session,
            SignRawAuthorityRequest::Product(request),
        ))
        .expect_err("no session after disconnect");
        assert_eq!(err, AuthorityError::Disconnected);
    }

    #[test]
    fn direct_allocation_handles_empty_and_optional_resource_batches() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::new();

        let empty = futures::executor::block_on(authority.allocate_resources(
            &cx,
            &session,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest { resources: vec![] },
        ))
        .expect("empty allocation succeeds");
        assert!(empty.outcomes.is_empty());

        let optional = futures::executor::block_on(authority.allocate_resources(
            &cx,
            &session,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest {
                resources: vec![
                    v01::AllocatableResource::SmartContractAllowance(0),
                    v01::AllocatableResource::AutoSigning,
                ],
            },
        ))
        .expect("optional allocation succeeds");
        assert_eq!(
            optional.outcomes,
            vec![
                v01::AllocationOutcome::NotAvailable,
                v01::AllocationOutcome::NotAvailable,
            ]
        );
    }
}
