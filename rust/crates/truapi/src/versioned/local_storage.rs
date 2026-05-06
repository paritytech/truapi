//! Versioned wrappers for [`LocalStorage`](crate::api::LocalStorage) methods.

use crate::v01;

versioned_type! {
    /// Request wrapper for `host_local_storage_read`.
    pub enum HostLocalStorageReadRequest { V1 => v01::StorageKey }
    /// Response wrapper for `host_local_storage_read`.
    pub enum HostLocalStorageReadResponse { V1 => Option<v01::StorageValue> }
    /// Error wrapper for `host_local_storage_read`.
    pub enum HostLocalStorageReadError { V1 => v01::StorageError }
    /// Request wrapper for `host_local_storage_write`.
    pub enum HostLocalStorageWriteRequest { V1 => v01::LocalStorageWriteRequest }
    /// Response wrapper for `host_local_storage_write`.
    pub enum HostLocalStorageWriteResponse { V1 }
    /// Error wrapper for `host_local_storage_write`.
    pub enum HostLocalStorageWriteError { V1 => v01::StorageError }
    /// Request wrapper for `host_local_storage_clear`.
    pub enum HostLocalStorageClearRequest { V1 => v01::StorageKey }
    /// Response wrapper for `host_local_storage_clear`.
    pub enum HostLocalStorageClearResponse { V1 }
    /// Error wrapper for `host_local_storage_clear`.
    pub enum HostLocalStorageClearError { V1 => v01::StorageError }
}
