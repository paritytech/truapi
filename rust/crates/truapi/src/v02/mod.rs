//! TrUAPI Protocol v0.2 type definitions.
//!
//! v0.2 only introduces new payload shapes where they materially change from
//! v0.1. Methods that did not change between versions continue to use their
//! v0.1 types. Versioned envelopes in [`crate::versioned`] keep `V1` arms
//! intact so a host still on v0.1 — or a product still on v0.1 — keeps
//! decoding.

mod notifications;

pub use notifications::*;
