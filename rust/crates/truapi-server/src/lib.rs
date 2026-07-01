//! TrUAPI server runtime support.
//!
//! This layer contains host-agnostic logic shared by the runtime and target
//! adapters. Wire dispatch and platform runtime wiring are added by later stack
//! layers.

#![forbid(unsafe_code)]

pub mod host_logic;
