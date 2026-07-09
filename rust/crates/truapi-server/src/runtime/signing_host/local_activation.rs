use super::{SigningHost, product_authority_error};
use crate::host_logic::product_account::derive_root_keypair_from_entropy;
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
}

#[async_trait::async_trait]
impl LocalActivation for SigningHost {
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError> {
        let secret = Zeroizing::new(secret);
        let root = derive_root_keypair_from_entropy(&secret).map_err(product_authority_error)?;
        let public_key = root.public.to_bytes();
        *self
            .root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned") = Some(secret);
        let session = SessionInfo {
            public_key,
            sso: None,
            root_entropy_source: None,
            identity_account_id: None,
            lite_username: None,
            full_username: None,
        };
        self.session_state.set_session(session.clone());
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        Ok(())
    }
}
