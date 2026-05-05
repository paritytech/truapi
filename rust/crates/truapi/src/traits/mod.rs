//! Unified TrUAPI trait set.
//!
//! Sub-traits mirror the topic groupings of [`crate::v02`]. Each method takes a
//! versioned request enum from [`crate::versioned`] and returns a versioned
//! response enum (or a [`crate::Subscription`] item enum for streamed
//! endpoints). Error types are borrowed from [`crate::v02`] unchanged.
//!
//! The [`TrUApi`] supertrait composes every sub-trait. A single
//! implementation of [`TrUApi`] is the canonical host contract consumed by
//! `truapi-codegen` and the generated Rust dispatcher.
//!
//! Every method must carry a stable `#[wire(id = N)]` annotation. The id is
//! part of the append-only wire protocol: request/response methods consume two
//! consecutive slots (`_request`, `_response`) and subscriptions consume four
//! (`_start`, `_stop`, `_interrupt`, `_receive`). Removing or reordering a
//! slot is a wire-breaking change; retired methods leave documented gaps.

pub mod account;
pub mod calls;
pub mod chain;
pub mod chat;
pub mod entropy;
pub mod local_storage;
pub mod payment;
pub mod permissions;
pub mod preimage;
pub mod signing;
pub mod statement_store;

pub use account::AccountManagement;
pub use calls::TrUApiCalls;
pub use chain::ChainInteraction;
pub use chat::Chat;
pub use entropy::EntropyDerivation;
pub use local_storage::LocalStorage;
pub use payment::Payment;
pub use permissions::Permissions;
pub use preimage::Preimage;
pub use signing::Signing;
pub use statement_store::StatementStore;

/// The unified TrUAPI contract. Composes every sub-trait so a host can be
/// expressed as a single `impl TrUApi for MyHost` rather than an
/// implementation per domain.
pub trait TrUApi:
    AccountManagement
    + ChainInteraction
    + Chat
    + EntropyDerivation
    + LocalStorage
    + Payment
    + Permissions
    + Preimage
    + Signing
    + StatementStore
    + TrUApiCalls
    + Send
    + Sync
{
}

impl<T> TrUApi for T where
    T: AccountManagement
        + ChainInteraction
        + Chat
        + EntropyDerivation
        + LocalStorage
        + Payment
        + Permissions
        + Preimage
        + Signing
        + StatementStore
        + TrUApiCalls
        + Send
        + Sync
{
}
