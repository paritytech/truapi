//! Versioned wrappers for [`Notifications`](crate::api::Notifications) methods.

use crate::v01;

versioned_type! {
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    pub enum HostPushNotificationResponse { V1 => v01::HostPushNotificationResponse }
    pub enum HostPushNotificationError { V1 => v01::HostPushNotificationError }
    pub enum HostPushNotificationCancelRequest { V1 => v01::HostPushNotificationCancelRequest }
    pub enum HostPushNotificationCancelResponse { V1 }
    pub enum HostPushNotificationCancelError { V1 => v01::GenericError }
    pub enum HostPushAddRulesRequest { V1 => v01::HostPushAddRulesRequest }
    pub enum HostPushAddRulesResponse { V1 }
    pub enum HostPushAddRulesError { V1 => v01::HostPushAddRulesError }
    pub enum HostPushRemoveRulesRequest { V1 => v01::HostPushRemoveRulesRequest }
    pub enum HostPushRemoveRulesResponse { V1 }
    pub enum HostPushRemoveRulesError { V1 => v01::HostPushRemoveRulesError }
    pub enum HostPushListRulesRequest { V1 => v01::HostPushListRulesRequest }
    pub enum HostPushListRulesResponse { V1 => v01::HostPushListRulesResponse }
    pub enum HostPushListRulesError { V1 => v01::HostPushListRulesError }
    pub enum HostPushSetRulesRequest { V1 => v01::HostPushSetRulesRequest }
    pub enum HostPushSetRulesResponse { V1 }
    pub enum HostPushSetRulesError { V1 => v01::HostPushSetRulesError }
}
