//! Versioned wrappers for [`AccountManagement`](crate::api::AccountManagement) methods.

use crate::{v01, v02};

versioned_type! {
    /// Request wrapper for `host_account_get`.
    pub enum HostAccountGetRequest { V1 => v01::ProductAccountId }
    /// Response wrapper for `host_account_get`.
    pub enum HostAccountGetResponse { V1 => v01::Account }
    /// Error wrapper for `host_account_get`.
    pub enum HostAccountGetError { V1 => v01::RequestCredentialsError }
    /// Request wrapper for `host_account_get_alias`.
    pub enum HostAccountGetAliasRequest { V1 => v01::ProductAccountId }
    /// Response wrapper for `host_account_get_alias`.
    pub enum HostAccountGetAliasResponse { V1 => v01::ContextualAlias }
    /// Error wrapper for `host_account_get_alias`.
    pub enum HostAccountGetAliasError { V1 => v01::RequestCredentialsError }
    /// Request wrapper for `host_account_create_proof`.
    pub enum HostAccountCreateProofRequest { V1 => v01::AccountCreateProofRequest }
    /// Response wrapper for `host_account_create_proof`.
    pub enum HostAccountCreateProofResponse { V1 => v01::RingVrfProof }
    /// Error wrapper for `host_account_create_proof`.
    pub enum HostAccountCreateProofError { V1 => v01::CreateProofError }
    /// Request wrapper for `host_get_non_product_accounts`.
    pub enum HostGetNonProductAccountsRequest { V1 }
    /// Response wrapper for `host_get_non_product_accounts`.
    pub enum HostGetNonProductAccountsResponse { V1 => Vec<v01::Account> }
    /// Error wrapper for `host_get_non_product_accounts`.
    pub enum HostGetNonProductAccountsError { V1 => v01::RequestCredentialsError }
    /// Subscription item wrapper for `host_account_connection_status_subscribe`.
    pub enum HostAccountConnectionStatusSubscribeItem { V1 => v01::AccountConnectionStatus }
    /// Request wrapper for `host_get_user_id` (V0.2+ only - no V0.1 counterpart).
    pub enum HostGetUserIdRequest { V2 }
    /// Response wrapper for `host_get_user_id` (V0.2+ only).
    pub enum HostGetUserIdResponse { V2 => v02::UserIdentity }
    /// Error wrapper for `host_get_user_id` (V0.2+ only).
    pub enum HostGetUserIdError { V2 => v02::UserIdentityError }
}
