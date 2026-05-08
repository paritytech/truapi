//! Versioned wrappers for [`EntropyDerivation`](crate::api::EntropyDerivation) methods.

use crate::v02;

versioned_type! {
    /// Versioned wrapper for [`v02::HostDeriveEntropyRequest`] and older versions.
    pub enum HostDeriveEntropyRequest { V2 => v02::HostDeriveEntropyRequest }
    /// Versioned wrapper for [`v02::HostDeriveEntropyResponse`] and older versions.
    pub enum HostDeriveEntropyResponse { V2 => v02::HostDeriveEntropyResponse }
    /// Versioned wrapper for [`v02::HostDeriveEntropyError`] and older versions.
    pub enum HostDeriveEntropyError { V2 => v02::HostDeriveEntropyError }
}
