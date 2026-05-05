//! Unified [`LocalStorage`] trait.

use crate::v02::StorageError;
use crate::versioned::local_storage::{
    HostLocalStorageClearRequest, HostLocalStorageClearResponse, HostLocalStorageReadRequest,
    HostLocalStorageReadResponse, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
};
use crate::wire;
use crate::CallContext;

/// Local key/value storage scoped to the calling product. Unified counterpart
/// of [`crate::v02::LocalStorage`].
#[async_trait::async_trait]
pub trait LocalStorage: Send + Sync {
    /// Read a value by key.
    #[wire(id = 12)]
    async fn host_local_storage_read(
        &self,
        cx: &CallContext,
        request: HostLocalStorageReadRequest,
    ) -> Result<HostLocalStorageReadResponse, StorageError>;

    /// Write a value to a key.
    #[wire(id = 14)]
    async fn host_local_storage_write(
        &self,
        cx: &CallContext,
        request: HostLocalStorageWriteRequest,
    ) -> Result<HostLocalStorageWriteResponse, StorageError>;

    /// Clear a value by key.
    #[wire(id = 16)]
    async fn host_local_storage_clear(
        &self,
        cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, StorageError>;
}
