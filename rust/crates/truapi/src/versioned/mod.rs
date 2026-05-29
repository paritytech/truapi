//! Versioned request and response wrappers for the unified TrUAPI contract.

#[allow(clippy::result_unit_err)]
pub trait IntoVersion: Sized {
    /// The latest version this envelope supports.
    const LATEST: u8;

    /// Convert this envelope to the requested version; `N` selects the `Vn` variant.
    fn into_version(self, version: u8) -> Result<Self, ()>;

    /// Convert this envelope to its latest supported version.
    fn into_latest(self) -> Result<Self, ()> {
        self.into_version(Self::LATEST)
    }
}

macro_rules! versioned_type {
    (
        $(
            $(#[$enum_meta:meta])*
            pub enum $name:ident {
                $($body:tt)*
            }
        )*
    ) => {
        $(
            versioned_type! {
                @one
                $(#[$enum_meta])*
                pub enum $name {
                    $($body)*
                }
            }
        )*
    };

    (
        @one
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(#[$variant_meta:meta])*
            V1 => $v1:ty $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$variant_meta])*
            #[codec(index = 0)]
            V1($v1),
        }

        impl $crate::versioned::IntoVersion for $name {
            const LATEST: u8 = 1;

            fn into_version(self, _version: u8) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };

    (
        @one
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(#[$variant_meta:meta])*
            V1 $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[doc = concat!("Versioned envelope for [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        pub enum $name {
            $(#[$variant_meta])*
            #[codec(index = 0)]
            V1,
        }

        impl $crate::versioned::IntoVersion for $name {
            const LATEST: u8 = 1;

            fn into_version(self, _version: u8) -> Result<Self, ()> {
                Ok(self)
            }
        }
    };
}

pub mod account;
pub mod chain;
pub mod chat;
pub mod coin_payment;
pub mod entropy;
pub mod local_storage;
pub mod notifications;
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
        let original = super::system::HostNavigateToResponse::V1;
        let decoded = super::system::HostNavigateToResponse::decode(&mut &original.encode()[..])
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
