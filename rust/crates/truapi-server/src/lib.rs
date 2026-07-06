//! TrUAPI server runtime: dispatcher, frames, SCALE encoding, stream management.
//!
//! Hosts instantiate a role runtime around a [`truapi_platform::Platform`]
//! implementation, then create product-scoped [`ProductRuntime`] endpoints that
//! expose the stable byte-frame API used from WASM, native mobile, or desktop
//! shells.

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

pub use host_core::{
    FrameSink, HostAdmin, PairingHostRuntime, ProductRuntime, ProductRuntimeError,
    SigningHostRuntime,
};
pub use truapi_platform::{
    HostRuntimeConfig, PairingHostConfig, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, Platform, ProductContext, SigningHostConfig,
};
