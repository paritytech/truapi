//! Versioned request and response wrappers for the unified TrUAPI contract.
//!
//! Every wire-level request and response is expressed as a versioned enum,
//! one variant per protocol version. The codec discriminant is pinned with
//! `#[codec(index = N)]` so adding a future `V3` slot doesn't shift existing
//! versions on the wire.
//!
//! Wrappers expose two operations through [`Versioned`]:
//! - [`Versioned::into_inner`] — extract the latest-version inner shape
//!   regardless of which variant arrived (downgrade-decode for V1 → latest
//!   handled per-wrapper).
//! - [`Versioned::wrap`] — given the negotiated version and an inner value,
//!   build the corresponding `V<N>` variant for the response wire bytes.
//!
//! The dispatcher uses both: `into_inner` after decoding a request so the
//! handler sees a single shape; `wrap` before encoding the response so the
//! wire bytes match the version the client negotiated at handshake time.

/// Common interface every versioned wrapper exposes. Codegen will eventually
/// emit these impls; for now they're hand-rolled per wrapper.
pub trait Versioned: Sized {
    /// Latest-version inner shape. Tuple wrappers expose the inner type;
    /// unit wrappers use `()`; struct wrappers use a tuple of fields.
    type Inner;

    /// Construct the variant matching `version`. Unknown versions fall back
    /// to the latest variant — a forward-compat policy that lets us add new
    /// versions without breaking dispatch when the negotiated version code
    /// hasn't been updated.
    fn wrap(version: u8, inner: Self::Inner) -> Self;

    /// Discard the version envelope and yield the latest-version inner
    /// shape. Older variants are upgraded via the wrapper-specific `From`
    /// impl when their inner type differs from the latest.
    fn into_inner(self) -> Self::Inner;
}

pub mod account;
pub mod calls;
pub mod chain;
pub mod chat;
pub mod entropy;
pub mod local_storage;
pub mod payment;
pub mod permissions;
pub mod preimage;
pub mod signing;
pub mod statement_store;

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::{Decode, Encode};

    #[test]
    fn v1_and_v2_discriminants_match_codec_index() {
        let v1 = calls::HostFeatureSupportedRequest::V1(crate::v02::Feature::Chain(vec![1, 2, 3]));
        let v2 = calls::HostFeatureSupportedRequest::V2(crate::v02::Feature::Chain(vec![1, 2, 3]));
        assert_eq!(v1.encode()[0], 0, "V1 must encode discriminant 0");
        assert_eq!(v2.encode()[0], 1, "V2 must encode discriminant 1");
    }

    #[test]
    fn unit_response_roundtrip() {
        let original = calls::HostNavigateToResponse::V2;
        let decoded =
            calls::HostNavigateToResponse::decode(&mut &original.encode()[..]).expect("decode");
        assert_eq!(original, decoded);
    }

    #[test]
    fn struct_variant_roundtrip() {
        let original = local_storage::HostLocalStorageWriteRequest::V2 {
            key: "greeting".into(),
            value: b"hello".to_vec(),
        };
        let decoded =
            local_storage::HostLocalStorageWriteRequest::decode(&mut &original.encode()[..])
                .expect("decode");
        assert_eq!(original, decoded);
    }
}
