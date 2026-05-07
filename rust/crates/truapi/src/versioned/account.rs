//! Versioned wrappers for [`AccountManagement`](crate::api::AccountManagement) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper for [`v01::HostAccountGetRequest`] and older versions.
    pub enum HostAccountGetRequest { V1 => v01::HostAccountGetRequest }
    /// Versioned wrapper for [`v01::HostAccountGetResponse`] and older versions.
    pub enum HostAccountGetResponse { V1 => v01::HostAccountGetResponse }
    /// Versioned wrapper for [`v01::HostAccountGetError`] and older versions.
    pub enum HostAccountGetError { V1 => v01::HostAccountGetError }
    /// Versioned wrapper for [`v01::HostAccountGetAliasRequest`] and older versions.
    pub enum HostAccountGetAliasRequest { V1 => v01::HostAccountGetAliasRequest }
    /// Versioned wrapper for [`v01::HostAccountGetAliasResponse`] and older versions.
    pub enum HostAccountGetAliasResponse { V1 => v01::HostAccountGetAliasResponse }
    /// Versioned wrapper for [`v01::HostAccountGetAliasError`] and older versions.
    pub enum HostAccountGetAliasError { V1 => v01::HostAccountGetAliasError }
    /// Versioned wrapper for [`v01::HostAccountCreateProofRequest`] and older versions.
    pub enum HostAccountCreateProofRequest { V1 => v01::HostAccountCreateProofRequest }
    /// Versioned wrapper for [`v01::HostAccountCreateProofResponse`] and older versions.
    pub enum HostAccountCreateProofResponse { V1 => v01::HostAccountCreateProofResponse }
    /// Versioned wrapper for [`v01::HostAccountCreateProofError`] and older versions.
    pub enum HostAccountCreateProofError { V1 => v01::HostAccountCreateProofError }
    /// Versioned wrapper for unit and older versions.
    pub enum HostGetNonProductAccountsRequest { V1 }
    /// Versioned wrapper for [`v01::HostGetNonProductAccountsResponse`] and older versions.
    pub enum HostGetNonProductAccountsResponse { V1 => v01::HostGetNonProductAccountsResponse }
    /// Versioned wrapper for [`v01::HostGetNonProductAccountsError`] and older versions.
    pub enum HostGetNonProductAccountsError { V1 => v01::HostGetNonProductAccountsError }
    /// Versioned wrapper for [`v01::HostAccountConnectionStatusSubscribeItem`] and older versions.
    pub enum HostAccountConnectionStatusSubscribeItem { V1 => v01::HostAccountConnectionStatusSubscribeItem }
    /// Versioned wrapper for unit and older versions.
    pub enum HostGetUserIdRequest { V2 }
    /// Versioned wrapper for [`v02::HostGetUserIdResponse`] and older versions.
    pub enum HostGetUserIdResponse { V2 => v02::HostGetUserIdResponse }
    /// Versioned wrapper for [`v02::HostGetUserIdError`] and older versions.
    pub enum HostGetUserIdError { V2 => v02::HostGetUserIdError }
}
