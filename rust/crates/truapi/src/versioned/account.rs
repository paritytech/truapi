//! Versioned wrappers for [`Account`](crate::api::Account) methods.

use crate::v01;
use truapi_macros::versioned_type;

versioned_type! {
    pub enum HostAccountGetRequest { V1 => v01::HostAccountGetRequest }
    pub enum HostAccountGetResponse { V1 => v01::HostAccountGetResponse }
    pub enum HostAccountGetError { V1 => v01::HostAccountGetError }
    pub enum HostAccountGetAliasRequest { V1 => v01::HostAccountGetAliasRequest }
    pub enum HostAccountGetAliasResponse { V1 => v01::HostAccountGetAliasResponse }
    pub enum HostAccountGetAliasError { V1 => v01::HostAccountGetError }
    pub enum HostAccountCreateProofRequest { V1 => v01::HostAccountCreateProofRequest }
    pub enum HostAccountCreateProofResponse { V1 => v01::HostAccountCreateProofResponse }
    pub enum HostAccountCreateProofError { V1 => v01::HostAccountCreateProofError }
    pub enum HostGetLegacyAccountsRequest { V1 }
    pub enum HostGetLegacyAccountsResponse { V1 => v01::HostGetLegacyAccountsResponse }
    pub enum HostGetLegacyAccountsError { V1 => v01::HostAccountGetError }
    pub enum HostAccountConnectionStatusSubscribeItem { V1 => v01::HostAccountConnectionStatusSubscribeItem }
    pub enum HostRequestLoginRequest { V1 => v01::HostRequestLoginRequest }
    pub enum HostRequestLoginResponse { V1 => v01::HostRequestLoginResponse }
    pub enum HostRequestLoginError { V1 => v01::HostRequestLoginError }
    pub enum HostGetUserIdRequest { V1 }
    pub enum HostGetUserIdResponse { V1 => v01::HostGetUserIdResponse }
    pub enum HostGetUserIdError { V1 => v01::HostGetUserIdError }
}
