use parity_scale_codec::{Decode, Encode};

/// A 32-byte chain genesis hash used to identify the target chain.
pub type GenesisHash = [u8; 32];

/// A 32-byte raw account identifier used for legacy (non-product) accounts.
pub type AccountId = [u8; 32];

/// A signed extension for a transaction payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadExtension {
    /// Extension name (e.g., `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Vec<u8>,
}

/// Transaction payload for a product account.
///
/// Contains everything the host needs to construct a signed extrinsic.
/// The signer is identified by its derivation index within the caller's own
/// product; the host resolves the corresponding key pair through its account
/// management layer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductAccountTxPayload {
    /// Derivation index of the caller's product account that will sign the transaction.
    pub derivation_index: u32,
    /// Chain where the transaction will execute.
    pub genesis_hash: GenesisHash,
    /// SCALE-encoded Call data.
    pub call_data: Vec<u8>,
    /// Transaction extensions supplied by the caller.
    pub extensions: Vec<TxPayloadExtension>,
    /// 0 for Extrinsic V4, runtime-supported value for V5.
    pub tx_ext_version: u8,
}

/// Transaction payload for a legacy (non-product) account.
///
/// Identical to [`ProductAccountTxPayload`] except the signer is a raw
/// 32-byte [`AccountId`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LegacyAccountTxPayload {
    /// Raw 32-byte public key of the legacy account.
    pub signer: AccountId,
    /// Chain where the transaction will execute.
    pub genesis_hash: GenesisHash,
    /// SCALE-encoded Call data.
    pub call_data: Vec<u8>,
    /// Transaction extensions supplied by the caller.
    pub extensions: Vec<TxPayloadExtension>,
    /// 0 for Extrinsic V4, runtime-supported value for V5.
    pub tx_ext_version: u8,
}

/// Transaction creation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCreateTransactionError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected.
    Rejected,
    /// Unsupported payload version or extension.
    NotSupported {
        /// Unsupported payload or extension reason.
        reason: String,
    },
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}
