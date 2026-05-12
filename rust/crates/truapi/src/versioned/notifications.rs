//! Versioned wrappers for the notification cancellation method introduced by
//! RFC 0019. The method does not exist in v0.1, so both request and response
//! envelopes carry only a `V2` arm.

use crate::{v01, v02};

versioned_type! {
    pub enum HostPushNotificationCancelRequest {
        V2 => v02::HostPushNotificationCancelRequest,
    }
    pub enum HostPushNotificationCancelResponse {
        V2,
    }
    pub enum HostPushNotificationCancelError {
        V2 => v01::GenericError,
    }
}
