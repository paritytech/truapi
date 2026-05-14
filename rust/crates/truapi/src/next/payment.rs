use parity_scale_codec::{Decode, Encode};

use crate::v01::{Balance, PaymentTopUpSource, PurseId};

/// Request to subscribe to payment balance updates with an optional purse
/// selector (RFC 0017).
///
/// `None` purse selects the ordinary user-owned main purse. `Some(purse)`
/// selects a specific CoinPayment purse when the calling product is authorized.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentBalanceSubscribeRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub purse: Option<PurseId>,
}

/// Request to top up the product payment balance with an optional purse
/// selector (RFC 0017).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentTopUpRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub into: Option<PurseId>,
    /// Amount to top up.
    pub amount: Balance,
    /// Funding source for the top-up.
    pub source: PaymentTopUpSource,
}

/// Request to initiate a payment from an optional purse (RFC 0017).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentRequestRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub from: Option<PurseId>,
    /// Amount to pay.
    pub amount: Balance,
    /// Destination account.
    pub destination: [u8; 32],
}
