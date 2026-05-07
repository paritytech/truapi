use parity_scale_codec::{Decode, Encode};

use crate::v01::{AccountId, DerivationIndex};

/// Balance amount for payment operations. Interpreted according to the host's
/// single fixed payment asset (e.g. pUSD).
pub type Balance = u128;

/// Unique payment identifier, scoped to the product that created it.
pub type PaymentId = String;

/// Ed25519 private key bytes (32 bytes).
pub type Ed25519PrivateKey = [u8; 32];

/// Current payment balance state pushed to subscribers.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct PaymentBalance {
    /// Balance that can be spent right now.
    pub available: Balance,
    /// Balance the user possesses but cannot spend yet (e.g. in recycling
    /// stage).
    pub pending: Balance,
}

/// Source for a payment top-up operation.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PaymentTopUpSource {
    /// Fund from one of the calling product's scoped accounts.
    ProductAccount(DerivationIndex),
    /// Fund from a one-time account represented by its private key. This is a
    /// standard account holding public funds, not a coin key.
    PrivateKey(Ed25519PrivateKey),
}

/// Request to top up the product payment balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct PaymentTopUpRequest {
    /// Amount to top up.
    pub amount: Balance,
    /// Funding source for the top-up.
    pub source: PaymentTopUpSource,
}

/// Request to initiate a payment to another account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct PaymentRequest {
    /// Amount to pay.
    pub amount: Balance,
    /// Destination account.
    pub destination: AccountId,
}

/// Receipt returned after a successful payment request.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct PaymentReceipt {
    /// The assigned payment identifier.
    pub id: PaymentId,
}

/// Payment lifecycle status pushed to subscribers.
///
/// Once a terminal state (`Completed` or `Failed`) is reached, the host
/// delivers it and may close the subscription.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PaymentStatus {
    /// Payment is being processed.
    Processing,
    /// Payment has been settled successfully.
    Completed,
    /// Payment has failed.
    Failed(String),
}

/// Error from [`crate::api::Payment::host_payment_balance_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PaymentBalanceError {
    /// User denied the balance disclosure request.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::host_payment_top_up`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PaymentTopUpError {
    /// The source account does not hold sufficient funds.
    InsufficientFunds,
    /// The source account was not found or is invalid.
    InvalidSource,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::host_payment_request`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PaymentRequestError {
    /// User denied the payment request.
    Denied,
    /// User's available balance is not sufficient for the requested amount.
    InsufficientBalance,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::host_payment_status_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PaymentStatusError {
    /// Payment ID was not found or does not belong to the current product.
    PaymentNotFound,
    /// Catch-all.
    Unknown { reason: String },
}

pub type HostPaymentBalanceSubscribeItem = PaymentBalance;
pub type HostPaymentBalanceSubscribeError = PaymentBalanceError;
pub type HostPaymentTopUpRequest = PaymentTopUpRequest;
pub type HostPaymentTopUpError = PaymentTopUpError;
pub type HostPaymentRequestRequest = PaymentRequest;
pub type HostPaymentRequestResponse = PaymentReceipt;
pub type HostPaymentRequestError = PaymentRequestError;
pub type HostPaymentStatusSubscribeRequest = PaymentId;
pub type HostPaymentStatusSubscribeItem = PaymentStatus;
pub type HostPaymentStatusSubscribeError = PaymentStatusError;
