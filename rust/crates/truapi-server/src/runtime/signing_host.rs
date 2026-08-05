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

use std::collections::HashSet;
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
    authority_session_validation_id,
};
use super::{RuntimeServices, connected_session_ui_info, validate_vrf_transcript};
use crate::host_logic::entropy::derive_product_entropy;
use crate::host_logic::extrinsic::{
    Sr25519Signer, build_signed_extrinsic_v4, build_signed_extrinsic_v4_with_signature,
};
use crate::host_logic::product_account::{
    ProductAccountError, SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_identity_keypair,
    derive_product_keypair, derive_product_subtree_keypair, derive_root_keypair_from_entropy,
};
use crate::host_logic::session::{SessionInfo, SessionState};
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
    CreateProofReview, PermissionAuthorizationStatus, Platform, ProductContext, SignVrfReview,
    UserConfirmationReview, normalize_product_identifier,
};
use zeroize::Zeroizing;

const BYTES_WRAP_PREFIX: &[u8] = b"<Bytes>";
const BYTES_WRAP_SUFFIX: &[u8] = b"</Bytes>";

#[derive(Default)]
struct LocalGrantState {
    activation_generation: u64,
    auto_signing_grants: HashSet<([u8; 32], String)>,
}

impl LocalGrantState {
    fn advance_activation(&mut self) {
        self.activation_generation = self
            .activation_generation
            .checked_add(1)
            .expect("local activation generation exhausted");
        self.auto_signing_grants.clear();
    }

    fn revoke_product(&mut self, product_id: &str) {
        self.activation_generation = self
            .activation_generation
            .checked_add(1)
            .expect("local activation generation exhausted");
        self.auto_signing_grants
            .retain(|(_, granted_product_id)| granted_product_id != product_id);
    }
}

/// Wallet-local account authority for a signing host.
pub(crate) struct SigningHost {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    session_state: Arc<SessionState>,
    auth_state: AuthStateMachine,
    ring_resolver: Arc<dyn RingResolver>,
    /// Root BIP-39 entropy held only while a session is active.
    root_entropy: Mutex<Option<Zeroizing<Vec<u8>>>>,
    /// In-memory grants and the activation generation that owns them. The
    /// lifecycle mutex also makes session replacement and snapshot creation
    /// atomic with respect to generation changes.
    local_grants: Mutex<LocalGrantState>,
}

impl SigningHost {
    /// Build a signing host with no active session.
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
            local_grants: Mutex::new(LocalGrantState::default()),
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
            local_grants: Mutex::new(LocalGrantState::default()),
        })
    }

    /// Shared session holder for connection-status subscriptions.
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

    fn product_subtree_secret(&self, product_id: &str) -> Result<[u8; 64], AuthorityError> {
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        derive_product_subtree_keypair(&root, &product_id)
            .map(|keypair| keypair.secret.to_bytes())
            .map_err(product_authority_error)
    }

    fn grant_auto_signing(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> Result<(), AuthorityError> {
        let (_, activation_generation) = self.require_current_session(session)?;
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let owner = root.public.to_bytes();
        if owner != session.public_key {
            return Err(AuthorityError::Disconnected);
        }
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        derive_product_subtree_keypair(&root, &product_id).map_err(product_authority_error)?;

        let mut state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        if state.activation_generation != activation_generation {
            return Err(AuthorityError::Disconnected);
        }
        state.auto_signing_grants.insert((owner, product_id));
        Ok(())
    }

    fn has_auto_signing_grant(
        &self,
        activation_generation: u64,
        owner: [u8; 32],
        calling_product_id: &str,
        account_product_id: &str,
    ) -> bool {
        let (Ok(calling_product_id), Ok(account_product_id)) = (
            normalize_product_identifier(calling_product_id),
            normalize_product_identifier(account_product_id),
        ) else {
            return false;
        };
        if calling_product_id != account_product_id {
            return false;
        }

        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        state.activation_generation == activation_generation
            && state
                .auto_signing_grants
                .contains(&(owner, calling_product_id))
    }

    /// Fence in-flight grant work and revoke this product's grants from the
    /// current local activation while preserving unrelated products.
    pub(crate) fn clear_product_state(&self, product_id: &str) -> Result<(), AuthorityError> {
        let product_id = normalize_product_identifier(product_id).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        self.local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned")
            .revoke_product(&product_id);
        Ok(())
    }

    /// Derive the product-account keypair for `account` from the root entropy.
    ///
    /// The root keypair is recomputed per call (PBKDF2, 2048 rounds, via
    /// `substrate-bip39`) rather than cached: the signing host holds only the
    /// raw, zeroizable entropy, never an expanded secret key.
    fn product_keypair_with_owner(
        &self,
        account: &v01::ProductAccountId,
    ) -> Result<([u8; 32], schnorrkel::Keypair), AuthorityError> {
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let owner = root.public.to_bytes();
        let product_id =
            normalize_product_identifier(&account.dot_ns_identifier).map_err(|err| {
                AuthorityError::Unavailable {
                    reason: err.to_string(),
                }
            })?;
        derive_product_keypair(
            &root,
            &product_id,
            derivation_index_bytes(&account.derivation_index),
        )
        .map(|keypair| (owner, keypair))
        .map_err(product_authority_error)
    }

    fn product_keypair(
        &self,
        account: &v01::ProductAccountId,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        self.product_keypair_with_owner(account)
            .map(|(_, keypair)| keypair)
    }

    fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
        let entropy = self.root_entropy()?;
        derive_identity_keypair(&entropy).map_err(product_authority_error)
    }

    fn install_local_session(&self, secret: Zeroizing<Vec<u8>>, session: SessionInfo) {
        let mut state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        state.advance_activation();
        *self
            .root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned") = Some(secret);
        self.session_state.set_session(session);
    }

    fn clear_local_session(&self) {
        let mut state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        state.advance_activation();
        self.root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned")
            .take();
        self.session_state.clear_session();
    }

    fn current_local_session(&self) -> Option<AuthoritySession> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        let session = self.session_state.current()?;
        Some(AuthoritySession::from_session_info(
            &session,
            local_session_validation_id(&session, state.activation_generation),
        ))
    }

    fn require_current_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<(SessionInfo, u64), AuthorityError> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        let current = self
            .session_state
            .current()
            .ok_or(AuthorityError::Disconnected)?;
        if local_session_validation_id(&current, state.activation_generation)
            != session.validation_id
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok((current, state.activation_generation))
    }

    fn person_entropy(
        &self,
        session: &AuthoritySession,
        key: PersonKey,
    ) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
        self.require_current_session(session)?;
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
        self.current_local_session()
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
        self.clear_local_session();
        self.auth_state.store_disconnected();
    }

    async fn product_subtree_public_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        self.require_current_session(session)?;
        let product_id = normalize_product_identifier(&product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        derive_product_subtree_keypair(&root, &product_id)
            .map(|keypair| keypair.public.to_bytes())
            .map_err(product_authority_error)
    }

    async fn sign_vrf(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: String,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        let (_, activation_generation) = self.require_current_session(session)?;
        validate_vrf_transcript(&request).map_err(|reason| AuthorityError::Unknown { reason })?;
        let (owner, keypair) = self.product_keypair_with_owner(&request.account)?;
        if !self.has_auto_signing_grant(
            activation_generation,
            owner,
            &calling_product_id,
            &request.account.dot_ns_identifier,
        ) {
            let confirmed = self
                .platform
                .confirm_user_action(UserConfirmationReview::SignVrf(SignVrfReview {
                    calling_product_id,
                    request: request.clone(),
                }))
                .await
                .map_err(|err| AuthorityError::Unknown {
                    reason: format!("VRF signing confirmation failed: {err:?}"),
                })?;
            if !confirmed {
                return Err(AuthorityError::Rejected);
            }
        }
        let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
            &keypair,
            &request.transcript_label,
            request
                .items
                .iter()
                .map(|item| (item.label.as_slice(), item.value.as_slice())),
        );
        Ok(v01::VrfSignature { pre_output, proof })
    }

    async fn sign_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        self.require_current_session(session)?;
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
        self.require_current_session(session)?;
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
        self.require_current_session(session)?;
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
        self.require_current_session(session)?;
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
            Err(err) => {
                return Err(RingVrfError::Unknown {
                    reason: err.to_string(),
                });
            }
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
        self.require_current_session(session)?;
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
        self.require_current_session(session)?;
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
                v01::AllocatableResource::SmartContractAllowance(_) => {
                    Ok(v01::AllocationOutcome::NotAvailable)
                }
                v01::AllocatableResource::AutoSigning => self
                    .grant_auto_signing(session, &product_id)
                    .map(|_| v01::AllocationOutcome::Allocated)
                    .map_err(sso_responder::AllowanceAllocationError::Authority),
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
        self.require_current_session(session)?;
        let secret = sso_responder::allocate_statement_store_allowance(
            &self.services,
            self,
            &product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
        .map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
        StatementStoreAllowanceKey::from_secret_bytes(secret)
    }

    async fn bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        self.require_current_session(session)?;
        let secret = sso_responder::allocate_bulletin_allowance(
            &self.services,
            self,
            &product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
        .map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret)
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        self.require_current_session(session)?;
        let secret = sso_responder::allocate_bulletin_allowance(
            &self.services,
            self,
            &product_id,
            OnExistingAllowancePolicy::Increase,
        )
        .await
        .map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret)
    }

    async fn sign_statement_store_product_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        account: v01::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        self.require_current_session(session)?;
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
        self.require_current_session(session)?;
        let entropy = self.root_entropy()?;
        derive_product_entropy(&entropy, product_id, context).map_err(|err| {
            AuthorityError::Unknown {
                reason: err.to_string(),
            }
        })
    }
}

fn local_session_validation_id(session: &SessionInfo, activation_generation: u64) -> Vec<u8> {
    let mut id = authority_session_validation_id(session);
    id.extend_from_slice(b":activation:");
    id.extend_from_slice(&activation_generation.to_le_bytes());
    id
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
    let preimage = extrinsic_payload_preimage(&payload).map_err(|err| AuthorityError::Unknown {
        reason: err.to_string(),
    })?;
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
        derive_identity_keypair, derive_product_keypair, derive_root_keypair_from_entropy,
        index_bytes,
    };
    use crate::host_logic::transaction::{
        extrinsic_payload_extensions, extrinsic_payload_preimage,
    };
    use crate::test_support::{StubPlatform, test_spawner};
    use truapi::api::{Account, Entropy, ResourceAllocation, Signing};
    use truapi::versioned::account::{HostAccountGetError, HostAccountGetRequest};
    use truapi::versioned::entropy::HostDeriveEntropyRequest;
    use truapi::versioned::resource_allocation::{
        HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
    };
    use truapi::versioned::signing::{HostSignRawError, HostSignRawRequest, HostSignRawResponse};
    use truapi::{CallContext, CallError, v01};
    use truapi_platform::{HostInfo, Platform, PlatformInfo, ProductContext, SigningHostConfig};
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
        signing_runtime_with_platform(Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            sign_vrf_confirmed: true,
            ..StubPlatform::default()
        }))
    }

    fn signing_runtime_with_platform(
        platform: Arc<dyn Platform>,
    ) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
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

    fn vrf_request(product_id: &str) -> v01::HostAccountSignVrfRequest {
        v01::HostAccountSignVrfRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: product_id.to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
            },
            transcript_label: b"pop:autosigning".to_vec(),
            items: vec![v01::VrfTranscriptItem {
                label: b"round".to_vec(),
                value: vec![1],
            }],
        }
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
        let cx = CallContext::default();
        let context = v01::ProductProofContext {
            product_id: "myapp.dot".to_string(),
            suffix: v01::DerivationIndex::Left(0),
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
        let cx = CallContext::default();
        let context = v01::ProductProofContext {
            product_id: "other.dot".to_string(),
            suffix: v01::DerivationIndex::Left(0),
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
        let cx = CallContext::default();
        let request = AccountAliasAuthorityRequest {
            calling_product_id: "myapp.dot".to_string(),
            context: v01::ProductProofContext {
                product_id: "other.dot".to_string(),
                suffix: v01::DerivationIndex::Left(0),
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
    fn local_activation_exposes_the_uid_dot_identity_account() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");

        let session = authority.current_session().expect("active session");
        let identity = derive_identity_keypair(&ENTROPY)
            .expect("uid.dot identity derivation")
            .public
            .to_bytes();
        assert_eq!(session.identity_account_id, Some(identity));
    }

    #[test]
    fn activate_then_sign_raw_verifies_against_derived_product_key() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, activation);
        let cx = CallContext::default();

        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        });
        let HostSignRawResponse::V1(response) =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
        assert!(response.signed_transaction.is_none());

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
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
    fn sign_vrf_replays_transcript_and_returns_verifiable_proof() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let request = v01::HostAccountSignVrfRequest {
            account: product_account(0),
            transcript_label: b"pop:airdrop".to_vec(),
            items: vec![
                v01::VrfTranscriptItem {
                    label: b"domain".to_vec(),
                    value: b"lottery".to_vec(),
                },
                v01::VrfTranscriptItem {
                    label: b"round".to_vec(),
                    value: 7u32.to_le_bytes().to_vec(),
                },
            ],
        };

        let signature = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &session,
            "myapp.dot".to_string(),
            request,
        ))
        .expect("VRF signing succeeds");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let mut transcript = merlin::Transcript::new(b"pop:airdrop");
        transcript.append_message(b"domain", b"lottery");
        transcript.append_message(b"round", &7u32.to_le_bytes());
        let pre_output = schnorrkel::vrf::VRFPreOut::from_bytes(&signature.pre_output).unwrap();
        let proof = schnorrkel::vrf::VRFProof::from_bytes(&signature.proof).unwrap();
        keypair
            .public
            .vrf_verify(transcript, &pre_output, &proof)
            .expect("VRF proof verifies");
    }

    #[test]
    fn approved_auto_signing_product_skips_vrf_confirmation_but_other_product_does_not() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, authority.clone());
        let allocation = futures::executor::block_on(ResourceAllocation::request(
            &runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("approved AutoSigning allocation succeeds");
        let HostRequestResourceAllocationResponse::V1(allocation) = allocation;
        assert_eq!(allocation.outcomes, vec![v01::AllocationOutcome::Allocated],);
        assert_eq!(
            platform
                .resource_allocation_reviews
                .lock()
                .expect("resource allocation review list mutex poisoned")
                .len(),
            1,
        );

        let session = authority.current_session().expect("active session");
        futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &session,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect("granted product signs without another confirmation");
        assert!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .is_empty(),
            "the allocation grant bypasses only the subsequent VRF prompt",
        );

        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &session,
            "other.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("different calling product remains confirmation-bound");
        assert_eq!(error, AuthorityError::Rejected);
        let reviews = platform
            .sign_vrf_reviews
            .lock()
            .expect("VRF signing review list mutex poisoned");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].calling_product_id, "other.dot");
    }

    #[test]
    fn product_clear_revokes_only_current_activation_grant_and_fences_stale_work() {
        let platform = Arc::new(StubPlatform::default());
        let (_services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let stale_session = authority.current_session().expect("active session");
        authority
            .grant_auto_signing(&stale_session, "myapp.dot")
            .expect("first product grant succeeds");
        authority
            .grant_auto_signing(&stale_session, "other.dot")
            .expect("other product grant succeeds");

        authority
            .clear_product_state("myapp.dot")
            .expect("product clear succeeds");

        let current_session = authority.current_session().expect("session remains active");
        let (_, current_generation) = authority
            .require_current_session(&current_session)
            .expect("current session validates");
        assert!(!authority.has_auto_signing_grant(
            current_generation,
            current_session.public_key,
            "myapp.dot",
            "myapp.dot",
        ));
        assert!(authority.has_auto_signing_grant(
            current_generation,
            current_session.public_key,
            "other.dot",
            "other.dot",
        ));
        assert!(matches!(
            authority.grant_auto_signing(&stale_session, "myapp.dot"),
            Err(AuthorityError::Disconnected)
        ));
    }

    #[test]
    fn auto_signing_grant_does_not_cross_root_identity_replacement() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation succeeds");
        let runtime = product_runtime(services, authority.clone());
        futures::executor::block_on(ResourceAllocation::request(
            &runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("AutoSigning allocation succeeds");

        futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
            .expect("replacement activation succeeds");
        let replacement = authority.current_session().expect("replacement session");
        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &replacement,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("replacement root must receive its own confirmation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn auto_signing_grant_does_not_cross_disconnect_and_same_wallet_reactivation() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation succeeds");
        let runtime = product_runtime(services, authority.clone());
        futures::executor::block_on(ResourceAllocation::request(
            &runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("AutoSigning allocation succeeds");

        futures::executor::block_on(authority.disconnect());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("same wallet reactivation succeeds");
        let reactivated = authority.current_session().expect("reactivated session");
        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &reactivated,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("reactivated wallet must receive its own confirmation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn stale_auto_signing_completion_cannot_grant_same_wallet_reactivation() {
        let platform = Arc::new(StubPlatform {
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (_services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation succeeds");
        let stale = authority.current_session().expect("first session snapshot");

        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("same wallet replacement activation succeeds");
        let current = authority.current_session().expect("replacement session");
        assert_ne!(stale.validation_id, current.validation_id);

        let error = futures::executor::block_on(authority.allocate_resources(
            &CallContext::default(),
            &stale,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            },
        ))
        .expect_err("completion captured from the old activation is stale");
        assert_eq!(error, AuthorityError::Disconnected);

        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &current,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("stale allocation must not grant the replacement activation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn auto_signing_grant_does_not_cross_runtime_instance() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, granting_authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(granting_authority.activate_local_session(ENTROPY.to_vec()))
            .expect("granting runtime activates");
        let granting_runtime = product_runtime(services, granting_authority);
        futures::executor::block_on(ResourceAllocation::request(
            &granting_runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("AutoSigning allocation succeeds");

        let (_replacement_services, replacement) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(replacement.activate_local_session(ENTROPY.to_vec()))
            .expect("replacement runtime activates with the same root");
        let session = replacement.current_session().expect("replacement session");
        let error = futures::executor::block_on(replacement.sign_vrf(
            &CallContext::default(),
            &session,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("a separate runtime must receive its own confirmation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn sign_payload_product_and_legacy_use_the_substrate_preimage() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
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
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
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
    fn sign_raw_legacy_accepts_only_the_uid_dot_identity_key() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
        let identity = derive_identity_keypair(&ENTROPY).unwrap();
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
        let cx = CallContext::default();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
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
            derivation_index: v01::DerivationIndex::Left(index),
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
        let cx = CallContext::default();

        let response = futures::executor::block_on(activation.create_transaction(
            &cx,
            &session,
            CreateTransactionAuthorityRequest::Product(tx_payload(0)),
        ))
        .expect("create_transaction ok");

        let (account, signature, tail) = split_v4(&response.transaction);
        assert_eq!(tail, vec![1, 0x00, 0x00], "body tail is extra ++ call_data");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
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
        let cx = CallContext::default();

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
        let cx = CallContext::default();

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
        let cx = CallContext::default();

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();

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
        let cx = CallContext::default();

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
        let cx = CallContext::default();
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
        let cx = CallContext::default();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
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
        let cx = CallContext::default();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"<Bytes>hi</Bytes>".to_vec(),
            },
        });
        let HostSignRawResponse::V1(response) =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
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

        let cx = CallContext::default();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
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

        let cx = CallContext::default();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Left(0),
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
        let cx = CallContext::default();

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
                    v01::AllocatableResource::SmartContractAllowance(v01::DerivationIndex::Left(0)),
                    v01::AllocatableResource::AutoSigning,
                ],
            },
        ))
        .expect("optional allocation succeeds");
        assert_eq!(
            optional.outcomes,
            vec![
                v01::AllocationOutcome::NotAvailable,
                v01::AllocationOutcome::Allocated,
            ]
        );
    }
}
