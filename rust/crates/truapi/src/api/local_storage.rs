//! Unified [`LocalStorage`] trait.

use crate::versioned::local_storage::{
    HostLocalStorageClearError, HostLocalStorageClearRequest, HostLocalStorageClearResponse,
    HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
    HostLocalStorageWriteError, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Local key/value storage scoped to the calling product.
pub trait LocalStorage: Send + Sync {
    /// Read a value by key.
    ///
    /// ```ts
    /// const result = await truapi.localStorage.read({ key: "test-key" });
    /// assert(result.isOk(), "read failed:", result);
    /// console.log("storage value read:", result.value.value);
    /// ```
    #[wire(request_id = 12)]
    async fn read(
        &self,
        cx: &CallContext,
        request: HostLocalStorageReadRequest,
    ) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>;

    /// Write a value to a key.
    ///
    /// ```ts
    /// const result = await truapi.localStorage.write({
    ///   key: "test-key",
    ///   value: "0x48656c6c6f",
    /// });
    /// assert(result.isOk(), "write failed:", result);
    /// console.log("storage write succeeded");
    /// ```
    #[wire(request_id = 14)]
    async fn write(
        &self,
        cx: &CallContext,
        request: HostLocalStorageWriteRequest,
    ) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>>;

    /// Clear a value by key.
    ///
    /// ```ts
    /// const result = await truapi.localStorage.clear({ key: "test-key" });
    /// assert(result.isOk(), "clear failed:", result);
    /// console.log("storage clear succeeded");
    /// ```
    #[wire(request_id = 16)]
    async fn clear(
        &self,
        cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>>;
}
