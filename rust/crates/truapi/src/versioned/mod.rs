//! Versioned request and response wrappers for the unified TrUAPI contract.
//!
//! Every wire-level request and response is expressed as a versioned enum
//! whose `V<N>(..)` arms wrap the per-version shape from the corresponding
//! version module. The codec discriminant is pinned with `#[codec(index = N)]`
//! so adding a future version slot doesn't shift existing versions on the wire.

/// Protocol version identifier. Each variant matches a `V<N>(..)` arm of the
/// versioned wrapper enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    /// Initial protocol version.
    V1,
    /// Second protocol version. Introduced by RFC 0019 (scheduled push
    /// notifications); only methods that have a v0.2 shape carry a `V2(..)`
    /// arm — others still resolve to their `V1` payload when promoted.
    V2,
}

/// Latest known protocol version.
pub mod latest {
    use super::Version;

    /// The latest protocol version.
    pub const VERSION: Version = Version::V2;
}

/// Convert a versioned wrapper into a different version of itself.
#[allow(clippy::result_unit_err)]
pub trait IntoVersion: Sized {
    /// Consume `self` and return same value expressed in some particular `version`.
    fn into_version(self, version: Version) -> Result<Self, ()>;

    /// Consume `self` and return same value expressed the latest version.
    fn into_latest(self) -> Result<Self, ()> {
        self.into_version(latest::VERSION)
    }
}

macro_rules! versioned_type {
    (
        $(
            pub enum $name:ident {
                $($body:tt)*
            }
        )*
    ) => {
        $(
            versioned_type! {
                @one
                pub enum $name {
                    $($body)*
                }
            }
        )*
    };

    (
        @one
        pub enum $name:ident {
            V1 => $v1:ty $(,)?
        }
    ) => {
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            #[codec(index = 0)]
            V1($v1),
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, _version: $crate::versioned::Version) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };

    (
        @one
        pub enum $name:ident {
            V1 $(,)?
        }
    ) => {
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            #[codec(index = 0)]
            V1,
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, _version: $crate::versioned::Version) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };

    (
        @one
        pub enum $name:ident {
            V1 => $v1:ty,
            V2 => $v2:ty $(,)?
        }
    ) => {
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            #[codec(index = 0)]
            V1($v1),
            #[codec(index = 1)]
            V2($v2),
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, _version: $crate::versioned::Version) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };

    (
        @one
        pub enum $name:ident {
            V1,
            V2 => $v2:ty $(,)?
        }
    ) => {
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            #[codec(index = 0)]
            V1,
            #[codec(index = 1)]
            V2($v2),
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, _version: $crate::versioned::Version) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };

    (
        @one
        pub enum $name:ident {
            V2 => $v2:ty $(,)?
        }
    ) => {
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            #[codec(index = 1)]
            V2($v2),
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, _version: $crate::versioned::Version) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };

    (
        @one
        pub enum $name:ident {
            V2 $(,)?
        }
    ) => {
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            #[codec(index = 1)]
            V2,
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, _version: $crate::versioned::Version) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };
}

pub mod account;
pub mod calls;
pub mod chain;
pub mod chat;
pub mod entropy;
pub mod jsonrpc;
pub mod local_storage;
pub mod payment;
pub mod permissions;
pub mod preimage;
pub mod resource_allocation;
pub mod signing;
pub mod statement_store;
pub mod theme;

/// Notification cancellation, introduced by RFC 0019. The cancel method does
/// not exist in v0.1, so its envelopes carry only a `V2` arm.
pub mod notifications;

#[cfg(test)]
mod tests {
    use parity_scale_codec::{Decode, Encode};

    #[test]
    fn v1_discriminant_is_zero() {
        let v1 = super::permissions::HostDevicePermissionRequest::V1(
            crate::v01::HostDevicePermissionRequest::Camera,
        );
        assert_eq!(v1.encode()[0], 0, "V1 must encode discriminant 0");
    }

    #[test]
    fn unit_response_roundtrip() {
        let original = super::calls::HostNavigateToResponse::V1;
        let decoded = super::calls::HostNavigateToResponse::decode(&mut &original.encode()[..])
            .expect("decode");
        assert_eq!(original, decoded);
    }

    #[test]
    fn struct_variant_roundtrip() {
        let original = super::local_storage::HostLocalStorageWriteRequest::V1(
            crate::v01::HostLocalStorageWriteRequest {
                key: "greeting".into(),
                value: b"hello".to_vec(),
            },
        );
        let decoded =
            super::local_storage::HostLocalStorageWriteRequest::decode(&mut &original.encode()[..])
                .expect("decode");
        assert_eq!(original, decoded);
    }

    #[test]
    fn v2_discriminant_is_one() {
        let v2 = super::calls::HostPushNotificationRequest::V2(
            crate::v02::HostPushNotificationRequest {
                text: "hi".into(),
                deeplink: None,
                scheduled_at: Some(1_715_000_000_000),
            },
        );
        assert_eq!(v2.encode()[0], 1, "V2 must encode discriminant 1");
    }

    #[test]
    fn v1_and_v2_coexist_in_push_notification_request() {
        let v1 = super::calls::HostPushNotificationRequest::V1(
            crate::v01::HostPushNotificationRequest {
                text: "hi".into(),
                deeplink: None,
            },
        );
        let v2 = super::calls::HostPushNotificationRequest::V2(
            crate::v02::HostPushNotificationRequest {
                text: "hi".into(),
                deeplink: Some("/route".into()),
                scheduled_at: Some(42),
            },
        );
        let v1_decoded =
            super::calls::HostPushNotificationRequest::decode(&mut &v1.encode()[..]).expect("v1");
        let v2_decoded =
            super::calls::HostPushNotificationRequest::decode(&mut &v2.encode()[..]).expect("v2");
        assert_eq!(v1, v1_decoded);
        assert_eq!(v2, v2_decoded);
    }

    #[test]
    fn push_notification_response_carries_id_in_v2() {
        let response = super::calls::HostPushNotificationResponse::V2(
            crate::v02::HostPushNotificationResponse { id: 7 },
        );
        let decoded =
            super::calls::HostPushNotificationResponse::decode(&mut &response.encode()[..])
                .expect("decode");
        assert_eq!(response, decoded);
    }

    #[test]
    fn cancel_envelopes_only_have_v2_arm() {
        // The cancel method did not exist in v0.1, so its discriminant skips
        // index 0. This pins that decision on the wire.
        let request = super::notifications::HostPushNotificationCancelRequest::V2(
            crate::v02::HostPushNotificationCancelRequest { id: 3 },
        );
        assert_eq!(request.encode()[0], 1, "cancel request V2 discriminant");

        let response = super::notifications::HostPushNotificationCancelResponse::V2;
        assert_eq!(response.encode()[0], 1, "cancel response V2 discriminant");
    }

    #[test]
    fn schedule_limit_reached_error_roundtrip() {
        let error = super::calls::HostPushNotificationError::V2(
            crate::v02::HostPushNotificationError::ScheduleLimitReached,
        );
        let decoded = super::calls::HostPushNotificationError::decode(&mut &error.encode()[..])
            .expect("decode");
        assert_eq!(error, decoded);
    }
}
