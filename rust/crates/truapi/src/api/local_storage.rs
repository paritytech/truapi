//! Unified [`LocalStorage`] trait.

use crate::versioned::local_storage::{
    HostLocalStorageClearError, HostLocalStorageClearRequest, HostLocalStorageClearResponse,
    HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
    HostLocalStorageWriteError, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Local key/value storage scoped to the calling product.
///
#[async_trait::async_trait]
pub trait LocalStorage: Send + Sync {
    /// Read a value by key.
    ///
    /// ```truapi-client-example
    /// import { type Client, type HexString } from "@parity/truapi";
    ///
    /// export async function readLocalValue(
    ///   truapi: Client,
    /// ): Promise<HexString | undefined> {
    ///   const result = await truapi.localStorage.localStorageRead({ key: "test-key" });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.value;
    /// }
    /// ```
    #[wire(request_id = 12)]
    async fn host_local_storage_read(
        &self,
        cx: &CallContext,
        request: HostLocalStorageReadRequest,
    ) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>;

    /// Write a value to a key.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function writeLocalValue(truapi: Client): Promise<void> {
    ///   const result = await truapi.localStorage.localStorageWrite({
    ///     key: "test-key",
    ///     value: "0x48656c6c6f",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 14)]
    async fn host_local_storage_write(
        &self,
        cx: &CallContext,
        request: HostLocalStorageWriteRequest,
    ) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>>;

    /// Clear a value by key.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function clearLocalValue(truapi: Client): Promise<void> {
    ///   const result = await truapi.localStorage.localStorageClear({ key: "test-key" });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 16)]
    async fn host_local_storage_clear(
        &self,
        cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>>;
}
