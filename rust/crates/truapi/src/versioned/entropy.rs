//! Versioned wrappers for [`EntropyDerivation`](crate::api::EntropyDerivation) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostDeriveEntropyRequest`].
    pub enum HostDeriveEntropyRequest { V1 => v01::HostDeriveEntropyRequest }
    /// Versioned wrapper for [`v01::HostDeriveEntropyResponse`].
    pub enum HostDeriveEntropyResponse { V1 => v01::HostDeriveEntropyResponse }
    /// Versioned wrapper for [`v01::HostDeriveEntropyError`].
    pub enum HostDeriveEntropyError { V1 => v01::HostDeriveEntropyError }
}
