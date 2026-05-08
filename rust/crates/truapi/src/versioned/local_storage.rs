//! Versioned wrappers for [`LocalStorage`](crate::api::LocalStorage) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostLocalStorageReadRequest`] and older versions.
    pub enum HostLocalStorageReadRequest { V1 => v01::HostLocalStorageReadRequest }
    /// Versioned wrapper for [`v01::HostLocalStorageReadResponse`] and older versions.
    pub enum HostLocalStorageReadResponse { V1 => v01::HostLocalStorageReadResponse }
    /// Versioned wrapper for [`v01::HostLocalStorageReadError`] and older versions.
    pub enum HostLocalStorageReadError { V1 => v01::HostLocalStorageReadError }
    /// Versioned wrapper for [`v01::HostLocalStorageWriteRequest`] and older versions.
    pub enum HostLocalStorageWriteRequest { V1 => v01::HostLocalStorageWriteRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum HostLocalStorageWriteResponse { V1 }
    /// Versioned wrapper for [`v01::HostLocalStorageReadError`] and older versions.
    pub enum HostLocalStorageWriteError { V1 => v01::HostLocalStorageReadError }
    /// Versioned wrapper for [`v01::HostLocalStorageClearRequest`] and older versions.
    pub enum HostLocalStorageClearRequest { V1 => v01::HostLocalStorageClearRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum HostLocalStorageClearResponse { V1 }
    /// Versioned wrapper for [`v01::HostLocalStorageReadError`] and older versions.
    pub enum HostLocalStorageClearError { V1 => v01::HostLocalStorageReadError }
}
