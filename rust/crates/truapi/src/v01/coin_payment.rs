use parity_scale_codec::{Decode, Encode};

/// RFC 0017 CoinPayment purse identifier.
pub type CoinPaymentPurseId = u32;

/// Well-known ordinary user-owned CoinPayment purse.
pub const MAIN_PURSE: CoinPaymentPurseId = u32::MAX;

/// Balance amount for CoinPayment operations.
pub type CoinPaymentBalance = u32;

/// Milliseconds since Unix epoch.
pub type CoinPaymentTimestamp = u64;

/// Authenticated product identifier recorded for a product-created purse.
pub type CoinPaymentProductId = String;

/// Public key identifying a CoinPayment receivable.
pub type CoinPaymentReceivable = [u8; 32];

/// Merkle root for a product-visible clearing reference.
pub type CoinPaymentMerkleRoot = [u8; 32];

/// Transaction hash for a product-visible clearing reference.
pub type CoinPaymentTransactionHash = [u8; 32];

/// Public Coinage key referenced by clearing evidence.
pub type CoinPaymentCoinagePubKey = [u8; 32];

/// Product-visible metadata and balance state for a CoinPayment purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinPaymentPurseInfo {
    /// Human-readable purse name supplied by the creating product.
    pub name: String,
    /// Creation timestamp.
    pub created: CoinPaymentTimestamp,
    /// Product that created the purse.
    pub creator: CoinPaymentProductId,
    /// Current product-visible balance.
    pub balance: CoinPaymentBalance,
}

/// Standardized encrypted Coinage secret transmission payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinPaymentCheque {
    /// Cheque format version. V1 uses 0.
    pub version: u8,
    /// Receivable public key protecting the cheque contents.
    pub id: CoinPaymentReceivable,
    /// Claimed payment amount.
    pub amount: CoinPaymentBalance,
    /// Concatenated coin secrets encrypted to the receivable.
    pub encrypted_secrets: Vec<u8>,
}

/// Errors returned by CoinPayment host operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoinPaymentError {
    /// Source purse has too little balance.
    BalanceLow,
    /// User agent denied spend, transfer, or access.
    Denied,
    /// Coin secrets do not control valid coins.
    BadCoins,
    /// Coin secrets were claimed elsewhere.
    SnipedCoins,
    /// Purse does not exist or is not visible to the caller.
    PurseNotFound,
    /// Receivable does not exist or is not visible to the caller.
    ReceivableNotFound,
    /// Requested transmission channel is not supported.
    UnsupportedChannel,
    /// Required host/user-agent capability is unavailable.
    UserAgentCapabilityUnavailable,
    /// Unexpected runtime failure.
    Internal,
}

/// Error from [`crate::api::CoinPayment::create_purse`].
pub type HostCoinPaymentCreatePurseError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::query_purse`].
pub type HostCoinPaymentQueryPurseError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::rebalance_purse`].
pub type HostCoinPaymentRebalancePurseError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::delete_purse`].
pub type HostCoinPaymentDeletePurseError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::create_receivable`].
pub type HostCoinPaymentCreateReceivableError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::create_cheque`].
pub type HostCoinPaymentCreateChequeError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::deposit`].
pub type HostCoinPaymentDepositError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::refund`].
pub type HostCoinPaymentRefundError = CoinPaymentError;

/// Error from [`crate::api::CoinPayment::listen_for`].
pub type HostCoinPaymentListenForError = CoinPaymentError;

/// Product-visible clearing reference for reconciliation and receipts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinPaymentClearingReference {
    /// Clearing Merkle root.
    pub root: CoinPaymentMerkleRoot,
    /// Product-visible coin key and transaction hash leaves.
    pub leaves: Vec<(CoinPaymentCoinagePubKey, CoinPaymentTransactionHash)>,
}

/// Clearing status stream item.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoinPaymentStatus {
    /// More coins have cleared.
    Clearing {
        /// Amount clearing in this update.
        clearing: CoinPaymentBalance,
        /// Cumulative cleared amount.
        cleared: CoinPaymentBalance,
    },
    /// Some or all coins failed to transfer.
    Failed {
        /// Failure reason.
        error: CoinPaymentError,
        /// Cumulative cleared amount.
        cleared: CoinPaymentBalance,
        /// Clearing reference for any cleared portion.
        reference: CoinPaymentClearingReference,
    },
    /// All coins cleared.
    Done {
        /// Cleared amount.
        cleared: CoinPaymentBalance,
        /// Clearing reference.
        reference: CoinPaymentClearingReference,
    },
}

/// Standardized cheque transmission channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoinPaymentTransmissionChannel {
    /// Statement-store/HOP handoff identified by an SSS topic.
    Standard {
        /// Statement-store topic.
        sss_topic: [u8; 32],
    },
}

/// Request to create a new firewalled CoinPayment purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreatePurseRequest {
    /// Human-readable purse name.
    pub name: String,
}

/// Created purse identifier.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreatePurseResponse {
    /// Assigned purse identifier.
    pub purse: CoinPaymentPurseId,
}

/// Request to query product-visible purse metadata.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentQueryPurseRequest {
    /// Purse to query.
    pub purse: CoinPaymentPurseId,
}

/// Product-visible purse metadata response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentQueryPurseResponse {
    /// Purse information.
    pub info: CoinPaymentPurseInfo,
}

/// Request to transfer balance between local purses.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentRebalancePurseRequest {
    /// Source purse.
    pub from: CoinPaymentPurseId,
    /// Destination purse.
    pub to: CoinPaymentPurseId,
    /// Amount to move.
    pub amount: CoinPaymentBalance,
}

/// Request to delete a purse after draining its balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentDeletePurseRequest {
    /// Purse to delete.
    pub target: CoinPaymentPurseId,
    /// Purse that receives drained funds.
    pub drain_into: CoinPaymentPurseId,
}

/// Request to create a fresh receivable for a purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateReceivableRequest {
    /// Target purse for future deposits.
    pub into: CoinPaymentPurseId,
}

/// Created receivable response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateReceivableResponse {
    /// Receivable public key.
    pub receivable: CoinPaymentReceivable,
}

/// Request to create a cheque from a local purse to a receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateChequeRequest {
    /// Source purse.
    pub from: CoinPaymentPurseId,
    /// Destination receivable.
    pub to: CoinPaymentReceivable,
    /// Payment amount.
    pub amount: CoinPaymentBalance,
}

/// Created cheque response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateChequeResponse {
    /// Encrypted cheque.
    pub cheque: CoinPaymentCheque,
}

/// Request to deposit a cheque into the purse associated with its receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentDepositRequest {
    /// Cheque to deposit.
    pub cheque: CoinPaymentCheque,
}

/// Request to refund coins associated with a receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentRefundRequest {
    /// Receivable to refund.
    pub receivable: CoinPaymentReceivable,
}

/// Request to listen for a cheque delivered to a receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentListenForRequest {
    /// Receivable to listen for.
    pub receivable: CoinPaymentReceivable,
}

/// Stream item for [`crate::api::CoinPayment::listen_for`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCoinPaymentListenForItem {
    /// Handoff channel suitable for inclusion in an invoice.
    Channel(CoinPaymentTransmissionChannel),
    /// Cheque received through the handoff channel.
    Cheque(CoinPaymentCheque),
}
