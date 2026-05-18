//! Unified TrUAPI trait set.

pub mod account;
pub mod chain;
pub mod chat;
pub mod coin_payment;
pub mod entropy;
pub mod jsonrpc;
pub mod local_storage;
pub mod notifications;
pub mod payment;
pub mod permissions;
pub mod preimage;
pub mod resource_allocation;
pub mod signing;
pub mod statement_store;
pub mod system;
pub mod theme;

pub use account::Account;
pub use chain::Chain;
pub use chat::Chat;
pub use coin_payment::CoinPayment;
pub use entropy::Entropy;
pub use jsonrpc::JsonRpc;
pub use local_storage::LocalStorage;
pub use notifications::Notifications;
pub use payment::Payment;
pub use permissions::Permissions;
pub use preimage::Preimage;
pub use resource_allocation::ResourceAllocation;
pub use signing::Signing;
pub use statement_store::StatementStore;
pub use system::System;
pub use theme::Theme;

/// The unified TrUAPI contract.
pub trait TrUApi:
    Account
    + Chain
    + Chat
    + CoinPayment
    + Entropy
    + JsonRpc
    + LocalStorage
    + Notifications
    + Payment
    + Permissions
    + Preimage
    + ResourceAllocation
    + Signing
    + StatementStore
    + System
    + Theme
    + Send
    + Sync
{
}

impl<T> TrUApi for T where
    T: Account
        + Chain
        + Chat
        + CoinPayment
        + Entropy
        + JsonRpc
        + LocalStorage
        + Notifications
        + Payment
        + Permissions
        + Preimage
        + ResourceAllocation
        + Signing
        + StatementStore
        + System
        + Theme
        + Send
        + Sync
{
}
