//! Golden SCALE wire-vector generator for cross-language codec conformance.
//!
//! Encodes representative protocol values with `parity_scale_codec` (the
//! canonical Rust encoder) and writes `{ name: hex }` JSON. The Dart client's
//! `wire_vectors_test.dart` loads the file and asserts its generated codecs
//! produce byte-identical output, so the Rust crate stays the source of truth.
//!
//! Run from the repo root:
//! ```bash
//! cargo run -p truapi --example wire_vectors -- dart/truapi/test/wire_vectors.json
//! ```

use parity_scale_codec::{Compact, Encode, OptionBool};
use truapi::v01;
use truapi::versioned;

// Sequential `push` reads clearly for a flat list of independent vectors.
#[allow(clippy::vec_init_then_push)]
fn main() {
    let mut vectors: Vec<(&str, Vec<u8>)> = Vec::new();

    vectors.push((
        "product_account_id",
        v01::ProductAccountId {
            dot_ns_identifier: "my-product.dot".to_string(),
            derivation_index: 7,
        }
        .encode(),
    ));

    vectors.push((
        "product_account",
        v01::ProductAccount {
            public_key: vec![1, 2, 3, 4],
        }
        .encode(),
    ));

    vectors.push((
        "legacy_account_some",
        v01::LegacyAccount {
            public_key: vec![0xaa, 0xbb],
            name: Some("Wallet".to_string()),
        }
        .encode(),
    ));

    vectors.push((
        "legacy_account_none",
        v01::LegacyAccount {
            public_key: vec![],
            name: None,
        }
        .encode(),
    ));

    vectors.push((
        "handshake_request",
        v01::HostHandshakeRequest { codec_version: 1 }.encode(),
    ));

    vectors.push((
        "handshake_error_unsupported",
        v01::HostHandshakeError::UnsupportedProtocolVersion.encode(),
    ));

    vectors.push((
        "typography_body_large",
        v01::TypographyStyle::BodyLargeRegular.encode(),
    ));

    vectors.push((
        "dimensions",
        v01::Dimensions {
            top: Compact(10),
            end: Compact(20),
            bottom: None,
            start: Some(Compact(5)),
        }
        .encode(),
    ));

    vectors.push((
        "button_props",
        v01::ButtonProps {
            text: "Go".to_string(),
            variant: Some(v01::ButtonVariant::Primary),
            enabled: OptionBool(Some(true)),
            loading: OptionBool(None),
            click_action: Some("go".to_string()),
        }
        .encode(),
    ));

    vectors.push((
        "account_get_alias_response",
        v01::HostAccountGetAliasResponse {
            context: [7u8; 32],
            alias: vec![9, 9],
        }
        .encode(),
    ));

    // Versioned envelope: V1 wrapper writes discriminant byte 0x00 then inner.
    vectors.push((
        "versioned_handshake_request_v1",
        versioned::system::HostHandshakeRequest::V1(v01::HostHandshakeRequest { codec_version: 1 })
            .encode(),
    ));

    let entries = vectors
        .iter()
        .map(|(name, bytes)| format!("  \"{name}\": \"{}\"", hex::encode(bytes)))
        .collect::<Vec<_>>()
        .join(",\n");
    let json = format!("{{\n{entries}\n}}\n");

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dart/truapi/test/wire_vectors.json".to_string());
    std::fs::write(&path, json).expect("write wire vectors");
    eprintln!("Wrote {} vectors to {path}", vectors.len());
}
