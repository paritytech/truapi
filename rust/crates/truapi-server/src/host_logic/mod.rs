//! Host-agnostic logic the Rust core owns on behalf of every platform host.
//!
//! Platform callbacks are a syscall layer for OS primitives (modals, native
//! storage, URL handler, notification center). Everything else lives here so
//! iOS, Android, and web hosts share one canonical implementation.

pub mod dotns;
pub mod entropy;
pub mod features;
pub mod permissions;
pub mod product_account;
pub mod session;
pub mod sso_pairing;
pub mod statement_store;
