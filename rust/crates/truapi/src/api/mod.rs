//! Unified TrUAPI trait set.
//!
//! Sub-traits define the canonical host API surface. Each method takes a
//! versioned request enum from [`crate::versioned`] and returns a versioned
//! response enum (or a [`crate::Subscription`] item enum for streamed
//! endpoints). Shared error and value types come from the protocol version
//! modules.
//!
//! The [`TrUApi`] supertrait composes every sub-trait. A single
//! implementation of [`TrUApi`] is the canonical host contract consumed by
//! `truapi-codegen` and the generated Rust dispatcher.
//!
//! Every method must carry a stable wire id annotation. Request/response
//! methods use `#[wire(request_id = N)]`; subscriptions use
//! `#[wire(start_id = N)]`. Omitted peer ids are inferred consecutively
//! (`_response`, or `_stop`, `_interrupt`, `_receive`) and can be overridden
//! explicitly for compatibility gaps. Removing or reordering a slot is a
//! wire-breaking change; retired methods leave documented gaps. Codegen
//! derives method availability from the versioned request, response, item, and
//! error wrappers.

/// Wire ids held back for upstream `triangle-js-sdks` methods that TrUAPI
/// does not implement, but whose discriminants must remain free to keep our
/// wire-table positionally aligned with the canonical host `MessagePayload`
/// enum. `truapi-codegen` links this crate at compile time and rejects any
/// `#[wire(...)]` annotation whose id falls in the reserved set.
///
/// Slot owners are documented on [`calls::TrUApiCalls`].
pub const RESERVED_WIRE_IDS: &[u8] = &[
    34, 35, 36, 37, 68, 69, 70, 71, 72, 73, 74, 75, 104, 105, 106, 107, 112, 113,
];

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
