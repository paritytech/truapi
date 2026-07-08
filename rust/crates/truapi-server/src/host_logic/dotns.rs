//! dotns URL parsing, normalization, and classification.
//!
//! The Rust core owns the whole decision so every platform host sees the
//! same categorization and the `navigate_to` callback only receives
//! already-validated input.

use unicode_normalization::UnicodeNormalization;
use url::Url;

/// How the input URL should be opened. Kept in one enum rather than passing
/// a raw string so the dispatcher can reject invalid input before reaching
/// any platform callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigateDecision {
    /// A `.dot` identifier plus path/query/hash suffix (no leading `/`).
    DotName {
        /// Lower-cased `.dot` host (e.g. `mytestapp.dot`).
        identifier: String,
        /// Path/query/hash suffix without a leading `/`.
        path: String,
    },
    /// A `localhost[:port]` URL plus path/query/hash suffix (no leading `/`).
    Localhost {
        /// `localhost` with optional `:port` suffix.
        host: String,
        /// Path/query/hash suffix without a leading `/`.
        path: String,
    },
    /// An absolute external URL with an `http(s):` scheme prepended if missing.
    External {
        /// Canonical URL string.
        url: String,
    },
    /// Input that fails every branch: empty, unparseable, or a `.dot` URL
    /// carrying port/userinfo (both forbidden since dotns resolves via the
    /// chain and has no notion of either).
    Reject {
        /// Human-readable reason for the rejection.
        reason: String,
    },
}

impl NavigateDecision {
    /// Canonical URL string for the three `Open*` variants; `None` for
    /// `Reject`. `DotName` and `Localhost` keep the dotns/localhost identity
    /// visible so env-aware hosts can rewrite `.dot` names for their active
    /// environment and re-parse without losing information.
    pub fn canonical_url(&self) -> Option<String> {
        match self {
            Self::DotName { identifier, path } => Some(join_url("https://", identifier, path)),
            Self::Localhost { host, path } => Some(join_url("http://", host, path)),
            Self::External { url } => Some(url.clone()),
            Self::Reject { .. } => None,
        }
    }
}

fn join_url(scheme: &str, host: &str, path: &str) -> String {
    if path.is_empty() {
        format!("{scheme}{host}")
    } else {
        format!("{scheme}{host}/{path}")
    }
}

/// Classify a URL the way the host navigation handler does: try `.dot` first,
/// then `localhost`, then normalize as external.
pub fn parse_navigate(input: &str) -> NavigateDecision {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return NavigateDecision::Reject {
            reason: "empty input".to_string(),
        };
    }

    if let Some(decision) = classify_dot(trimmed) {
        return decision;
    }

    if let Some(decision) = classify_localhost(trimmed) {
        return decision;
    }

    match normalize_external(trimmed) {
        Ok(url) => NavigateDecision::External { url },
        Err(reason) => NavigateDecision::Reject { reason },
    }
}

/// Canonical host form: case-folded and NFC-normalized (belt-and-suspenders;
/// `url` already applies IDNA to parsed hosts), with a trailing root dot
/// dropped so the absolute form `example.dot.` keys identically to
/// `example.dot`.
fn normalize_host(host: &str) -> String {
    let normalized: String = host.nfc().collect::<String>().to_lowercase();
    normalized
        .strip_suffix('.')
        .unwrap_or(&normalized)
        .to_string()
}

/// `.dot` TLD check, applied to the [`normalize_host`] form so `Example.DOT`
/// and the trailing-dot FQDN `example.dot.` classify like `example.dot`.
fn is_dot_domain(host: &str) -> bool {
    normalize_host(host).ends_with(".dot")
}

fn parse_with_explicit_https(input: &str) -> Option<Url> {
    if let Ok(direct) = Url::parse(input) {
        return Some(direct);
    }
    Url::parse(&format!("https://{input}")).ok()
}

/// Recognize `.dot` URLs (including the `polkadot://` scheme). Returns:
/// - `Some(DotName)` for a clean `.dot` URL
/// - `Some(Reject)` for a `.dot` URL with port or userinfo
/// - `None` when the input isn't a `.dot` URL (caller falls through to
///   localhost / external)
fn classify_dot(input: &str) -> Option<NavigateDecision> {
    let parsed = if input.starts_with("polkadot://") {
        Url::parse(input).ok()?
    } else {
        parse_with_explicit_https(input)?
    };

    let hostname = parsed.host_str()?;
    if !is_dot_domain(hostname) {
        return None;
    }

    if parsed.port().is_some() || !parsed.username().is_empty() || parsed.password().is_some() {
        return Some(NavigateDecision::Reject {
            reason: format!("{hostname} carries port or userinfo; dotns forbids both"),
        });
    }

    Some(NavigateDecision::DotName {
        identifier: normalize_host(hostname),
        path: strip_leading_slash(parsed.path()) + &suffix(&parsed),
    })
}

/// Recognize `localhost[:port]` URLs, with or without an explicit scheme.
fn classify_localhost(input: &str) -> Option<NavigateDecision> {
    let with_scheme = if input.starts_with("localhost") {
        format!("http://{input}")
    } else {
        input.to_string()
    };

    let parsed = Url::parse(&with_scheme).ok()?;
    if parsed.host_str()? != "localhost" {
        return None;
    }

    let host = match parsed.port() {
        Some(port) => format!("localhost:{port}"),
        None => "localhost".to_string(),
    };

    Some(NavigateDecision::Localhost {
        host,
        path: strip_leading_slash(parsed.path()) + &suffix(&parsed),
    })
}

/// External URL scheme allowlist. Anything outside this set is treated as
/// a [`NavigateDecision::Reject`] so dangerous schemes (`javascript:`,
/// `data:`, `file:`, `vbscript:`, ...) cannot reach `Platform::navigate_to`.
const ALLOWED_EXTERNAL_SCHEMES: &[&str] = &["http", "https", "mailto", "tel", "polkadot", "dot"];

/// Mirrors `normalizeUrl`: prepend `https://` if missing, otherwise pass the
/// URL through as its canonical string form. Returns `Err(reason)` for an
/// unparseable input or a scheme outside [`ALLOWED_EXTERNAL_SCHEMES`].
fn normalize_external(input: &str) -> Result<String, String> {
    // `parse_with_explicit_https` returns a successful direct parse as-is and
    // only prepends `https://` when the direct parse fails, so a disallowed
    // scheme (e.g. `javascript:`) is never rewritten to https: the single
    // scheme check below rejects it.
    let url = parse_with_explicit_https(input)
        .ok_or_else(|| "URL constructor rejected input".to_string())?;
    if !ALLOWED_EXTERNAL_SCHEMES.contains(&url.scheme()) {
        return Err(format!("scheme `{}` is not allowed", url.scheme()));
    }
    Ok(url.to_string())
}

fn strip_leading_slash(path: &str) -> String {
    path.strip_prefix('/').unwrap_or(path).to_string()
}

fn suffix(url: &Url) -> String {
    let mut out = String::new();
    if let Some(q) = url.query() {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = url.fragment() {
        out.push('#');
        out.push_str(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Expected {
        Decision(NavigateDecision),
        AnyExternalOrReject,
        Reject,
    }

    struct TestCase {
        name: &'static str,
        input: &'static str,
        expected: Expected,
    }

    fn dot(identifier: &str, path: &str) -> Expected {
        Expected::Decision(NavigateDecision::DotName {
            identifier: identifier.to_string(),
            path: path.to_string(),
        })
    }

    fn localhost(host: &str, path: &str) -> Expected {
        Expected::Decision(NavigateDecision::Localhost {
            host: host.to_string(),
            path: path.to_string(),
        })
    }

    fn external(url: &str) -> Expected {
        Expected::Decision(NavigateDecision::External {
            url: url.to_string(),
        })
    }

    #[test]
    fn parse_navigate_cases() {
        let cases = vec![
            TestCase {
                name: "dot bare",
                input: "mytestapp.dot",
                expected: dot("mytestapp.dot", ""),
            },
            TestCase {
                name: "dot trailing root dot",
                input: "example.dot.",
                expected: dot("example.dot", ""),
            },
            TestCase {
                name: "dot trailing root dot with path",
                input: "https://example.dot./path",
                expected: dot("example.dot", "path"),
            },
            TestCase {
                name: "dot li is external",
                input: "mytestapp.dot.li",
                expected: external("https://mytestapp.dot.li/"),
            },
            TestCase {
                name: "dot with https",
                input: "https://mytestapp.dot",
                expected: dot("mytestapp.dot", ""),
            },
            TestCase {
                name: "dot with http",
                input: "http://mytestapp.dot",
                expected: dot("mytestapp.dot", ""),
            },
            TestCase {
                name: "dot with path",
                input: "mytestapp.dot/some/path",
                expected: dot("mytestapp.dot", "some/path"),
            },
            TestCase {
                name: "dot with query only",
                input: "pr508.faucet.dot?embed=1",
                expected: dot("pr508.faucet.dot", "?embed=1"),
            },
            TestCase {
                name: "dot with hash only",
                input: "pr508.faucet.dot#section=main",
                expected: dot("pr508.faucet.dot", "#section=main"),
            },
            TestCase {
                name: "dot with path query hash",
                input: "pr508.faucet.dot/nested/path?embed=1#frame=compact",
                expected: dot("pr508.faucet.dot", "nested/path?embed=1#frame=compact"),
            },
            TestCase {
                name: "polkadot scheme dot host",
                input: "polkadot://currenthost.dot/mytestapp.dot",
                expected: dot("currenthost.dot", "mytestapp.dot"),
            },
            TestCase {
                name: "polkadot scheme non dot host falls through",
                input: "polkadot://example.com/settings",
                expected: Expected::AnyExternalOrReject,
            },
            TestCase {
                name: "polkadot scheme with path",
                input: "polkadot://currenthost.dot/mytestapp.dot/settings",
                expected: dot("currenthost.dot", "mytestapp.dot/settings"),
            },
            TestCase {
                name: "polkadot scheme with query and hash",
                input: "polkadot://currenthost.dot/mytestapp.dot?embed=1#frame=compact",
                expected: dot("currenthost.dot", "mytestapp.dot?embed=1#frame=compact"),
            },
            TestCase {
                name: "dot subdomain",
                input: "sub.acme.dot/path",
                expected: dot("sub.acme.dot", "path"),
            },
            TestCase {
                name: "dot mixed case",
                input: "Example.DOT/Path",
                expected: dot("example.dot", "Path"),
            },
            TestCase {
                name: "dot with port is rejected",
                input: "https://x.dot:8080/path",
                expected: Expected::Reject,
            },
            TestCase {
                name: "dot with userinfo is rejected",
                input: "https://user:pass@x.dot/path",
                expected: Expected::Reject,
            },
            TestCase {
                name: "trim whitespace",
                input: "  mytestapp.dot/path  ",
                expected: dot("mytestapp.dot", "path"),
            },
            TestCase {
                name: "localhost bare with port",
                input: "localhost:3000",
                expected: localhost("localhost:3000", ""),
            },
            TestCase {
                name: "localhost with port and path",
                input: "localhost:3000/some/path",
                expected: localhost("localhost:3000", "some/path"),
            },
            TestCase {
                name: "localhost with explicit http",
                input: "http://localhost:5000",
                expected: localhost("localhost:5000", ""),
            },
            TestCase {
                name: "localhost with http and path",
                input: "http://localhost:5000/path",
                expected: localhost("localhost:5000", "path"),
            },
            TestCase {
                name: "localhost with query and hash",
                input: "localhost:3000/path?q=1#h",
                expected: localhost("localhost:3000", "path?q=1#h"),
            },
            TestCase {
                name: "localhost without port",
                input: "localhost",
                expected: localhost("localhost", ""),
            },
            TestCase {
                name: "localhost without port with path",
                input: "localhost/path",
                expected: localhost("localhost", "path"),
            },
            TestCase {
                name: "external bare domain",
                input: "google.com",
                expected: external("https://google.com/"),
            },
            TestCase {
                name: "external bare domain with path",
                input: "google.com/search?q=test",
                expected: external("https://google.com/search?q=test"),
            },
            TestCase {
                name: "external preserves https",
                input: "https://example.com/page",
                expected: external("https://example.com/page"),
            },
            TestCase {
                name: "external preserves http",
                input: "http://example.com/page",
                expected: external("http://example.com/page"),
            },
            TestCase {
                name: "external dot li",
                input: "acme.dot.li/path/1",
                expected: external("https://acme.dot.li/path/1"),
            },
            TestCase {
                name: "reject empty",
                input: "",
                expected: Expected::Reject,
            },
            TestCase {
                name: "reject whitespace",
                input: "   ",
                expected: Expected::Reject,
            },
            TestCase {
                name: "reject unparseable",
                input: ":::invalid",
                expected: Expected::Reject,
            },
            TestCase {
                name: "reject javascript URI",
                input: "javascript:alert(1)",
                expected: Expected::Reject,
            },
            TestCase {
                name: "reject file URI",
                input: "file:///etc/passwd",
                expected: Expected::Reject,
            },
            TestCase {
                name: "reject data URI",
                input: "data:text/html,<script>alert(1)</script>",
                expected: Expected::Reject,
            },
            TestCase {
                name: "reject vbscript URI",
                input: "vbscript:msgbox(1)",
                expected: Expected::Reject,
            },
        ];

        for case in cases {
            let actual = parse_navigate(case.input);
            match case.expected {
                Expected::Decision(expected) => assert_eq!(actual, expected, "{}", case.name),
                Expected::AnyExternalOrReject => assert!(
                    matches!(
                        actual,
                        NavigateDecision::External { .. } | NavigateDecision::Reject { .. }
                    ),
                    "{}: expected External or Reject, got {actual:?}",
                    case.name,
                ),
                Expected::Reject => assert!(
                    matches!(actual, NavigateDecision::Reject { .. }),
                    "{}: expected Reject, got {actual:?}",
                    case.name,
                ),
            }
        }

        let nfc = parse_navigate("café.dot");
        let nfd = parse_navigate("cafe\u{0301}.dot");
        match (&nfc, &nfd) {
            (
                NavigateDecision::DotName {
                    identifier: a,
                    path: _,
                },
                NavigateDecision::DotName {
                    identifier: b,
                    path: _,
                },
            ) => assert_eq!(a, b, "NFC and NFD inputs must normalize to one identifier"),
            other => panic!("expected two DotName decisions, got {other:?}"),
        }
    }
}
