//! Versioned request and response wrappers for the unified TrUAPI contract.
//!
//! Every wire-level request and response is expressed as a versioned enum
//! whose `V<N>(..)` arms wrap the per-version shape from [`crate::v01`] /
//! [`crate::v02`]. The codec discriminant is pinned with `#[codec(index = N)]`
//! so adding a future `V3` slot doesn't shift existing versions on the wire.

/// Protocol version identifier. Each variant matches a `V<N>(..)` arm of the
/// versioned wrapper enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    /// Pre-RFC-0001 protocol shipped by `@novasamatech/host-api@0.6.x`.
    V1,
    /// RFC-0001 protocol.
    V2,
}

/// Latest known protocol version. Bumped whenever a new `V<N>` slot is added
/// to the wrapper enums.
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
            $(#[$meta:meta])*
            pub enum $name:ident {
                $($body:tt)*
            }
        )*
    ) => {
        $(
            versioned_type! {
                @one
                $(#[$meta])*
                pub enum $name {
                    $($body)*
                }
            }
        )*
    };

    (
        @one
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(#[$v1_meta:meta])*
            V1 => $v1:ty $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$v1_meta])*
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
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(#[$v1_meta:meta])*
            V1 $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$v1_meta])*
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
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(#[$v1_meta:meta])*
            V1 => $v1:ty,
            $(#[$v2_meta:meta])*
            V2 => $v2:ty $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$v1_meta])*
            #[codec(index = 0)]
            V1($v1),
            $(#[$v2_meta])*
            #[codec(index = 1)]
            V2($v2),
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, version: $crate::versioned::Version) -> Result<Self, ()> {
                match version {
                    $crate::versioned::Version::V1 => match self {
                        Self::V1(value) => Ok(Self::V1(value)),
                        Self::V2(value) => <$v1 as std::convert::TryFrom<$v2>>::try_from(value)
                            .map(Self::V1)
                            .map_err(|_| ()),
                    },
                    $crate::versioned::Version::V2 => match self {
                        Self::V1(value) => <$v2 as std::convert::TryFrom<$v1>>::try_from(value)
                            .map(Self::V2)
                            .map_err(|_| ()),
                        Self::V2(value) => Ok(Self::V2(value)),
                    },
                }
            }
        }
    };

    (
        @one
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(#[$v1_meta:meta])*
            V1,
            $(#[$v2_meta:meta])*
            V2 $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$v1_meta])*
            #[codec(index = 0)]
            V1,
            $(#[$v2_meta])*
            #[codec(index = 1)]
            V2,
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, version: $crate::versioned::Version) -> Result<Self, ()> {
                let _ = self;
                Ok(match version {
                    $crate::versioned::Version::V1 => Self::V1,
                    $crate::versioned::Version::V2 => Self::V2,
                })
            }
        }
    };

    (
        @one
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(#[$v2_meta:meta])*
            V2 => $v2:ty $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$v2_meta])*
            #[codec(index = 1)]
            V2($v2),
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, version: $crate::versioned::Version) -> Result<Self, ()> {
                match (self, version) {
                    (value @ Self::V2(_), $crate::versioned::Version::V2) => Ok(value),
                    (Self::V2(_), $crate::versioned::Version::V1) => Err(()),
                }
            }
        }
    };

    (
        @one
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(#[$v2_meta:meta])*
            V2 $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$v2_meta])*
            #[codec(index = 1)]
            V2,
        }

        impl $crate::versioned::IntoVersion for $name {
            fn into_version(self, version: $crate::versioned::Version) -> Result<Self, ()> {
                match version {
                    $crate::versioned::Version::V2 => Ok(Self::V2),
                    $crate::versioned::Version::V1 => Err(()),
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::{Decode, Encode};

    #[test]
    fn v1_and_v2_discriminants_match_codec_index() {
        let v1 = permissions::HostDevicePermissionRequest::V1(
            crate::v01::HostDevicePermissionRequest::Camera,
        );
        let v2 = permissions::HostDevicePermissionRequest::V2(
            crate::v02::HostDevicePermissionRequest::Camera,
        );
        assert_eq!(v1.encode()[0], 0, "V1 must encode discriminant 0");
        assert_eq!(v2.encode()[0], 1, "V2 must encode discriminant 1");
    }

    #[test]
    fn unit_response_roundtrip() {
        let original = calls::HostNavigateToResponse::V1;
        let decoded =
            calls::HostNavigateToResponse::decode(&mut &original.encode()[..]).expect("decode");
        assert_eq!(original, decoded);
    }

    #[test]
    fn struct_variant_roundtrip() {
        let original = local_storage::HostLocalStorageWriteRequest::V1(
            crate::v01::HostLocalStorageWriteRequest {
                key: "greeting".into(),
                value: b"hello".to_vec(),
            },
        );
        let decoded =
            local_storage::HostLocalStorageWriteRequest::decode(&mut &original.encode()[..])
                .expect("decode");
        assert_eq!(original, decoded);
    }
}
