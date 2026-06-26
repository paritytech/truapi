use parity_scale_codec::{Decode, Encode};

/// Request to write a value into local storage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageWriteRequest {
    /// Storage key to write.
    pub key: String,
    /// Value to store at the key.
    pub value: Vec<u8>,
}

/// Local storage operation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostLocalStorageReadError {
    /// Storage quota exceeded.
    Full,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to read a local storage value.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageReadRequest {
    /// Storage key to read.
    pub key: String,
}

/// Response containing an optional local storage value.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageReadResponse {
    /// Stored value, if present.
    pub value: Option<Vec<u8>>,
}

/// Request to clear a local storage key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageClearRequest {
    /// Storage key to clear.
    pub key: String,
}
