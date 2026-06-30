//! TrUAPI host runtime: frame codec, dispatcher, and subscription lifecycle.
//!
//! This crate turns an implementation of the `truapi::api` traits into a
//! transport-agnostic host runtime. It does not implement host capabilities
//! such as wallet access, chain access, payment, permissions, or storage.

#![forbid(unsafe_code)]

pub mod dispatcher;
pub mod frame;
pub mod generated;
pub mod subscription;
pub mod transport;

pub use dispatcher::*;
pub use frame::*;
pub use subscription::*;
pub use transport::*;
