//! TrUAPI trait and type definitions for the dotli product SDK.
//!
//! This crate provides three protocol versions as separate modules:
//!
//! - [`v01`] — Protocol v0.1 (stable).
//! - [`v02`] — Protocol v0.2.
//! - [`v02_5`] — Protocol v0.2.5.

#![forbid(unsafe_code)]

pub mod v01;
pub mod v02;
pub mod v02_5;
