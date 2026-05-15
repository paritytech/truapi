//! Versioned request and response wrappers for the unified TrUAPI contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    /// Initial protocol version.
    V1,
}

pub mod latest {
    use super::Version;

    pub const VERSION: Version = Version::V1;
}

#[allow(clippy::result_unit_err)]
pub trait IntoVersion: Sized {
    fn into_version(self, version: Version) -> Result<Self, ()>;

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
pub mod chain;
pub mod chat;
pub mod entropy;
pub mod jsonrpc;
pub mod local_storage;
pub mod navigation;
pub mod payment;
pub mod permissions;
pub mod preimage;
pub mod resource_allocation;
pub mod signing;
pub mod statement_store;
pub mod system;
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
        let original = super::navigation::HostNavigateToResponse::V1;
        let decoded =
            super::navigation::HostNavigateToResponse::decode(&mut &original.encode()[..])
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
