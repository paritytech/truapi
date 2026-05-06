# Review TODO 2 — PR #30

Findings from the second pass of review on the worktree-integrate-truapi-next branch.

Local checks pass: `cargo build/clippy/test --workspace`, `cargo fmt --check`, `truapi` build + tests (117 wire pairs, 6 wire-equality tests), playground build + lint.

## Findings

| # | Severity | File | Line | Description |
|---|----------|------|------|-------------|
| 1 | High | js/packages/truapi/src/client.ts | 138 | `_receive` calls `decodePayload` outside any try/catch, so a single malformed subscription frame propagates up the provider's `subscribe` callback and breaks the dispatcher for every other consumer. Wrap in try/catch and route to `onInterrupt`/log. |
| 2 | High | js/packages/truapi/src/client.ts | 143-150 | `_interrupt` deletes the subscription, then throws if `interruptCodec` is missing. The throw escapes into the provider dispatch loop and tears it down. Either require `interruptCodec` whenever `onInterrupt` is set, or swallow + log. |
| 3 | High | playground/src/lib/transport.ts | 76 | `parent.postMessage(message, '*', [message.buffer])` uses a wildcard target origin. Any parent that loads the playground iframe can read frames containing signed payloads and account data. Pin to the host's origin (or document the trust model in CLAUDE.md if intentional). |
| 4 | Medium | js/packages/truapi/src/client.ts | 110, 231 | `closeWithError` and the early-return `subscribe` path coerce a JS `Error` into the `Interrupt` channel via `data as Interrupt`. Callers that decode Interrupt-shaped values get a runtime type mismatch. Consider a separate `onClose`/`onError` channel. |
| 5 | Medium | rust/crates/truapi/src/traits/{account,signing,statement_store,...}.rs | various | Default impls record `cx.fail_unavailable()` then return forged placeholders (`Vec::new()`, `[0u8; 32]`, `String::new()`). User policy: platform hosts return `unavailable`, never forge fixture values in Rust. The dispatcher discards them via `take_failure`, but the values still live in source. Prefer `unimplemented!()` or removing default bodies. |
| 6 | Medium | playground/next.config.js | 14-16 | `eslint: { ignoreDuringBuilds: true }` silently disables eslint in the deploy workflow's `yarn build` step. Either remove this and keep `next lint`, or migrate to `next lint --strict` invoked separately in CI. |
| 7 | Medium | js/packages/truapi/package.json | 12 | `npm test` shells out to `bun`. Contributors without bun installed get a confusing failure. Document the requirement in README or migrate to `node --test`. |
| 8 | Medium | js/packages/truapi/src/index.ts | 15-19 | `Subscription` and `TrUApiTransport` are exported both from hand-written `client.ts` and via `export * from './generated/index.js'`. Build passes because they're structurally identical, but a future divergence will become an import collision. Pick one source. |
| 9 | Medium | scripts/codegen.sh | 16-19 | Regenerates `src/generated/*` but doesn't rebuild `dist/`. CLAUDE.md tells the user to run `npm run build` afterward, fold it into the script so the toolchain is one command. |
| 10 | Medium | rust/crates/truapi-codegen/src/main.rs | 27-40 | `main` returns `anyhow::Result`, so a missing input file prints the default `Error: ...` debug. Add a `.context("reading {path}")` for friendlier diagnostics. |
| 11 | Low | playground/src/app/page/page.tsx | — | The route `/page` (a "navigation_test" diagnostics page) reads as a typo. Confirm intentional or rename to `/diagnostics` or `/navtest`. |
| 12 | Low | .gitmodules | 4 | Submodule URL is SSH (`git@github.com:paritytech/dotli`). Not used by the deploy workflow, but a fresh clone over HTTPS won't authenticate. Use HTTPS or document the SSH requirement. |
| 13 | Low | js/packages/truapi/package.json | — | Missing `repository`, `license`, `author`, `files` fields. Fine if internal, blocking before publishing. |
| 14 | Low | rust/crates/truapi-codegen/src/typescript.rs | 16-32 | `VersionedWrapper` doc explains "earliest version" picking but the field comment on `variant` says "expects on inbound responses" with the response unwrap path using `value.value` directly. Doc-only, no behavior bug. |
| 15 | Info | rust/crates/truapi/src/traits/* | — | Doc-coverage on every `pub` item passes by spot check; format-string inlining is consistent. |

## Verdict

**Useful, but needs the High-severity items addressed.** Items 1-3 are real correctness/security issues. Items 4-10 are quality bars worth fixing now since this PR is the foundational integration. Items 11-15 are nits.
