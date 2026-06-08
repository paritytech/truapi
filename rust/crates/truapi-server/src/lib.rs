//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! The platform path (`TrUApiCore::from_platform_with_config`) wraps a
//! [`truapi_platform::Platform`] in a `PlatformRuntimeHost` that implements
//! every `truapi::api::*` trait by delegating to platform callbacks.
//!
//! Host-facing bridges:
//! - [`ws_bridge`] (feature `ws-bridge`): localhost WebSocket bridge for
//!   native WebView hosts (Android/iOS).
//! - [`native`]: UniFFI surface exposing `NativeTrUApiCore` + `HostCallbacks`.
//! - [`wasm`] (wasm32 only): wasm-bindgen surface exposing `WasmTrUApiCore`.

#![forbid(unsafe_code)]

pub mod chain_runtime;
pub mod core;
pub mod debug_log;
pub mod dispatcher;
pub mod frame;
pub mod host_logic;
pub mod runtime;
pub mod subscription;
pub mod transport;

pub mod generated;

#[cfg(feature = "smoldot")]
pub mod smoldot_provider;

#[cfg(all(not(target_arch = "wasm32"), feature = "ws-bridge"))]
pub mod ws_bridge;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use crate::core::TrUApiCore;
pub use dispatcher::*;
pub use frame::*;
pub use runtime::PlatformRuntimeHost;
pub use subscription::*;
pub use transport::*;

#[cfg(all(not(target_arch = "wasm32"), feature = "ws-bridge"))]
pub use ws_bridge::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

uniffi::setup_scaffolding!();
