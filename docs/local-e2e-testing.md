# Local end-to-end testing playbook

A prescriptive checklist for verifying a change to the TrUAPI protocol
(Rust traits, versioned wrappers, codegen, TS client, or playground) end to
end on a single workstation. Written for both humans and agents — every
step has an exact command, an expected outcome, and the failure mode that
would prompt a re-run or rollback.

## Automation

The chain below is also automated:

- **Claude skills** under `.claude/skills/` mirror each layer:
  `rust-checks`, `regen-codegen`, `ts-client-checks`,
  `refresh-playground-snapshot`, `playground-checks`, `e2e-dotli`, and
  the umbrella `truapi-definition-of-done`. Invoke them when working in
  the repo with Claude Code; each is a small, command-first runbook.
- **CI workflow** `.github/workflows/ci.yml` runs the same chain on every
  PR. The static jobs (`rust`, `codegen-drift`, `ts-client`,
  `playground`) are fast; the `e2e` job builds dotli and
  drives the playground inside its iframe via Playwright (specs in
  `playground/tests/e2e/`). Failed e2e runs upload the Playwright HTML
  report as an artifact.

The doc below is still the canonical narrative and the source of truth
for failure modes — both the skills and CI cite it.

`make e2e-dotli` is the end-to-end dotli + playground diagnosis harness. It
starts the local dotli preview and playground, opens Chromium, signs out any
restored host session, signs in through the signer-bot SSO service, runs the
playground Diagnosis screen, and writes
`playground/test-results/e2e-dotli/diagnosis-report.md`. Full automation
requires `SIGNER_BOT_SVC_TOKEN`; `SIGNER_BOT_BASE_URL` and
`SIGNER_BOT_NETWORK` default to dotli CI's signer-bot service and
`paseo-next-v2`. Without the token, use
`E2E_DOTLI_SMOKE=1 make e2e-dotli` to verify the local stack, browser launch,
login click, TrUAPI debug logs, and QR/deeplink extraction without a phone.
Ports 5173 and 3000 must be free so the diagnosis runner can start both apps.
In root CI, the job also needs `DOTLI_CHECKOUT_TOKEN` to read the private
dotli submodule. Without dotli access it reports a warning and skips the e2e
job; with dotli access but without `SIGNER_BOT_SVC_TOKEN`, it runs the smoke
path only.

The order matters: each layer assumes the layer below it builds clean.
Skip a step only if you are certain the change cannot affect that layer.

```
Rust crates  →  codegen  →  @parity/truapi  →  playground  →  dotli iframe
```

## 0. Pre-flight

```bash
# from repo root
git submodule update --init --recursive
( cd js/packages/truapi && npm install )
( cd playground && yarn install --frozen-lockfile )
( cd hosts/dotli && bun install )
```

Failure modes:

- `hosts/dotli/` empty → submodule wasn't initialised.
- `playground/node_modules/@parity/truapi` missing after `yarn install` →
  the `file:` snapshot didn't materialise; see [Gotchas](#gotchas).
- `bun: command not found` → install Bun (`curl -fsSL https://bun.sh/install | bash`).

## 1. Rust workspace static checks

```bash
cargo build --workspace --all-targets --all-features
cargo +nightly fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Each command must finish with `Finished` / `test result: ok`. Treat any
warning as a failure (clippy is `-D warnings` already; build warnings are
not).

If you only touched `truapi` types/traits, scoping to that crate is fine
during iteration:

```bash
cargo build -p truapi --all-features
cargo test  -p truapi --all-features
```

Always run the workspace-wide commands once before declaring done — the
codegen and macro crates depend on `truapi`.

## 2. Regenerate the TypeScript client (only if Rust trait surface changed)

`scripts/codegen.sh` rebuilds the ignored generated TS under `js/packages/truapi/src/generated/`
(`client.ts`, `types.ts`, `wire-table.ts`) from the rustdoc JSON of
`truapi`. Skip this step if your change is purely Rust-internal (e.g.
versioned wrapper conversion logic that doesn't change rustdoc output) or
purely TS-side.

```bash
./scripts/codegen.sh
```

Expected: `Generated client at js/packages/truapi/src/generated/`. The
script uses `cargo +nightly rustdoc --output-format json` so a missing
nightly toolchain or broken intra-doc links will fail it. Fix doc links
that the rustdoc step warns about — they break codegen and look worse
in published docs.

After regenerating, rebuild the package or run the package tests so the
ignored outputs are exercised.

## 3. `@parity/truapi` build + smoke tests

```bash
cd js/packages/truapi
npm run build
npm test
```

Expected:

- `tsc` (the `build` step) exits cleanly with no diagnostics.
- `bun test` reports `N pass` / `0 fail` across the `src/**/*.test.ts` files.
- `src/wire-table.test.ts` emits one `round-trips <method>.<kind>` case per
  generated frame id; the count tracks `WIRE_TABLE`. When you add a method,
  it grows by 2 (request + response) or 4 (subscribe).

`tsc` errors here usually mean the codegen was skipped or out of sync.
If a wire-equality test fails (golden hex mismatch) the wire format
changed — that is a wire-breaking change, not a regression to "fix" by
tweaking the test.

## 4. Refresh the playground's `@parity/truapi` snapshot

yarn 1.x copies `file:` deps at install time, so `playground/node_modules/@parity/truapi`
is a _snapshot_ of the package state when `yarn install` last ran. Any
change to `js/packages/truapi/` (codegen, hand-written transport,
package.json) requires:

```bash
cd playground
rm -rf node_modules
yarn install
```

A `rm -rf node_modules/@parity` followed by `yarn install` is _not_
sufficient when yarn already considers the lockfile satisfied — it'll
say `success Already up-to-date` and leave the directory missing. Always
nuke the whole `node_modules` (it's a few seconds to repopulate) when
the snapshot is stale.

You can tell the snapshot is stale when `playground/node_modules/@parity/truapi/dist/generated/client.d.ts`
disagrees with `js/packages/truapi/dist/generated/client.d.ts`.

## 5. Playground build + lint

```bash
cd playground
yarn build
yarn lint
```

`yarn build` runs the Next.js static export and a strict `tsc` pass over
the playground sources. Type errors here typically mean the bridge
(`src/lib/host-api-bridge.ts`, `src/lib/transport.ts`) is calling the
generated client with the wrong shape. The fix is at the bridge call
site, never in the generated files.

`yarn lint` is ESLint and should print `No ESLint warnings or errors`.

## 6. End-to-end inside dotli

The static checks above don't exercise the wire protocol. To do that
locally, run the playground inside dotli's host iframe and drive each
method from the UI.

### Start dotli's preview server

```bash
cd hosts/dotli
bun run preview            # → http://localhost:5173
```

When investigating a wire issue, raise the Rust core's log level from the
host origin. The WASM worker bridge forwards core `tracing` output to the
browser console, mapping each level to the matching `console` method:

```js
window.__truapi.setLogLevel("trace");
```

`debug` and `trace` are emitted via `console.debug`, which Chrome hides
unless the console **Default levels ▾** dropdown includes **Verbose**;
`info`/`warn`/`error` always render.

### Start the playground dev server

```bash
cd playground
yarn dev                   # → http://localhost:3000
```

### Open the playground inside dotli

Navigate (in any browser) to:

```
http://localhost:5173/localhost:3000
```

dotli's host parses `/localhost:<port>` as a proxy directive and iframes
the playground at `http://localhost:3000`. The playground detects the
iframe via `window.parent` and uses the iframe `postMessage` provider.

### Verification flow inside the playground UI

1. The connection chip should flip from _Offline_ to _Handshaking_ and
   then to _Host Linked_ within ~1s. `Host Linked` proves the handshake
   round trip (`host_handshake_request` → `host_handshake_response`)
   decoded on both ends.
2. Open `Account Management → host_account_get` (or any unary method),
   keep the default request, and click **Call**. A success result with
   a public key proves: SCALE encode in TS → wire frame → dotli decode →
   versioned-wrapper unwrap → host handler → versioned-wrapper wrap →
   wire frame → SCALE decode in TS → neverthrow `Result.isOk()`.
3. Open a subscription (e.g. `Account Management → host_account_connection_status_subscribe`)
   and click **Subscribe**. You should immediately see one or more
   pushed events; clicking **Unsubscribe** must stop them. This proves
   the `_start` / `_receive` / `_stop` lifecycle.
4. For chain methods, open `Chain Interaction → remote_chain_head_follow_subscribe`
   and subscribe. The bridge auto-detects dependent methods (header,
   body, storage, call, unpin, continue, stop_operation) and opens an
   ephemeral follow when `followSubscriptionId` is empty — exercising
   one is enough to validate the auto-follow path.
5. If you changed a versioned wrapper, exercise at least one V1-only
   method (e.g. `host_account_get`) and one V0.2-only method (e.g.
   `host_get_user_id`) to confirm both wire variants still decode.

If the connection chip stays on _Handshaking_, the handshake is
failing. Check:

- The dotli console for `Unknown wire tag` / `Unknown wire discriminant`
  errors — wire-table mismatch between the dotli vendored copy of
  `@parity/truapi` and the just-built one.
- The playground console for `decodeWireMessage` errors — the inbound
  frame's discriminant is unknown (the playground's wire-table is
  stale; redo step 4).

If a method call hangs, the host either didn't receive the frame
(check dotli's console with `truapi:logLevel` set to `debug`) or didn't respond.
The bridge auto-responds to `host_handshake_request` only; everything
else is on the host implementation.

## 7. Codegen tests

If you changed `truapi-codegen` itself, also run its self-tests:

```bash
cargo test -p truapi-codegen --all-features
```

The wire-table generator has property tests (sorted, no duplicates,
well-formed for empty input) plus targeted regression tests for the
`detect_versioned_wrapper` heuristic.

## Gotchas

### yarn 1.x `file:` dep stale snapshot

Symptom: `playground` builds against the _old_ shape of `@parity/truapi`
even after rebuilding it. Or: webpack reports
`Can't resolve '@parity/truapi'` after a partial rebuild.

Cause: `playground/node_modules/@parity/truapi` is a snapshot copied at
install time. Yarn caches the install result, so a re-`yarn install`
without changes is a no-op and won't refresh the snapshot.

Fix: `rm -rf playground/node_modules && yarn install` (full nuke).

### rust-analyzer stale diagnostics

Symptom: rust-analyzer flags `unresolved import super::Versioned` (or
similar) on files I just rewrote, but `cargo build` succeeds.

Cause: rust-analyzer indexed an earlier state.

Fix: ignore the diagnostic if `cargo build/clippy` are both clean. The
authoritative source is `cargo`, not the editor squiggle.

### Broken intra-doc links break codegen

Symptom: `cargo +nightly rustdoc -p truapi` emits
`unresolved link to ...` warnings, then `truapi-codegen` produces
output but you missed an item in the generated TS.

Fix: turn the link into a fully-qualified path (`super::T`,
`crate::vXY::T`, or just drop the link to a sibling that won't resolve
across the doc-namespace boundary). Re-run `./scripts/codegen.sh`.

### Payloadless request has no payload arm

Symptom: codegen omits a payload arm for a request enum like
`HostGetUserIdRequest`. The wire-table loop test passes a smaller `<N>`
than expected.

This is intentional. A request that takes no arguments declares a
payloadless `V1` variant (`pub enum HostGetUserIdRequest { V1 }`), so
there is no inner type for codegen to emit.

## Definition of done

A change is end-to-end-verified locally when all of:

- [ ] `cargo build/test/clippy --workspace --all-targets --all-features` clean
- [ ] `cargo +nightly fmt --check` clean
- [ ] `./scripts/codegen.sh` clean (only if Rust surface changed)
- [ ] `npm run build && npm test` in `js/packages/truapi/` clean
- [ ] `yarn build && yarn lint` in `playground/` clean (after a fresh
      `rm -rf node_modules && yarn install` if step 2 ran)
- [ ] Playground loads inside `http://localhost:5173/localhost:3000`,
      connection chip turns _Host Linked_, at least one unary call and one
      subscription succeed against the dotli host.
