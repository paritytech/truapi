//! Versioned wrappers for [`Contacts`](crate::api::Contacts) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostContactsResolveRequest { V1 => v01::HostContactsResolveRequest }
    pub enum HostContactsResolveResponse { V1 => v01::HostContactsResolveResponse }
    pub enum HostContactsResolveError { V1 => v01::HostContactsResolveError }
    pub enum HostContactsDeriveSharedKeyRequest { V1 => v01::HostContactsDeriveSharedKeyRequest }
    pub enum HostContactsDeriveSharedKeyResponse { V1 => v01::HostContactsDeriveSharedKeyResponse }
    pub enum HostContactsDeriveSharedKeyError { V1 => v01::HostContactsDeriveSharedKeyError }
    pub enum HostContactsSendRequest { V1 => v01::HostContactsSendRequest }
    pub enum HostContactsSendError { V1 => v01::HostContactsSendError }
    pub enum HostContactsSubscribeItem { V1 => v01::HostContactsSubscribeItem }
    pub enum HostContactsSubscribeError { V1 => v01::HostContactsSubscribeError }
}
