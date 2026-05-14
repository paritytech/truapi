# `@parity/truapi-host`

Host-side dispatcher and generated typed handler interfaces for TrUAPI.

Sibling to [`@parity/truapi`](../truapi/). Both packages are generated from
the same rustdoc JSON (`scripts/codegen.sh`), so the host and client share
identical wire ids, codecs, and type definitions.

## What it provides

- **Typed handler interfaces**, one per service trait (`AccountHostHandlers`,
  `ChainHostHandlers`, …). Each request method takes the inner request
  payload and returns `Result<Ok, Err>`; each subscription method receives a
  `SubscriptionSink<Item>` plus a `CallContext`.
- **`createTrUApiServer(provider, handlers)`** — attaches a host server to a
  `Provider` from `@parity/truapi`. Routes inbound request and subscription
  frames to the supplied handlers, handles versioned wrapper encode/decode,
  and forwards responses/items/interrupts as wire frames.
- **Hand-written server core** (`server-core.ts`) that owns the dispatch
  table, subscription state map, and provider plumbing.

## Out of scope

The package exposes 1:1 wire primitives. Subscription multiplexing,
deduplication, buffering, replay semantics, and connection-status policy are
not in scope, products and hosts layer their own policy if needed.

## Codegen

This package depends on generated source in `src/generated/`. Run
`./scripts/codegen.sh` at the repo root to refresh both packages from the
Rust trait surface.
