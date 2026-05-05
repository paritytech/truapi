//! Versioned wrappers for [`LocalStorage`](super::super::v02::LocalStorage) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{StorageKey, StorageValue};

/// Request wrapper for `host_local_storage_read`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostLocalStorageReadRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(StorageKey),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(StorageKey),
}

impl Versioned for HostLocalStorageReadRequest {
    type Inner = StorageKey;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_local_storage_read`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostLocalStorageReadResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Option<StorageValue>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Option<StorageValue>),
}

impl Versioned for HostLocalStorageReadResponse {
    type Inner = Option<StorageValue>;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_local_storage_write`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostLocalStorageWriteRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1 {
        key: StorageKey,
        value: StorageValue,
    },
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2 {
        key: StorageKey,
        value: StorageValue,
    },
}

impl Versioned for HostLocalStorageWriteRequest {
    type Inner = (StorageKey, StorageValue);
    fn wrap(version: u8, (key, value): Self::Inner) -> Self {
        match version {
            1 => Self::V1 { key, value },
            _ => Self::V2 { key, value },
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1 { key, value } | Self::V2 { key, value } => (key, value),
        }
    }
}

/// Response wrapper for `host_local_storage_write`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostLocalStorageWriteResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostLocalStorageWriteResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `host_local_storage_clear`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostLocalStorageClearRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(StorageKey),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(StorageKey),
}

impl Versioned for HostLocalStorageClearRequest {
    type Inner = StorageKey;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_local_storage_clear`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostLocalStorageClearResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostLocalStorageClearResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}
