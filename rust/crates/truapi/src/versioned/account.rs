//! Versioned wrappers for [`AccountManagement`](crate::api::AccountManagement) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostAccountGetRequest`].
    pub enum HostAccountGetRequest { V1 => v01::HostAccountGetRequest }
    /// Versioned wrapper for [`v01::HostAccountGetResponse`].
    pub enum HostAccountGetResponse { V1 => v01::HostAccountGetResponse }
    /// Versioned wrapper for [`v01::HostAccountGetError`].
    pub enum HostAccountGetError { V1 => v01::HostAccountGetError }
    /// Versioned wrapper for [`v01::HostAccountGetAliasRequest`].
    pub enum HostAccountGetAliasRequest { V1 => v01::HostAccountGetAliasRequest }
    /// Versioned wrapper for [`v01::HostAccountGetAliasResponse`].
    pub enum HostAccountGetAliasResponse { V1 => v01::HostAccountGetAliasResponse }
    /// Versioned wrapper around the alias-lookup error path; reuses [`v01::HostAccountGetError`].
    pub enum HostAccountGetAliasError { V1 => v01::HostAccountGetError }
    /// Versioned wrapper for [`v01::HostAccountCreateProofRequest`].
    pub enum HostAccountCreateProofRequest { V1 => v01::HostAccountCreateProofRequest }
    /// Versioned wrapper for [`v01::HostAccountCreateProofResponse`].
    pub enum HostAccountCreateProofResponse { V1 => v01::HostAccountCreateProofResponse }
    /// Versioned wrapper for [`v01::HostAccountCreateProofError`].
    pub enum HostAccountCreateProofError { V1 => v01::HostAccountCreateProofError }
    /// Versioned wrapper for unit.
    pub enum HostGetLegacyAccountsRequest { V1 }
    /// Versioned wrapper for [`v01::HostGetLegacyAccountsResponse`].
    pub enum HostGetLegacyAccountsResponse { V1 => v01::HostGetLegacyAccountsResponse }
    /// Versioned wrapper around the legacy-accounts error path; reuses [`v01::HostAccountGetError`].
    pub enum HostGetLegacyAccountsError { V1 => v01::HostAccountGetError }
    /// Versioned wrapper for [`v01::HostAccountConnectionStatusSubscribeItem`].
    pub enum HostAccountConnectionStatusSubscribeItem { V1 => v01::HostAccountConnectionStatusSubscribeItem }
    /// Versioned wrapper for [`v01::HostRequestLoginRequest`].
    pub enum HostRequestLoginRequest { V1 => v01::HostRequestLoginRequest }
    /// Versioned wrapper for [`v01::HostRequestLoginResponse`].
    pub enum HostRequestLoginResponse { V1 => v01::HostRequestLoginResponse }
    /// Versioned wrapper for [`v01::HostRequestLoginError`].
    pub enum HostRequestLoginError { V1 => v01::HostRequestLoginError }
    /// Versioned wrapper for unit.
    pub enum HostGetUserIdRequest { V1 }
    /// Versioned wrapper for [`v01::HostGetUserIdResponse`].
    pub enum HostGetUserIdResponse { V1 => v01::HostGetUserIdResponse }
    /// Versioned wrapper for [`v01::HostGetUserIdError`].
    pub enum HostGetUserIdError { V1 => v01::HostGetUserIdError }
}
