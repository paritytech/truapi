//! Versioned wrappers for [`Payment`](super::super::v02::Payment) methods (V0.2+).

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{
    AccountId, Balance, PaymentBalance, PaymentId, PaymentReceipt, PaymentStatus,
    PaymentTopUpSource,
};

/// Subscription request wrapper for `host_payment_balance_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentBalanceSubscribeRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostPaymentBalanceSubscribeRequest {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Subscription item wrapper for `host_payment_balance_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentBalanceItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(PaymentBalance),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(PaymentBalance),
}

impl Versioned for HostPaymentBalanceItem {
    type Inner = PaymentBalance;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_payment_top_up`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentTopUpRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1 {
        amount: Balance,
        source: PaymentTopUpSource,
    },
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2 {
        amount: Balance,
        source: PaymentTopUpSource,
    },
}

impl Versioned for HostPaymentTopUpRequest {
    type Inner = (Balance, PaymentTopUpSource);
    fn wrap(version: u8, (amount, source): Self::Inner) -> Self {
        match version {
            1 => Self::V1 { amount, source },
            _ => Self::V2 { amount, source },
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1 { amount, source } | Self::V2 { amount, source } => (amount, source),
        }
    }
}

/// Response wrapper for `host_payment_top_up`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentTopUpResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostPaymentTopUpResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `host_payment_request`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentRequestRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1 {
        amount: Balance,
        destination: AccountId,
    },
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2 {
        amount: Balance,
        destination: AccountId,
    },
}

impl Versioned for HostPaymentRequestRequest {
    type Inner = (Balance, AccountId);
    fn wrap(version: u8, (amount, destination): Self::Inner) -> Self {
        match version {
            1 => Self::V1 {
                amount,
                destination,
            },
            _ => Self::V2 {
                amount,
                destination,
            },
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1 {
                amount,
                destination,
            }
            | Self::V2 {
                amount,
                destination,
            } => (amount, destination),
        }
    }
}

/// Response wrapper for `host_payment_request`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentRequestResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(PaymentReceipt),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(PaymentReceipt),
}

impl Versioned for HostPaymentRequestResponse {
    type Inner = PaymentReceipt;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Subscription request wrapper for `host_payment_status_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentStatusSubscribeRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(PaymentId),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(PaymentId),
}

impl Versioned for HostPaymentStatusSubscribeRequest {
    type Inner = PaymentId;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Subscription item wrapper for `host_payment_status_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPaymentStatusItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(PaymentStatus),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(PaymentStatus),
}

impl Versioned for HostPaymentStatusItem {
    type Inner = PaymentStatus;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}
