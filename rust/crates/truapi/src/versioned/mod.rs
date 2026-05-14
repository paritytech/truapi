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
}

/// Latest known protocol version.
pub mod latest {
    use super::Version;

    /// The latest protocol version.
    pub const VERSION: Version = Version::V1;
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
}

pub mod account;
pub mod calls;
pub mod chain;
pub mod chat;
pub mod coin_payment;
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
}
