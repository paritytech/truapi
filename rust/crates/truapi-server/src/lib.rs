//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! The host embedding path is [`HostCore::from_platform_with_config`]. It
//! wraps a [`truapi_platform::Platform`] implementation and exposes a stable
//! byte-frame API that target adapters can use from WASM, native mobile, or
//! desktop shells.
//!
//! Host-facing bridges:
//! - [`wasm`] (wasm32 only): wasm-bindgen surface exposing `WasmHostCore`.

#![forbid(unsafe_code)]

pub(crate) mod chain_runtime;
pub mod core;
pub(crate) mod dispatcher;
pub mod frame;
pub(crate) mod host_core;
pub mod host_logic;
pub mod logging;
pub(crate) mod runtime;
pub mod subscription;
pub mod transport;

#[cfg(test)]
pub(crate) mod test_support;

pub mod generated;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use host_core::{FrameSink, HostCore, HostCoreError};
pub use truapi_platform::{
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform, RuntimeConfig,
};

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
