use parity_scale_codec::{Decode, Encode};

/// Key name for local storage operations.
pub type StorageKey = String;

/// Binary value stored in local storage.
pub type StorageValue = Vec<u8>;

/// Request to write a value into local storage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct LocalStorageWriteRequest {
    /// Storage key to write.
    pub key: StorageKey,
    /// Value to store at the key.
    pub value: StorageValue,
}

/// Local storage operation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum StorageError {
    /// Storage quota exceeded.
    Full,
    /// Catch-all.
    Unknown { reason: String },
}
