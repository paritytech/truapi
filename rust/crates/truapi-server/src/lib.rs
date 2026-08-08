#![allow(
    clippy::double_must_use,
    reason = "async-trait generates must_use futures for async trait methods"
)]

//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! Hosts instantiate a role runtime around a [`truapi_platform::Platform`]
//! implementation, then create product-scoped [`ProductRuntime`] endpoints that
//! expose the stable byte-frame API used from WASM, native mobile, or desktop
//! shells.
//!
//! Host-facing bridges:
//! - [`ws_bridge`] (feature `ws-bridge`): localhost WebSocket bridge for
//!   native WebView hosts (Android/iOS).
//! - [`native`]: UniFFI surface exposing the native host runtime + callbacks.
//! - `wasm` (wasm32 only): wasm-bindgen surface exposing `WasmProductRuntime`.

pub(crate) mod chain_runtime;
pub mod core;
pub(crate) mod dispatcher;
mod dynamic_vrf;
pub mod frame;
pub(crate) mod host_core;
pub mod host_logic;
pub(crate) mod host_rpc_client;
pub mod logging;
pub(crate) mod runtime;
pub mod subscription;
pub mod transport;

#[cfg(test)]
pub(crate) mod test_support;

pub mod generated;

#[cfg(all(not(target_arch = "wasm32"), feature = "ws-bridge"))]
pub mod ws_bridge;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(not(target_arch = "wasm32"))]
pub mod native_renderer;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use host_core::{
    FrameSink, HostAdmin, PairingHostRuntime, ProductRuntime, ProductRuntimeControl,
    ProductRuntimeError, SigningHostRuntime,
};
pub use host_logic::session::{
    ExternalPairedSession, SsoSessionInfo, decode_persisted_session, encode_external_paired_session,
};
pub use runtime::ResponderExit;
#[cfg(not(target_arch = "wasm32"))]
pub use runtime::statement_allowance;
pub use truapi_platform::{
    CoreStorageKeyDescription, CoreStorageKeyDescriptionError, HostRuntimeConfig,
    PairingHostConfig, PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform,
    ProductContext, SigningHostConfig, describe_core_storage_key,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "ws-bridge"))]
pub use ws_bridge::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native_renderer::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

#[cfg(not(target_arch = "wasm32"))]
truapi::uniffi_reexport_scaffolding!();

#[cfg(not(target_arch = "wasm32"))]
truapi_platform::uniffi_reexport_scaffolding!();
