/// Key name for local storage operations.
pub type StorageKey = String;

/// Binary value stored in local storage.
pub type StorageValue = Vec<u8>;

/// Local storage operation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Storage quota exceeded.
    Full,
    /// Catch-all.
    Unknown { reason: String },
}
