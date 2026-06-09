//! Versioned wrappers for [`Contacts`](crate::api::Contacts) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostContactsGetRequest { V1 => v01::HostContactsGetRequest }
    pub enum HostContactsGetResponse { V1 => v01::HostContactsGetResponse }
    pub enum HostContactsGetError { V1 => v01::HostContactsError }
    pub enum HostContactsSubscribeRequest { V1 => v01::HostContactsGetRequest }
    pub enum HostContactsSubscribeItem { V1 => v01::HostContactsSubscribeItem }
    pub enum HostContactsSubscribeError { V1 => v01::HostContactsError }
}
