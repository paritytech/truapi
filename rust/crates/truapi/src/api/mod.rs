//! Unified TrUAPI trait set.

/// Wire ids held back for upstream `triangle-js-sdks` methods that TrUAPI
/// does not implement, but whose discriminants must remain free to keep our
/// wire-table positionally aligned with the canonical host `MessagePayload`
/// enum.
pub const RESERVED_WIRE_IDS: &[u8] = &[];

pub mod account;
pub mod chain;
pub mod chat;
pub mod entropy;
pub mod jsonrpc;
pub mod local_storage;
pub mod payment;
pub mod permissions;
pub mod preimage;
pub mod resource_allocation;
pub mod signing;
pub mod statement_store;
pub mod system;
pub mod theme;
pub mod transaction;

pub use account::Account;
pub use chain::Chain;
pub use chat::Chat;
pub use entropy::Entropy;
pub use jsonrpc::JsonRpc;
pub use local_storage::LocalStorage;
pub use payment::Payment;
pub use permissions::Permissions;
pub use preimage::Preimage;
pub use resource_allocation::ResourceAllocation;
pub use signing::Signing;
pub use statement_store::StatementStore;
pub use system::System;
pub use theme::Theme;
pub use transaction::Transaction;

/// The unified TrUAPI contract.
pub trait TrUApi:
    Account
    + Chain
    + Chat
    + Entropy
    + JsonRpc
    + LocalStorage
    + Payment
    + Permissions
    + Preimage
    + ResourceAllocation
    + Signing
    + StatementStore
    + System
    + Theme
    + Transaction
    + Send
    + Sync
{
}

impl<T> TrUApi for T where
    T: Account
        + Chain
        + Chat
        + Entropy
        + JsonRpc
        + LocalStorage
        + Payment
        + Permissions
        + Preimage
        + ResourceAllocation
        + Signing
        + StatementStore
        + System
        + Theme
        + Transaction
        + Send
        + Sync
{
}
