//! TrUAPI server runtime support.
//!
//! This layer contains host-agnostic logic, wire-frame dispatch, and chain
//! JSON-RPC mechanics shared by the runtime and target adapters. Platform
//! runtime wiring is added by a later stack layer.

#![forbid(unsafe_code)]

pub(crate) mod chain_runtime;
pub(crate) mod dispatcher;
pub mod frame;
pub mod host_logic;
pub(crate) mod host_rpc_client;
pub mod subscription;
pub mod transport;

pub mod generated;
