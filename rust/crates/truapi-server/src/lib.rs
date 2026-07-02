//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! Hosts instantiate a role runtime around a [`truapi_platform::Platform`]
//! implementation, then create product-scoped [`ProductRuntime`] endpoints that
//! expose the stable byte-frame API used from WASM, native mobile, or desktop
//! shells.
//!
//! Host-facing bridges:
//! - `wasm` (wasm32 only): wasm-bindgen surface exposing `WasmProductRuntime`.

pub(crate) mod chain_runtime;
pub mod core;
pub(crate) mod dispatcher;
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

/// Deterministic in-process mock wallet (SSO/statement-store seam) composed with
/// `truapi-platform`'s `MockPlatform`. Available in tests, or under the `mock`
/// feature for out-of-crate consumers (browser E2E). Never in the default build.
#[cfg(any(test, feature = "mock"))]
pub mod mock_wallet;

pub mod generated;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use host_core::{
    FrameSink, HostAdmin, PairingHostRuntime, ProductRuntime, ProductRuntimeError,
    SigningHostRuntime,
};
pub use truapi_platform::{
    HostRuntimeConfig, PairingHostConfig, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, Platform, ProductContext, SigningHostConfig,
};

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
