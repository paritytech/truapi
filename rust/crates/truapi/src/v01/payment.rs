use parity_scale_codec::{Decode, Encode};

use super::coin_payment::CoinPaymentPurseId;

/// Balance amount for payment operations. Interpreted according to the host's
/// single fixed payment asset (e.g. pUSD).
pub type Balance = u128;

/// Request to subscribe to payment balance updates.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentBalanceSubscribeRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub purse: Option<CoinPaymentPurseId>,
}

/// Current payment balance state pushed to subscribers.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentBalanceSubscribeItem {
    /// Balance that can be spent right now.
    pub available: Balance,
}

/// Source for a payment top-up operation.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PaymentTopUpSource {
    /// Fund from one of the calling product's scoped accounts.
    ProductAccount {
        /// Product account derivation index.
        derivation_index: u32,
    },
    /// Fund from a one-time account represented by its private key. This is a
    /// standard account holding public funds, not a coin key.
    PrivateKey {
        /// Sr25519 secret key bytes.
        sr25519_secret_key: [u8; 64],
    },
    /// Fund directly from coin secret keys. Each key is an sr25519 secret
    /// controlling a single coin.
    Coins {
        /// Sr25519 secret keys, one per coin.
        sr25519_secret_keys: Vec<[u8; 64]>,
    },
}

/// Request to top up the product payment balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentTopUpRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub into: Option<CoinPaymentPurseId>,
    /// Amount to top up.
    pub amount: Balance,
    /// Funding source for the top-up.
    pub source: PaymentTopUpSource,
}

/// Request to initiate a payment to another account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub from: Option<CoinPaymentPurseId>,
    /// Amount to pay.
    pub amount: Balance,
    /// Destination account.
    pub destination: [u8; 32],
}

/// Receipt returned after a successful payment request.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentResponse {
    /// The assigned payment identifier.
    pub id: String,
}

/// Payment lifecycle status pushed to subscribers.
///
/// Once a terminal state (`Completed` or `Failed`) is reached, the host
/// delivers it and may close the subscription.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentStatusSubscribeItem {
    /// Payment is being processed.
    Processing,
    /// Payment has been settled successfully.
    Completed,
    /// Payment has failed.
    Failed {
        /// Failure reason.
        reason: String,
    },
}

/// Error from [`crate::api::Payment::balance_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentBalanceSubscribeError {
    /// User denied the balance disclosure request.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::top_up`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentTopUpError {
    /// The source account does not hold sufficient funds.
    InsufficientFunds,
    /// The source account was not found or is invalid.
    InvalidSource,
    /// Some coins were claimed but the total fell short of the requested amount.
    PartialPayment {
        /// Amount that was successfully credited.
        credited: Balance,
    },
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::request`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentError {
    /// User rejected the payment request.
    Rejected,
    /// User's available balance is not sufficient for the requested amount.
    InsufficientBalance,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::status_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentStatusSubscribeError {
    /// Payment ID was not found or does not belong to the current product.
    PaymentNotFound,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to subscribe to a payment status.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentStatusSubscribeRequest {
    /// Payment identifier to watch.
    pub payment_id: String,
}
