use parity_scale_codec::{Decode, Encode};

/// Balance amount for payment operations. Interpreted according to the host's
/// single fixed payment asset (e.g. pUSD).
pub type Balance = u128;

/// Current payment balance state pushed to subscribers.
///
/// See [RFC 0006]. V0.2: the `pending` field was removed; only `available`
/// remains.
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
        /// Ed25519 private key bytes.
        ed25519_private_key: [u8; 32],
    },
}

/// Request to top up the product payment balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentTopUpRequest {
    /// Amount to top up.
    pub amount: Balance,
    /// Funding source for the top-up.
    pub source: PaymentTopUpSource,
}

/// Request to initiate a payment to another account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentRequestRequest {
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
pub struct HostPaymentRequestResponse {
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

/// Error from [`crate::api::Payment::host_payment_balance_subscribe`].
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

/// Error from [`crate::api::Payment::host_payment_top_up`].
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
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Payment::host_payment_request`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentRequestError {
    /// User rejected the payment request.
    Rejected,
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
