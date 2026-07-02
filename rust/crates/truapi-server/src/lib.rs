//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! The host embedding path is [`HostCore::from_platform_with_config`]. It
//! wraps a [`truapi_platform::Platform`] implementation and exposes a stable
//! byte-frame API that target adapters can use from WASM, native mobile, or
//! desktop shells.
//!
//! Host-facing bridges:
//! - [`ws_bridge`] (feature `ws-bridge`): localhost WebSocket bridge for
//!   native WebView hosts (Android/iOS).
//! - [`native`]: UniFFI surface exposing `NativeTrUApiCore` + `HostCallbacks`.
//! - [`wasm`] (wasm32 only): wasm-bindgen surface exposing `WasmHostCore`.

#![forbid(unsafe_code)]

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

pub mod generated;

#[cfg(all(not(target_arch = "wasm32"), feature = "ws-bridge"))]
pub mod ws_bridge;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use host_core::{FrameSink, HostCore, HostCoreError};
pub use truapi_platform::{
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform, RuntimeConfig,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "ws-bridge"))]
pub use ws_bridge::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();
