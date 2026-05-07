//! Unified [`LocalStorage`] trait.

use crate::versioned::local_storage::{
    HostLocalStorageClearError, HostLocalStorageClearRequest, HostLocalStorageClearResponse,
    HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
    HostLocalStorageWriteError, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Local key/value storage scoped to the calling product.
#[async_trait::async_trait]
pub trait LocalStorage: Send + Sync {
    /// Read a value by key.
    ///
    /// ```truapi-playground-request
    /// { "key": "test-key" }
    /// ```
    #[wire(id = 12)]
    async fn host_local_storage_read(
        &self,
        cx: &CallContext,
        request: HostLocalStorageReadRequest,
    ) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>;

    /// Write a value to a key.
    ///
    /// ```truapi-playground-request
    /// { "key": "test-key", "value": "0x48656c6c6f" }
    /// ```
    #[wire(id = 14)]
    async fn host_local_storage_write(
        &self,
        cx: &CallContext,
        request: HostLocalStorageWriteRequest,
    ) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>>;

    /// Clear a value by key.
    ///
    /// ```truapi-playground-request
    /// { "key": "test-key" }
    /// ```
    #[wire(id = 16)]
    async fn host_local_storage_clear(
        &self,
        cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>>;
}
