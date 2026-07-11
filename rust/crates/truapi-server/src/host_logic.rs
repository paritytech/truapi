//! Host-agnostic logic the Rust core owns on behalf of every platform host.
//!
//! Platform callbacks are a syscall layer for OS primitives (modals, native
//! storage, URL handler, notification center). Everything else lives here so
//! iOS, Android, and web hosts share one canonical implementation.

/// Bandersnatch ring-VRF product-account aliases (native signing host only).
#[cfg(not(target_arch = "wasm32"))]
pub mod alias;
/// Lite-person username registration parameters (native signing host only).
#[cfg(not(target_arch = "wasm32"))]
pub mod attestation;
pub mod bulletin;
pub mod dotns;
pub mod entropy;
pub mod extrinsic;
pub mod features;
pub mod identity;
pub mod permissions;
pub mod product_account;
pub mod session;
pub mod session_store;
pub mod sso;
pub mod statement_store;
pub mod transaction;
