use super::{SigningHost, product_authority_error};
use crate::host_logic::product_account::{
    derive_root_keypair_from_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::session::SessionInfo;
use crate::runtime::authority::AuthorityError;
use crate::runtime::connected_session_ui_info;

use zeroize::Zeroizing;

/// Establish a wallet-local session from host-held secret material.
///
/// A signing host owns the user's keys, so it establishes sessions directly
/// rather than through the SSO pairing flow. Only [`SigningHost`] implements
/// this; pairing hosts have no local secret to activate.
#[async_trait::async_trait]
pub(crate) trait LocalActivation: Send + Sync {
    /// Activate a local session from raw BIP-39 entropy, deriving the root
    /// public key and marking the session connected.
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError>;

    /// Activate a local session and attach known identity metadata from the
    /// host's signer/account store.
    async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), AuthorityError>;
}

#[async_trait::async_trait]
impl LocalActivation for SigningHost {
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError> {
        self.activate_local_session_with_identity(secret, None)
            .await
    }

    async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), AuthorityError> {
        let secret = Zeroizing::new(secret);
        let root = derive_root_keypair_from_entropy(&secret).map_err(product_authority_error)?;
        let public_key = root.public.to_bytes();
        let identity_account_id = derive_sr25519_hard_path(&secret, &["wallet", "sso"])
            .map_err(product_authority_error)?
            .public
            .to_bytes();
        *self
            .root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned") = Some(secret);
        let session = SessionInfo {
            public_key,
            sso: None,
            root_entropy_source: None,
            identity_account_id: Some(identity_account_id),
            lite_username,
            full_username: None,
        };
        self.session_state.set_session(session.clone());
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        Ok(())
    }
}
