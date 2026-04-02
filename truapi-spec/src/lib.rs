//! TrUAPI trait and type definitions for the dotli product SDK.
//!
//! This crate provides two protocol versions as separate modules:
//!
//! - [`v01`] — Protocol v0.1 (stable).
//! - [`v02`] — Protocol v0.2-preview (draft, subject to change).

#![forbid(unsafe_code)]

pub mod v01;
pub mod v02;
