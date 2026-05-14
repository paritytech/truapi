//! Versioned wrappers for [`LocalStorage`](crate::api::LocalStorage) methods.

use crate::v01;

versioned_type! {
    pub enum HostLocalStorageReadRequest { V1 => v01::HostLocalStorageReadRequest }
    pub enum HostLocalStorageReadResponse { V1 => v01::HostLocalStorageReadResponse }
    pub enum HostLocalStorageReadError { V1 => v01::HostLocalStorageReadError }
    pub enum HostLocalStorageWriteRequest { V1 => v01::HostLocalStorageWriteRequest }
    pub enum HostLocalStorageWriteResponse { V1 }
    pub enum HostLocalStorageWriteError { V1 => v01::HostLocalStorageReadError }
    pub enum HostLocalStorageClearRequest { V1 => v01::HostLocalStorageClearRequest }
    pub enum HostLocalStorageClearResponse { V1 }
    pub enum HostLocalStorageClearError { V1 => v01::HostLocalStorageReadError }
}
