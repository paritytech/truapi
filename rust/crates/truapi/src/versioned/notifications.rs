//! Versioned wrappers for [`Notifications`](crate::api::Notifications) methods.

use crate::v01;

versioned_type! {
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    pub enum HostPushNotificationResponse { V1 }
    pub enum HostPushNotificationError { V1 => v01::GenericError }
    pub enum HostPushSubscribeRequest { V1 => v01::HostPushSubscribeRequest }
    pub enum HostPushSubscribeResponse { V1 }
    pub enum HostPushSubscribeError { V1 => v01::HostPushSubscribeError }
    pub enum HostPushUnsubscribeRequest { V1 => v01::HostPushUnsubscribeRequest }
    pub enum HostPushUnsubscribeResponse { V1 }
    pub enum HostPushUnsubscribeError { V1 => v01::HostPushUnsubscribeError }
}
