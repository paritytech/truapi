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
//! pre-encoded, so no chain metadata is needed), RFC-0007 product entropy, and
//! bandersnatch ring-VRF product-account aliases (native only), and
//! product-scoped Statement Store and Bulletin allowance keys.

mod local_activation;
mod sso_responder;

use std::sync::{Arc, Mutex};

pub(crate) use local_activation::LocalActivation;
pub use sso_responder::ResponderExit;
pub(crate) use sso_responder::respond_to_pairing;

use super::authority::{
    AuthorityError, AuthoritySession, BulletinAllowanceKey, CreateTransactionAuthorityRequest,
    ProductAuthority, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey, authority_session, require_current_session,
};
use super::connected_session_ui_info;
use crate::host_logic::entropy::derive_product_entropy;
use crate::host_logic::extrinsic::{Sr25519Signer, build_signed_extrinsic_v4};
use crate::host_logic::product_account::{
    ProductAccountError, SR25519_SIGNING_CONTEXT, derive_product_keypair, derive_sr25519_hard_path,
};
use crate::host_logic::session::SessionState;
use crate::host_logic::sso::messages::OnExistingAllowancePolicy;
use crate::host_logic::transaction::extrinsic_payload_preimage;
use crate::runtime::auth_state::AuthStateMachine;
use crate::runtime::services::RuntimeServices;

use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};
use truapi_platform::{Platform, ProductContext, normalize_product_identifier};
use zeroize::Zeroizing;

const BYTES_WRAP_PREFIX: &[u8] = b"<Bytes>";
const BYTES_WRAP_SUFFIX: &[u8] = b"</Bytes>";

/// Wallet-local account authority for a signing host.
pub(crate) struct SigningHost {
    #[cfg(not(target_arch = "wasm32"))]
    services: Arc<RuntimeServices>,
    session_state: Arc<SessionState>,
    auth_state: AuthStateMachine,
    /// Root BIP-39 entropy held only while a session is active.
    root_entropy: Mutex<Option<Zeroizing<Vec<u8>>>>,
}

impl SigningHost {
    pub(crate) fn new(platform: Arc<dyn Platform>, services: Arc<RuntimeServices>) -> Arc<Self> {
        #[cfg(target_arch = "wasm32")]
        let _ = services;

        Arc::new(Self {
            #[cfg(not(target_arch = "wasm32"))]
            services,
            session_state: SessionState::new(),
            auth_state: AuthStateMachine::new(platform),
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

    /// Derive the product-account keypair for `account` from the wallet root.
    ///
    /// Per host-spec C.5, product keys derive from the user's main wallet
    /// account at `//wallet` (whose public key is `rootUserAccountId`), not the
    /// bare BIP-39 root. The wallet keypair is recomputed per call; the signing
    /// host holds only the raw, zeroizable entropy.
    fn product_keypair(
        &self,
        account: &v01::ProductAccountId,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        let entropy = self.root_entropy()?;
        let wallet = wallet_root_keypair(&entropy)?;
        let product_id =
            normalize_product_identifier(&account.dot_ns_identifier).map_err(|err| {
                AuthorityError::Unavailable {
                    reason: err.to_string(),
                }
            })?;
        derive_product_keypair(&wallet, &product_id, account.derivation_index)
            .map_err(product_authority_error)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn local_statement_store_allowance_key(
        &self,
        product_id: &str,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let secret =
            sso_responder::allocate_statement_store_allowance(&self.services, self, product_id)
                .await
                .map_err(allocation_error)?;
        StatementStoreAllowanceKey::from_secret_bytes(secret)
    }

    #[cfg(target_arch = "wasm32")]
    async fn local_statement_store_allowance_key(
        &self,
        _product_id: &str,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        Err(AuthorityError::Unavailable {
            reason: "signing host: statement-store allowance allocation is native-only".to_string(),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn local_bulletin_allowance_key(
        &self,
        product_id: &str,
        policy: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let secret =
            sso_responder::allocate_bulletin_allowance(&self.services, self, product_id, policy)
                .await
                .map_err(allocation_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret).map_err(AuthorityError::from)
    }

    #[cfg(target_arch = "wasm32")]
    async fn local_bulletin_allowance_key(
        &self,
        _product_id: &str,
        _policy: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        Err(AuthorityError::Unavailable {
            reason: "signing host: Bulletin allowance allocation is native-only".to_string(),
        })
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
        let (account, payload) = match request {
            SignPayloadAuthorityRequest::Product(request) => (request.account, request.payload),
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => (product_account, request.payload),
        };
        require_current_session(&self.session_state, session)?;
        let keypair = self.product_keypair(&account)?;
        let message = extrinsic_payload_preimage(&payload);
        let signature = keypair
            .secret
            .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
            .to_bytes();
        Ok(v01::HostSignPayloadResponse {
            signature: signature.to_vec(),
            signed_transaction: None,
        })
    }

    async fn sign_raw(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: SignRawAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let (account, payload) = match request {
            SignRawAuthorityRequest::Product(request) => (request.account, request.payload),
            SignRawAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => (product_account, request.payload),
        };
        require_current_session(&self.session_state, session)?;
        let keypair = self.product_keypair(&account)?;
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
        }
    }

    async fn account_alias(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_account_id: v01::ProductAccountId,
        _requesting_product_id: String,
    ) -> Result<v01::HostAccountGetAliasResponse, AuthorityError> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (session, product_account_id);
            Err(AuthorityError::Unavailable {
                reason: "signing host: ring-VRF alias derivation is native-only".to_string(),
            })
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            require_current_session(&self.session_state, session)?;
            let entropy = self.root_entropy()?;
            let product_id = normalize_product_identifier(&product_account_id.dot_ns_identifier)
                .map_err(|err| AuthorityError::Unavailable {
                    reason: err.to_string(),
                })?;
            let alias = crate::host_logic::alias::derive_product_alias(
                &entropy,
                &product_id,
                product_account_id.derivation_index,
            )
            .map_err(|reason| AuthorityError::Unknown { reason })?;
            Ok(v01::HostAccountGetAliasResponse {
                context: alias.context,
                alias: alias.alias.to_vec(),
            })
        }
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
                    self.local_statement_store_allowance_key(&product_id)
                        .await?;
                    v01::AllocationOutcome::Allocated
                }
                v01::AllocatableResource::BulletinAllowance => {
                    self.local_bulletin_allowance_key(
                        &product_id,
                        OnExistingAllowancePolicy::Ignore,
                    )
                    .await?;
                    v01::AllocationOutcome::Allocated
                }
                v01::AllocatableResource::SmartContractAllowance(_)
                | v01::AllocatableResource::AutoSigning => v01::AllocationOutcome::NotAvailable,
            };
            outcomes.push(outcome);
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
        self.local_statement_store_allowance_key(&product_id).await
    }

    async fn bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        self.local_bulletin_allowance_key(&product_id, OnExistingAllowancePolicy::Ignore)
            .await
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        require_current_session(&self.session_state, session)?;
        self.local_bulletin_allowance_key(&product_id, OnExistingAllowancePolicy::Increase)
            .await
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

fn product_authority_error(err: ProductAccountError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: err.to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn allocation_error(reason: String) -> AuthorityError {
    AuthorityError::Unavailable { reason }
}

/// The user's main wallet keypair at `//wallet` (host-spec C.0), the root of
/// product-account derivation and the `rootUserAccountId` shared with paired
/// hosts.
pub(crate) fn wallet_root_keypair(entropy: &[u8]) -> Result<schnorrkel::Keypair, AuthorityError> {
    derive_sr25519_hard_path(entropy, &["wallet"]).map_err(product_authority_error)
}

#[cfg(test)]
fn derive_bulletin_allowance_key(
    entropy: &[u8],
    product_id: &str,
) -> Result<BulletinAllowanceKey, AuthorityError> {
    let allowance = derive_sr25519_hard_path(entropy, &["allowance", "bulletin", product_id])
        .map_err(product_authority_error)?;
    BulletinAllowanceKey::from_secret_bytes(allowance.secret.to_bytes().to_vec())
        .map_err(AuthorityError::from)
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
        AuthorityError, CreateTransactionAuthorityRequest, SignRawAuthorityRequest,
    };
    use super::super::{ProductAuthority, ProductRuntimeHost, RuntimeServices, SigningHostRole};
    use super::{
        BYTES_WRAP_PREFIX, BYTES_WRAP_SUFFIX, LocalActivation, derive_bulletin_allowance_key,
        raw_payload_bytes,
    };
    use crate::host_logic::extrinsic::tests::split_v4;
    use crate::host_logic::product_account::{derive_product_keypair, derive_sr25519_hard_path};
    use crate::test_support::{StubPlatform, test_spawner};
    use truapi::api::{Account, Entropy, Signing};
    use truapi::versioned::account::{HostAccountGetError, HostAccountGetRequest};
    use truapi::versioned::entropy::HostDeriveEntropyRequest;
    use truapi::versioned::signing::{HostSignRawError, HostSignRawRequest, HostSignRawResponse};
    use truapi::{CallContext, CallError, v01};
    use truapi_platform::{HostInfo, PlatformInfo, ProductContext, SigningHostConfig};

    const ENTROPY: [u8; 16] = [0xAB; 16];

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
        let signing_host = SigningHostRole::new(platform, services.clone());
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

        let wallet = derive_sr25519_hard_path(&ENTROPY, &["wallet"]).unwrap();
        let keypair = derive_product_keypair(&wallet, "myapp.dot", 0).unwrap();
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

        let wallet = derive_sr25519_hard_path(&ENTROPY, &["wallet"]).unwrap();
        let keypair = derive_product_keypair(&wallet, "myapp.dot", 0).unwrap();
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

        let wallet = derive_sr25519_hard_path(&ENTROPY, &["wallet"]).unwrap();
        let keypair = derive_product_keypair(&wallet, "myapp.dot", 0).unwrap();

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
        let wallet = derive_sr25519_hard_path(&ENTROPY, &["wallet"]).unwrap();
        let keypair = derive_product_keypair(&wallet, "myapp.dot", 0).unwrap();
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
    fn sign_payload_verifies_against_derived_product_key() {
        use super::super::authority::SignPayloadAuthorityRequest;
        use crate::host_logic::transaction::extrinsic_payload_preimage;

        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::new();
        let payload = v01::HostSignPayloadData {
            block_hash: vec![0xB1; 32],
            block_number: vec![0x01],
            era: vec![0x00],
            genesis_hash: vec![0x61; 32],
            method: vec![0x4D, 0x00],
            nonce: vec![0x00],
            spec_version: vec![0x51],
            tip: vec![0x00],
            transaction_version: vec![0x56],
            signed_extensions: vec![],
            version: 4,
            asset_id: None,
            metadata_hash: None,
            mode: None,
            with_signed_transaction: None,
        };
        let request = v01::HostSignPayloadRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            payload: payload.clone(),
        };

        let response = futures::executor::block_on(authority.sign_payload(
            &cx,
            &session,
            SignPayloadAuthorityRequest::Product(request),
        ))
        .expect("sign_payload ok");

        assert!(response.signed_transaction.is_none());
        let wallet = derive_sr25519_hard_path(&ENTROPY, &["wallet"]).unwrap();
        let keypair = derive_product_keypair(&wallet, "myapp.dot", 0).unwrap();
        let signature =
            schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
        assert!(
            keypair
                .public
                .verify_simple(
                    b"substrate",
                    &extrinsic_payload_preimage(&payload),
                    &signature
                )
                .is_ok(),
            "signature verifies over the payload preimage",
        );
    }

    #[test]
    fn create_transaction_builds_verifiable_v4_extrinsic() {
        use super::super::authority::CreateTransactionAuthorityRequest;
        use crate::host_logic::transaction::transaction_signing_preimage;

        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::new();
        let extensions = vec![v01::TxPayloadExtension {
            id: "CheckNonce".to_string(),
            extra: vec![0x04],
            additional_signed: vec![],
        }];
        let payload = v01::ProductAccountTxPayload {
            signer: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            genesis_hash: [0x61; 32],
            call_data: vec![0x00, 0x00],
            extensions: extensions.clone(),
            tx_ext_version: 0,
        };

        let response = futures::executor::block_on(authority.create_transaction(
            &cx,
            &session,
            CreateTransactionAuthorityRequest::Product(payload),
        ))
        .expect("create_transaction ok");

        let wallet = derive_sr25519_hard_path(&ENTROPY, &["wallet"]).unwrap();
        let keypair = derive_product_keypair(&wallet, "myapp.dot", 0).unwrap();
        let transaction = response.transaction;
        let mut body = transaction.as_slice();
        let body_len =
            <parity_scale_codec::Compact<u32> as parity_scale_codec::Decode>::decode(&mut body)
                .expect("compact length prefix")
                .0 as usize;
        assert_eq!(body.len(), body_len);
        assert_eq!(body[0], 0x84);
        assert_eq!(body[1], 0x00);
        assert_eq!(&body[2..34], &keypair.public.to_bytes());
        assert_eq!(body[34], 0x01);
        let signature = schnorrkel::Signature::from_bytes(&body[35..99]).unwrap();
        assert_eq!(body[99], 0x04);
        assert_eq!(&body[100..], &[0x00, 0x00]);
        assert!(
            keypair
                .public
                .verify_simple(
                    b"substrate",
                    &transaction_signing_preimage(&[0x00, 0x00], &extensions),
                    &signature
                )
                .is_ok(),
            "extrinsic signature verifies over call ++ extra ++ implicit",
        );
    }

    #[test]
    fn create_transaction_rejects_v5_extension_version() {
        use super::super::authority::CreateTransactionAuthorityRequest;

        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::new();
        let payload = v01::ProductAccountTxPayload {
            signer: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
            genesis_hash: [0x61; 32],
            call_data: vec![],
            extensions: vec![],
            tx_ext_version: 1,
        };

        let err = futures::executor::block_on(authority.create_transaction(
            &cx,
            &session,
            CreateTransactionAuthorityRequest::Product(payload),
        ))
        .expect_err("v5 rejected");

        assert!(matches!(err, AuthorityError::NotSupported { .. }));
    }

    #[test]
    fn account_alias_returns_ring_vrf_alias() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::new();

        let alias = futures::executor::block_on(authority.account_alias(
            &cx,
            &session,
            v01::ProductAccountId {
                dot_ns_identifier: "truapi-playground.dot".to_string(),
                derivation_index: 0,
            },
            "truapi-playground.dot".to_string(),
        ))
        .expect("alias derives");

        let expected =
            crate::host_logic::alias::derive_product_alias(&ENTROPY, "truapi-playground.dot", 0)
                .unwrap();
        assert_eq!(alias.context, expected.context);
        assert_eq!(alias.alias, expected.alias.to_vec());
    }

    #[test]
    fn empty_resource_allocation_returns_empty_response() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::new();

        let alloc = futures::executor::block_on(authority.allocate_resources(
            &cx,
            &session,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest { resources: vec![] },
        ))
        .expect("empty allocation");
        assert!(alloc.outcomes.is_empty());
    }

    #[test]
    fn bulletin_allowance_key_uses_product_scoped_ios_path() {
        let key = derive_bulletin_allowance_key(&ENTROPY, "truapi-playground.dot")
            .expect("bulletin allowance key");
        let expected = derive_sr25519_hard_path(
            &ENTROPY,
            &["allowance", "bulletin", "truapi-playground.dot"],
        )
        .unwrap();

        assert_eq!(key.as_secret_bytes(), &expected.secret.to_bytes());
    }
}
