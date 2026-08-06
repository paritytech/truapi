//! Versioned wrappers for [`Account`](crate::api::Account) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostAccountGetRequest { V1 => v01::HostAccountGetRequest }
    pub enum HostAccountGetResponse { V1 => v01::HostAccountGetResponse }
    pub enum HostAccountGetError { V1 => v01::HostAccountGetError }
    pub enum HostAccountGetAliasRequest { V1 => v01::HostAccountGetAliasRequest }
    pub enum HostAccountGetAliasResponse { V1 => v01::ContextualAlias }
    pub enum HostAccountGetAliasError { V1 => v01::HostAccountGetAliasError }
    pub enum HostAccountCreateProofRequest { V1 => v01::HostAccountCreateProofRequest }
    pub enum HostAccountCreateProofResponse { V1 => v01::HostAccountCreateProofResponse }
    pub enum HostAccountCreateProofError { V1 => v01::HostAccountCreateProofError }
    pub enum HostAccountRegisterRingVrfKeyRequest { V1 => v01::HostAccountRegisterRingVrfKeyRequest }
    pub enum HostAccountRegisterRingVrfKeyResponse { V1 => v01::RingVrfPublicKey }
    pub enum HostAccountRegisterRingVrfKeyError { V1 => v01::HostAccountRegisterRingVrfKeyError }
    pub enum HostAccountListRingVrfKeysRequest { V1 => v01::HostAccountListRingVrfKeysRequest }
    pub enum HostAccountListRingVrfKeysResponse { V1 => Vec<v01::RegisteredRingVrfKey> }
    pub enum HostAccountListRingVrfKeysError { V1 => v01::HostAccountListRingVrfKeysError }
    pub enum HostAccountRingVrfSignRequest { V1 => v01::HostAccountRingVrfSignRequest }
    pub enum HostAccountRingVrfSignResponse { V1 => Vec<u8> }
    pub enum HostAccountRingVrfSignError { V1 => v01::HostAccountRingVrfSignError }
    pub enum HostAccountSignVrfRequest { V1 => v01::HostAccountSignVrfRequest }
    pub enum HostAccountSignVrfResponse { V1 => v01::VrfSignature }
    pub enum HostAccountSignVrfError { V1 => v01::HostAccountSignVrfError }
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
