---
title: "Host Identity and Version via System.host_info"
owner: "@valentinfernandez1"
---

# RFC — Host Identity and Version via `System.host_info`

|                 |                                                                                          |
| --------------- | ---------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-06-03                                                                               |
| **Description** | Add a `host_info` method to the `System` trait so products can identify which host, and which version of it, is running them. |
| **Authors**     | Valentin Fernandez                                                                       |

## Summary

Add an always-available `System::host_info` method that returns the host's
platform, name, and version. This gives a product first-class knowledge of
which host — and which build of it — is running it, so it can adapt to the
host, report telemetry, and attribute behaviour to a concrete implementation
and version.

## Motivation

Products today cannot tell which host is running them, nor which version. The
`System` trait exposes only `handshake`, `feature_supported`, and
`navigate_to`; `feature_supported` answers a single narrow question — whether
the host can interact with a chain identified by genesis hash — and the
handshake response carries only the negotiated codec version (a bare
`HostHandshakeResponse::V1` unit). No host identity or version crosses the
wire, and there is no window global that carries it either. A product runs
blind to its own environment.

Knowing the host is useful across several situations:

- **Adapting to the host.** The platform (web iframe vs. desktop vs. mobile)
  legitimately shapes presentation and behaviour — layout density, available
  affordances, how the product talks about itself.
- **Diagnostics and bug attribution.** When behaviour differs between host
  builds, a product, a log, or a bug ticket can name the exact host and
  version instead of guessing.
- **Telemetry and support.** Aggregating which hosts and versions are in the
  field, and reproducing a user's report against the build they actually ran.

The diagnostics case is the most concrete today. The playground's Diagnosis
screen runs every TrUAPI method against the connected host and emits a
per-host compatibility report, which the explorer aggregates into a cross-host
matrix. The report can only label a run by an inferred *mode* — `Web`,
`Desktop`, `Android`, `iOS`, or `Unknown` — derived heuristically from
`navigator.userAgent` and a `__HOST_WEBVIEW_MARK__` flag. When a method passes
on one host build and fails on another (for example a statement-store
submission rejected by one Desktop build), the report cannot say *which
build*. A host that names itself and its version closes that gap: a report
row, an error log, or a bug ticket can record "Polkadot Desktop 1.4.2" or
"dotli 0.9.0" instead of just "Desktop".

## Detailed Design

### Trait method

Add to the `System` trait (`rust/crates/truapi/src/api/system.rs`):

```rust
/// Report the host's identity and version.
#[wire(request_id = 164)]
async fn host_info(
    &self,
    cx: &CallContext,
    request: HostInfoRequest,
) -> Result<HostInfoResponse, CallError<HostInfoError>>;
```

`host_info` is an always-available method in the sense of RFC 0009: like
`handshake`, `feature_supported`, and `navigate_to`, it depends on no user
identity and must work before authentication. The request carries no fields;
it exists only to match the versioned request/response/error shape every other
`System` method follows.

### Payload types

Add to `rust/crates/truapi/src/v01/system.rs`:

```rust
/// Platform category a host runs on.
pub enum HostPlatform {
    /// Browser-embedded product (an iframe inside a web host).
    Web,
    /// Android application.
    Android,
    /// iOS application.
    Ios,
    /// Desktop application.
    Desktop,
    /// Host could not classify its platform.
    Unknown,
}

/// Identity and version of the host currently running the product.
pub struct HostInfo {
    /// Platform category the host runs on.
    pub platform: HostPlatform,
    /// Human-readable name of the host implementation, e.g. "Polkadot
    /// Desktop", "Polkadot Mobile", or "dotli". Hosts should report a stable,
    /// non-empty name.
    pub name: String,
    /// Host-native version string, e.g. a semver such as "1.2.3". Hosts should
    /// report a non-empty value; the format is the host's own.
    pub version: String,
}
```

The three fields are deliberately flat. `platform` is a closed enum because the
set of platforms is small and stable, and products already reason in exactly
those terms (the playground's host-mode union is literally
`"Web" | "Desktop" | "Android" | "iOS" | "Unknown"`). `name` and `version` are
free-form strings because the set of hosts and their versioning schemes evolve
independently of the protocol — a closed enum of host names would force a
protocol revision every time a new host or fork appeared. The triple
(platform, name, version) is enough to pin down exactly what is running.

### Versioned wrappers

Add to `rust/crates/truapi/src/versioned/system.rs`:

```rust
pub enum HostInfoRequest { V1 }
pub enum HostInfoResponse { V1 => v01::HostInfo }
pub enum HostInfoError { V1 => v01::GenericError }
```

The error is the `GenericError` catch-all, matching `feature_supported`; the
call is host-local introspection with no domain-specific failure modes.

### Generated client

`./scripts/codegen.sh` regenerates the TypeScript client and playground
metadata from the trait's rustdoc. The codegen strips the conventional `host_`
prefix from method names, so `host_info` projects to `truapi.system.info()`
(the wire constant keeps the full `SYSTEM_HOST_INFO`). `HostInfo` becomes an
object `{ platform, name, version }`, and the fieldless `HostPlatform` enum
becomes the string union `"Web" | "Android" | "Ios" | "Desktop" | "Unknown"`.

## Alternatives

- **Extend the handshake response.** A `HostHandshakeResponse::V2` variant could
  carry host info, delivering it at connection time with no extra round trip.
  Rejected: it couples host identity to codec negotiation, is harder to evolve
  than a dedicated versioned type, and forces every product through the
  handshake path to read a value many will not need.
- **Window globals (`__HOST_VERSION__`, `__HOST_NAME__`).** Hosts could inject
  globals the product reads directly. Rejected: it bypasses the typed protocol
  and codegen, is invisible to non-web transports, and offers no versioning or
  schema.
- **Structured app-plus-components model.** A richer `HostInfo { name, version,
  components: Vec<{ name, version }> }` could report the app *and* its embedded
  runtime (e.g. Polkadot Desktop *and* dotli) simultaneously. Rejected for now
  as more than current consumers need: the directly-running host names itself,
  and the flat triple already identifies it unambiguously. The components model
  is noted under Future Directions.

## Compatibility

`host_info` is a new method, so hosts that predate it return the standard
unavailable/unimplemented call error rather than a `HostInfo`. Products must
treat the call as best-effort: on error, fall back to the existing
user-agent-derived platform mode and an unknown version. The playground's
Diagnosis report will do exactly this — populate the report header from
`host_info` when available, and degrade to today's heuristic mode otherwise.

The generated `HostPlatform` string union uses `Ios`, whereas the playground's
existing `HostMode` type uses the literal `iOS`. The playground will reconcile
its local type to the generated `Ios` variant when it adopts `host_info`.

## Future Directions

If a need arises to report an app and its embedded runtime separately (for
instance distinguishing the Polkadot Desktop version from the dotli version it
embeds), `HostInfo` can gain an optional `components: Vec<HostComponent>` field
in a later version without disturbing the flat triple. This RFC intentionally
does not add it until a concrete consumer needs it.

A natural follow-up, outside this RFC's protocol scope, is the playground
Diagnosis report consuming `host_info` to stamp each report with the host
name and version, and the explorer's compatibility matrix keying rows on them.

## Unresolved Questions

- Should the RFC recommend a canonical registry of `name` values (e.g.
  `"Polkadot Desktop"`, `"Polkadot Mobile"`, `"dotli"`) so reports group
  cleanly, or leave naming entirely to hosts? A loose convention may be enough.
- Is a recommended `version` format (semver) worth stating, given hosts may not
  all follow semver?
