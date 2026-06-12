//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! The platform path (`TrUApiCore::from_platform_with_config`) wraps a
//! [`truapi_platform::Platform`] in a `PlatformRuntimeHost` that implements
//! every `truapi::api::*` trait by delegating to platform callbacks.
//!
//! Host-facing bridges:
//! - [`wasm`] (wasm32 only): wasm-bindgen surface exposing `WasmTrUApiCore`.

#![forbid(unsafe_code)]

pub mod chain_runtime;
pub mod core;
pub mod dispatcher;
pub mod frame;
pub mod host_logic;
pub mod logging;
pub mod runtime;
pub mod subscription;
pub mod transport;

#[cfg(test)]
pub(crate) mod test_support;

pub mod generated;

#[cfg(feature = "smoldot")]
pub mod smoldot_provider;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use crate::core::TrUApiCore;
pub use dispatcher::*;
pub use frame::*;
pub use runtime::PlatformRuntimeHost;
pub use subscription::*;
pub use transport::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
