//! Custom serde helpers for types serde's derive can't handle natively.
//!
//! Currently used only for fixed-size byte arrays (signatures, ring keys,
//! public keys) where the size exceeds serde's auto-implemented range
//! (`[T; 0..32]`). The helpers render as `0x`-prefixed hex strings, which
//! is what humans want to see in debug logs and what every wire codec
//! decodes.

use serde::Serializer;

/// Serializes any byte slice as a `0x`-prefixed lowercase hex string.
/// Pair with `#[serde(serialize_with = "...")]` on a `[u8; N]` field.
pub fn hex_bytes<S>(bytes: &[u8], ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_str(&format!("0x{}", hex::encode(bytes)))
}
