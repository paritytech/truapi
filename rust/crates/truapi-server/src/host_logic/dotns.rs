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
    /// visible so env-aware hosts (e.g. dotli rewriting `.dot` to `.dot.li`)
    /// can re-parse and do their own assembly without losing information.
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

/// Classify a URL the way dotli's `handleNavigateTo` does: try `.dot` first,
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

/// `.dot` TLD check: NFC-normalized and case-folded so `Example.DOT` and
/// `example.dot` collapse to the same outcome.
fn is_dot_domain(host: &str) -> bool {
    let normalized: String = host.nfc().collect::<String>().to_lowercase();
    normalized.ends_with(".dot")
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
        identifier: hostname.nfc().collect::<String>().to_lowercase(),
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
    // `parse_with_explicit_https` first tries direct parse, then prepends
    // `https://`. If the direct parse succeeded but produced a disallowed
    // scheme, reject early so we never silently rewrite (e.g.) `javascript:`
    // into `https://javascript:...`.
    if let Ok(direct) = Url::parse(input)
        && !ALLOWED_EXTERNAL_SCHEMES.contains(&direct.scheme())
    {
        return Err(format!("scheme `{}` is not allowed", direct.scheme()));
    }
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

    fn dot(identifier: &str, path: &str) -> NavigateDecision {
        NavigateDecision::DotName {
            identifier: identifier.to_string(),
            path: path.to_string(),
        }
    }

    fn localhost(host: &str, path: &str) -> NavigateDecision {
        NavigateDecision::Localhost {
            host: host.to_string(),
            path: path.to_string(),
        }
    }

    fn external(url: &str) -> NavigateDecision {
        NavigateDecision::External {
            url: url.to_string(),
        }
    }

    #[test]
    fn dot_bare() {
        assert_eq!(parse_navigate("mytestapp.dot"), dot("mytestapp.dot", ""));
    }

    #[test]
    fn dot_li_is_not_a_product() {
        assert_eq!(
            parse_navigate("mytestapp.dot.li"),
            external("https://mytestapp.dot.li/")
        );
    }

    #[test]
    fn dot_with_https() {
        assert_eq!(
            parse_navigate("https://mytestapp.dot"),
            dot("mytestapp.dot", "")
        );
    }

    #[test]
    fn dot_with_http() {
        assert_eq!(
            parse_navigate("http://mytestapp.dot"),
            dot("mytestapp.dot", "")
        );
    }

    #[test]
    fn dot_with_path() {
        assert_eq!(
            parse_navigate("mytestapp.dot/some/path"),
            dot("mytestapp.dot", "some/path")
        );
    }

    #[test]
    fn dot_with_query_only() {
        assert_eq!(
            parse_navigate("pr508.faucet.dot?embed=1"),
            dot("pr508.faucet.dot", "?embed=1")
        );
    }

    #[test]
    fn dot_with_hash_only() {
        assert_eq!(
            parse_navigate("pr508.faucet.dot#section=main"),
            dot("pr508.faucet.dot", "#section=main")
        );
    }

    #[test]
    fn dot_with_path_query_hash() {
        assert_eq!(
            parse_navigate("pr508.faucet.dot/nested/path?embed=1#frame=compact"),
            dot("pr508.faucet.dot", "nested/path?embed=1#frame=compact")
        );
    }

    #[test]
    fn polkadot_scheme_dot_host() {
        assert_eq!(
            parse_navigate("polkadot://currenthost.dot/mytestapp.dot"),
            dot("currenthost.dot", "mytestapp.dot")
        );
    }

    #[test]
    fn polkadot_scheme_non_dot_host_falls_through() {
        match parse_navigate("polkadot://example.com/settings") {
            NavigateDecision::External { .. } | NavigateDecision::Reject { .. } => {}
            other => panic!("expected External or Reject, got {other:?}"),
        }
    }

    #[test]
    fn polkadot_scheme_with_path() {
        assert_eq!(
            parse_navigate("polkadot://currenthost.dot/mytestapp.dot/settings"),
            dot("currenthost.dot", "mytestapp.dot/settings")
        );
    }

    #[test]
    fn polkadot_scheme_with_query_and_hash() {
        assert_eq!(
            parse_navigate("polkadot://currenthost.dot/mytestapp.dot?embed=1#frame=compact"),
            dot("currenthost.dot", "mytestapp.dot?embed=1#frame=compact")
        );
    }

    #[test]
    fn dot_subdomain() {
        assert_eq!(
            parse_navigate("sub.acme.dot/path"),
            dot("sub.acme.dot", "path")
        );
    }

    #[test]
    fn dot_with_mixed_case_normalizes() {
        assert_eq!(
            parse_navigate("Example.DOT/Path"),
            dot("example.dot", "Path")
        );
    }

    #[test]
    fn dot_with_port_is_rejected() {
        assert!(matches!(
            parse_navigate("https://x.dot:8080/path"),
            NavigateDecision::Reject { .. }
        ));
    }

    #[test]
    fn dot_with_userinfo_is_rejected() {
        assert!(matches!(
            parse_navigate("https://user:pass@x.dot/path"),
            NavigateDecision::Reject { .. }
        ));
    }

    #[test]
    fn trim_whitespace() {
        assert_eq!(
            parse_navigate("  mytestapp.dot/path  "),
            dot("mytestapp.dot", "path")
        );
    }

    #[test]
    fn localhost_bare_with_port() {
        assert_eq!(
            parse_navigate("localhost:3000"),
            localhost("localhost:3000", "")
        );
    }

    #[test]
    fn localhost_with_port_and_path() {
        assert_eq!(
            parse_navigate("localhost:3000/some/path"),
            localhost("localhost:3000", "some/path")
        );
    }

    #[test]
    fn localhost_with_explicit_http() {
        assert_eq!(
            parse_navigate("http://localhost:5000"),
            localhost("localhost:5000", "")
        );
    }

    #[test]
    fn localhost_with_http_and_path() {
        assert_eq!(
            parse_navigate("http://localhost:5000/path"),
            localhost("localhost:5000", "path")
        );
    }

    #[test]
    fn localhost_with_query_and_hash() {
        assert_eq!(
            parse_navigate("localhost:3000/path?q=1#h"),
            localhost("localhost:3000", "path?q=1#h")
        );
    }

    #[test]
    fn localhost_without_port() {
        assert_eq!(parse_navigate("localhost"), localhost("localhost", ""));
    }

    #[test]
    fn localhost_without_port_with_path() {
        assert_eq!(
            parse_navigate("localhost/path"),
            localhost("localhost", "path")
        );
    }

    #[test]
    fn external_bare_domain() {
        assert_eq!(
            parse_navigate("google.com"),
            external("https://google.com/")
        );
    }

    #[test]
    fn external_bare_domain_with_path() {
        assert_eq!(
            parse_navigate("google.com/search?q=test"),
            external("https://google.com/search?q=test")
        );
    }

    #[test]
    fn external_preserves_https() {
        assert_eq!(
            parse_navigate("https://example.com/page"),
            external("https://example.com/page")
        );
    }

    #[test]
    fn external_preserves_http() {
        assert_eq!(
            parse_navigate("http://example.com/page"),
            external("http://example.com/page")
        );
    }

    #[test]
    fn external_dot_li() {
        assert_eq!(
            parse_navigate("acme.dot.li/path/1"),
            external("https://acme.dot.li/path/1")
        );
    }

    #[test]
    fn reject_empty() {
        assert!(matches!(
            parse_navigate(""),
            NavigateDecision::Reject { .. }
        ));
    }

    #[test]
    fn reject_whitespace() {
        assert!(matches!(
            parse_navigate("   "),
            NavigateDecision::Reject { .. }
        ));
    }

    #[test]
    fn reject_unparseable() {
        assert!(matches!(
            parse_navigate(":::invalid"),
            NavigateDecision::Reject { .. }
        ));
    }

    /// `javascript:` URIs must never reach the platform's `navigate_to`;
    /// otherwise a malicious product could execute arbitrary JS in the host.
    #[test]
    fn reject_javascript_uri() {
        assert!(
            matches!(
                parse_navigate("javascript:alert(1)"),
                NavigateDecision::Reject { .. }
            ),
            "javascript: scheme must be rejected"
        );
    }

    /// `file:` URIs leak local filesystem paths; reject them.
    #[test]
    fn reject_file_uri() {
        assert!(
            matches!(
                parse_navigate("file:///etc/passwd"),
                NavigateDecision::Reject { .. }
            ),
            "file: scheme must be rejected"
        );
    }

    /// `data:` URIs can carry inline HTML/JS payloads; reject them.
    #[test]
    fn reject_data_uri() {
        assert!(
            matches!(
                parse_navigate("data:text/html,<script>alert(1)</script>"),
                NavigateDecision::Reject { .. }
            ),
            "data: scheme must be rejected"
        );
    }

    /// `vbscript:` URIs are the legacy IE equivalent of `javascript:`;
    /// reject them too even though modern browsers don't execute them.
    #[test]
    fn reject_vbscript_uri() {
        assert!(
            matches!(
                parse_navigate("vbscript:msgbox(1)"),
                NavigateDecision::Reject { .. }
            ),
            "vbscript: scheme must be rejected"
        );
    }

    /// NFC-normalized and NFD-normalized inputs that represent the same
    /// dotns name must produce the same `DotName.identifier` so downstream
    /// resolution can't be fooled into looking up two different lookup keys
    /// for one visual identity.
    #[test]
    fn nfc_normalization_collapses_nfd() {
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
