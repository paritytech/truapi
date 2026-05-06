//! Versioned wrappers for [`EntropyDerivation`](crate::api::EntropyDerivation) methods.

use crate::v02;

versioned_type! {
    /// Request wrapper for `host_derive_entropy` (V0.2+ only).
    pub enum HostDeriveEntropyRequest { V2 => Vec<u8> }
    /// Response wrapper for `host_derive_entropy` (V0.2+ only).
    pub enum HostDeriveEntropyResponse { V2 => v02::Entropy }
    /// Error wrapper for `host_derive_entropy` (V0.2+ only).
    pub enum HostDeriveEntropyError { V2 => v02::DeriveEntropyError }
}
