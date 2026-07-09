//! Role-neutral account authority contracts used by product runtimes.
//!
//! Pairing and signing hosts implement these traits differently, but
//! `ProductRuntimeHost` can use this module's shared request/session types
//! without knowing where the key material lives.

use async_trait::async_trait;
use core::fmt;
use core::time::Duration;
use std::sync::Arc;
use truapi::latest::{
    HostAccountGetAliasResponse, HostCreateTransactionResponse,
    HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
    HostSignPayloadRequest, HostSignPayloadResponse, HostSignPayloadWithLegacyAccountRequest,
    HostSignRawRequest, HostSignRawWithLegacyAccountRequest, LegacyAccountTxPayload,
    ProductAccountId, ProductAccountTxPayload,
};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, CancellationReason};
use truapi_platform::{BulletinAllowanceKeyError, ProductContext};

pub(crate) use truapi_platform::BulletinAllowanceKey;

use crate::host_logic::session::{SessionInfo, SessionState};
use crate::host_logic::statement_store::statement_public_key_from_secret;

/// Snapshot of an account-authority session selected by the authority.
///
/// This is the neutral session projection product runtimes can use while
/// preserving authority-private material inside the concrete authority
/// implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthoritySession {
    /// Root account public key for the active authority session.
    pub public_key: [u8; 32],
    /// Identity account resolved from the signing host, when available.
    pub identity_account_id: Option<[u8; 32]>,
    /// Lightweight username resolved from People-chain identity, when available.
    pub lite_username: Option<String>,
    /// Fully qualified username resolved from People-chain identity, when available.
    pub full_username: Option<String>,
    /// Opaque session token used to reject stale pre-confirmation snapshots.
    pub validation_id: Vec<u8>,
}

impl AuthoritySession {
    pub(crate) fn from_session_info(info: &SessionInfo, validation_id: Vec<u8>) -> Self {
        Self {
            public_key: info.public_key,
            identity_account_id: info.identity_account_id,
            lite_username: info.lite_username.clone(),
            full_username: info.full_username.clone(),
            validation_id,
        }
    }
}

/// Typed account-authority failure before it is mapped to an API-specific error.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub(crate) enum AuthorityError {
    /// User or authority rejected the request.
    #[display("Rejected")]
    Rejected,
    /// The selected authority session is no longer active.
    #[display("Disconnected")]
    Disconnected,
    /// The authority call was cancelled before completion.
    #[display("{_0}")]
    Cancelled(AuthorityCancelError),
    /// The authority cannot service the request.
    #[display("{reason}")]
    Unavailable { reason: String },
    /// The authority cannot service this request shape (e.g. an unsupported
    /// transaction-extension version).
    #[display("{reason}")]
    NotSupported { reason: String },
    /// Catch-all authority failure.
    #[display("{reason}")]
    Unknown { reason: String },
}

impl AuthorityError {
    pub(crate) fn reason(self) -> String {
        self.to_string()
    }
}

impl From<BulletinAllowanceKeyError> for AuthorityError {
    fn from(err: BulletinAllowanceKeyError) -> Self {
        AuthorityError::Unavailable {
            reason: err.to_string(),
        }
    }
}

/// Cancellation cause for an account-authority call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorityCancelError {
    request_id: String,
    reason: CancellationReason,
}

impl AuthorityCancelError {
    pub(crate) fn new(request_id: &str, reason: CancellationReason) -> Self {
        Self {
            request_id: request_id.to_string(),
            reason,
        }
    }
}

impl fmt::Display for AuthorityCancelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let request = if self.request_id.is_empty() {
            String::new()
        } else {
            format!(" for {}", self.request_id)
        };
        match &self.reason {
            CancellationReason::Cancelled => {
                write!(f, "Account authority request cancelled{request}")
            }
            CancellationReason::TimedOut { timeout } => write!(
                f,
                "Account authority request timed out after {}{request}",
                format_timeout_duration(*timeout)
            ),
        }
    }
}

fn format_timeout_duration(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

/// Payload-signing request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignPayloadAuthorityRequest {
    /// Sign a payload with a product-derived account.
    Product(HostSignPayloadRequest),
    /// Sign a payload through the legacy-account API.
    LegacyAccount {
        /// Product slot-zero account that backs the validated legacy signer.
        product_account: ProductAccountId,
        /// Original legacy-account request.
        request: HostSignPayloadWithLegacyAccountRequest,
    },
}

/// Raw-signing request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignRawAuthorityRequest {
    /// Sign raw data with a product-derived account.
    Product(HostSignRawRequest),
    /// Sign raw data through the legacy-account API using the product slot-zero account.
    LegacyAccount {
        /// Product slot-zero account that backs the validated legacy signer.
        product_account: ProductAccountId,
        /// Original legacy-account request.
        request: HostSignRawWithLegacyAccountRequest,
    },
}

/// Transaction-creation request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CreateTransactionAuthorityRequest {
    /// Create a transaction with a product-derived account.
    Product(ProductAccountTxPayload),
    /// Create a transaction through the legacy-account API using the product slot-zero account.
    LegacyAccount {
        /// Product slot-zero account that backs the validated legacy signer.
        product_account: ProductAccountId,
        /// Original legacy-account transaction request.
        request: LegacyAccountTxPayload,
    },
}

/// Statement-store allowance signing material held by the authority layer.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StatementStoreAllowanceKey {
    pub(crate) secret: [u8; 64],
    pub(crate) public_key: [u8; 32],
}

impl StatementStoreAllowanceKey {
    pub(crate) fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
        let secret: [u8; 64] =
            secret
                .try_into()
                .map_err(|secret: Vec<u8>| AuthorityError::Unavailable {
                    reason: format!(
                        "statement-store allowance key must be 64 bytes, got {}",
                        secret.len()
                    ),
                })?;
        let public_key = statement_public_key_from_secret(secret)
            .map_err(|reason| AuthorityError::Unavailable { reason })?;
        Ok(Self { secret, public_key })
    }
}

/// Host-level account authority used by product runtimes.
///
/// Pairing hosts implement this by forwarding authority requests to a paired
/// signing host. A signing-host implementation can later provide the same
/// surface from local keys without changing product runtime code.
#[async_trait]
pub(crate) trait ProductAuthority: Send + Sync {
    /// Current account-authority session, if connected.
    fn current_session(&self) -> Option<AuthoritySession>;

    /// Shared session holder owned by this authority.
    ///
    /// Product runtimes use it for connection-status subscriptions. The
    /// concrete authority keeps ownership of the actual session material.
    fn session_state(&self) -> Arc<SessionState>;

    /// Request account connection for the calling product.
    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>>;

    /// Disconnect the current account-authority session.
    async fn disconnect(&self);

    /// Sign a SCALE transaction payload for a product account.
    async fn sign_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: SignPayloadAuthorityRequest,
    ) -> Result<HostSignPayloadResponse, AuthorityError>;

    /// Sign arbitrary bytes for a product account.
    async fn sign_raw(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: SignRawAuthorityRequest,
    ) -> Result<HostSignPayloadResponse, AuthorityError>;

    /// Build and sign a transaction for a product account.
    async fn create_transaction(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<HostCreateTransactionResponse, AuthorityError>;

    /// Request an alias proof for a product account in another product context.
    async fn account_alias(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_account_id: ProductAccountId,
        requesting_product_id: String,
    ) -> Result<HostAccountGetAliasResponse, AuthorityError>;

    /// Ask the account authority to allocate product-scoped resources.
    async fn allocate_resources(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, AuthorityError>;

    /// Return statement-store allowance key material for the calling product.
    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError>;

    /// Return Bulletin allowance key material for the calling product.
    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError>;

    /// Evict any cached Bulletin allowance key for the product and allocate a
    /// fresh one, increasing the existing allowance.
    ///
    /// Called after a submission is rejected for an exhausted/missing
    /// allowance, where reusing the cached key would loop forever.
    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError>;

    /// Sign exact statement-store proof bytes with a product-derived account.
    async fn sign_statement_store_product_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        account: ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError>;

    /// Derive product-scoped entropy for a connected session.
    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError>;
}

/// Build the neutral authority-session snapshot for `session`.
pub(super) fn authority_session(session: &SessionInfo) -> AuthoritySession {
    AuthoritySession::from_session_info(session, authority_session_validation_id(session))
}

/// Revalidate a pre-confirmation snapshot against the live session, returning
/// the current [`SessionInfo`] when it still matches.
///
/// Both roles use this before touching key material: a snapshot taken before
/// user confirmation must still be the current authority session when the
/// signature or derivation happens, otherwise the request is rejected.
pub(super) fn require_current_session(
    session_state: &SessionState,
    session: &AuthoritySession,
) -> Result<SessionInfo, AuthorityError> {
    let current = session_state
        .current()
        .ok_or(AuthorityError::Disconnected)?;
    if authority_session_validation_id(&current) == session.validation_id {
        Ok(current)
    } else {
        Err(AuthorityError::Disconnected)
    }
}

/// Opaque token identifying which concrete session a snapshot was taken from.
pub(super) fn authority_session_validation_id(session: &SessionInfo) -> Vec<u8> {
    let mut id = Vec::with_capacity(67);
    if let Some(sso) = &session.sso {
        id.extend_from_slice(b"sso");
        id.extend_from_slice(&sso.session_id_own);
        id.extend_from_slice(&sso.session_id_peer);
    } else {
        id.extend_from_slice(b"local");
        id.extend_from_slice(&session.public_key);
    }
    id
}
