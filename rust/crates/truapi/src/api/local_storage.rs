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
    /// if (result.isErr()) throw result.error;
    /// console.log(result.value.value);
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
    /// if (result.isErr()) throw result.error;
    /// console.log("ok");
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
    /// if (result.isErr()) throw result.error;
    /// console.log("ok");
    /// ```
    #[wire(request_id = 16)]
    async fn clear(
        &self,
        cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>>;
}
