//! Versioned wrappers for [`EntropyDerivation`](crate::api::EntropyDerivation) methods.

use crate::v01;

versioned_type! {
    pub enum HostDeriveEntropyRequest { V1 => v01::HostDeriveEntropyRequest }
    pub enum HostDeriveEntropyResponse { V1 => v01::HostDeriveEntropyResponse }
    pub enum HostDeriveEntropyError { V1 => v01::HostDeriveEntropyError }
}
