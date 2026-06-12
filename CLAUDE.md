# CLAUDE.md

Guidance for Claude Code when working in this repository.

This repo is the single source of truth for the TrUAPI protocol. It vendors `dotli` as a git submodule at `hosts/dotli/`.

## Layout

```
rust/crates/
  truapi/                Rust trait + type definitions for protocol versions v0.1 and v0.2 (canonical)
  truapi-codegen/        rustdoc JSON → TypeScript client + Rust dispatcher
  truapi-macros/         #[wire(id = N)] proc-macro
  truapi-platform/       Host syscall traits (storage, navigation, consent, ...)
  truapi-server/         Rust runtime hosts implement; ships as WASM (browser/node)
js/packages/
  truapi/                  @parity/truapi TS package; generated TS lives under ignored paths
  truapi-host/             @parity/truapi-host host-side codegen + dispatcher (no shared core)
  truapi-host-wasm/        @parity/truapi-host-wasm: WASM-backed host runtime. Subpath entries:
	                           `.` (core Provider + dispatcher), `/web` (iframe + Web
	                           Worker), `/electron` (MessagePortMain), `/worker-runtime` (Worker entry).
	                           WASM bundle (gitignored) under dist/wasm/web/, built via `make wasm`
playground/                Next.js interactive playground; deploys to truapi-playground.dot
hosts/dotli/               dotli submodule
docs/                      design docs, RFCs, feature proposals
scripts/codegen.sh         regenerate the TS client from the Rust crate
```

### Crate + binding invariants

- `truapi` is canonical; runtime crates re-export rather than redefine. New
  syscall traits and host-side runtime types live in `truapi-platform` and
  `truapi-server`, not in `truapi`. Any additions to `truapi` itself are limited
  to additive `Display` impls.
- All types exposed by `truapi-platform` and `truapi-server` come from
  `truapi::versioned::*` and `truapi::v01::*`. The runtime crates re-export
  rather than redefine.
- `truapi-server` WASM artifacts live under
  `js/packages/truapi-host-wasm/dist/wasm/web/` and are gitignored.
  Build them locally with `make wasm` (rerun whenever
  `rust/crates/truapi-server/` changes); CI builds the bundle fresh from the
  Rust source on every run.

## Code style

- Every `pub` Rust item (functions, methods, types, traits, modules, constants) carries a doc comment (`///` or `//!`).
  Keep it short and focused on intent or invariants, not on what the signature already says.
- Do not add code comments or doc comments that narrate migrations, compatibility shims, or historical changes. Comments should describe only the current code.
- Remove legacy compatibility code by default. Keep or add it only when explicitly requested.
- In Rust format strings, prefer inlined variables: `"log value: {value:?}"` over `"log value: {:?}", value`.
- **No `any` in TypeScript types**: If a type can't be expressed cleanly, stop and ask the user whether to (a) refactor or import the right type or (b) add a scoped `// eslint-disable-next-line @typescript-eslint/no-explicit-any` exception. Never silently leave `any`.
- Don't introduce typealias chains that just rename a public type from another crate (e.g. `pub type StorageError = crate::v01::HostLocalStorageReadError`). Use the canonical name directly. A typealias is only worth its indirection when it captures a real abstraction.
- After any code change, update `README.md` (and CLAUDE.md if the layout changed) so the top-level docs reflect what the repo actually contains. Stale docs are a regression.
- In codegen emitters, prefer `indoc::writedoc!` / `formatdoc!` over chains of `writeln!`. A single `writedoc!` with a multi-line raw string keeps the emitted shape visible in source instead of fragmenting it across one-line `writeln!` calls. Reserve `writeln!` for the genuinely-one-line case (a single import, a single statement inside a loop).
- In PR descriptions, issue comments, and other artifacts that outlive the conversation: describe the resulting state, not the transition between commits. Avoid "previously X, now Y", "we removed", "the old shim is gone", "this PR replaces", those read as ephemeral history once the PR is squash-merged. Write what the system _does_ after the change, not what each commit _changed_ on the way there. (Commit messages are the place for transition narrative; they survive in `git log` even after the squash.)

## First-time setup

```bash
# Check out the dotli submodule
git submodule update --init --recursive

# Build the TypeScript client (triggers tsc via `prepare`)
( cd js/packages/truapi && npm install )

# Install playground dependencies (picks up @parity/truapi via the file: link)
( cd playground && yarn install --frozen-lockfile )
```

## Regenerating the TS client

When the Rust trait surface changes, rerun:

```bash
./scripts/codegen.sh
```

That will repopulate the ignored generated TS under `js/packages/truapi/src/generated/`,
`js/packages/truapi/src/playground/codegen/`, and `js/packages/truapi/test/generated/examples/`.
After regenerating, rebuild the client and refresh the playground's link copy:

```bash
( cd js/packages/truapi && npm run build )
( cd playground && rm -rf node_modules/@parity && yarn install )
```

(yarn 1.x copies `file:` deps at install time, so the playground's `node_modules/@parity/truapi` is a snapshot.)

## Local development

### Rust

```bash
cargo build --workspace
cargo +nightly fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

### TypeScript client

```bash
cd js/packages/truapi
npm run build
npm test                # wire-equality + wire-table-loop smoke tests
```

### Explorer

The explorer is a standalone Vite/React site (no host needed). To run it
locally, just start its own dev server and open the URL directly in a browser.
**Do not** launch dotli for the explorer.

```bash
cd explorer
npx vite --base / --port 5181   # standalone site at http://localhost:5181/
npm run build                    # static export to dist/
```

Use a port other than 5173 (dotli's conventional port) to avoid stale-tab
confusion.

### Playground

```bash
cd playground
yarn dev                # Next.js dev server on :3000
yarn build              # static export to out/
yarn lint
```

The playground must be opened from inside a TrUAPI host. The fastest local
setup is to run dotli's preview server alongside the playground and open
`http://localhost:5173/localhost:3000` in any browser. Use the
[`playground-local-stack`](.claude/skills/playground-local-stack/SKILL.md)
skill to bring both servers up in tmux (it handles the `hosts/dotli/`
submodule init + `bun install` and the per-pane `cd` discipline).
Alternatively, with a deployed Polkadot Desktop Host installed, navigate to
`https://dot.li/localhost:3000` from within it.

#### Local dotli + playground E2E notes

Use `make dev DEBUG=1` from the repo root for the local host stack. It prepares
the ignored WASM/build artifacts, verifies dotli can resolve
`@parity/truapi-host-wasm`, then starts dotli on `:5173` and the playground on
`:3000`. Open `http://localhost:5173/localhost:3000`.

When automating with Playwright, block service workers for smoke tests unless
the test is explicitly about SW behavior. Stale host/product bundles can mask
runtime fixes. Use a fresh cache-busting query string on
`http://localhost:5173/localhost:3000?...`, collect `pageerror` and
`console` messages, and fail on unexpected page errors.

For interactive SSO checks, prefer a persistent headed Chrome profile and reuse
the same browser context across checks. SSO pairing needs a real phone QR scan,
and signing/resource-allocation flows may need web or mobile confirmation; if
the human or companion app is unavailable, skip those methods and record the
skip instead of treating it as a protocol failure. Non-interactive checks should
still verify that the playground renders, the TrUAPI debug panel receives
host/product events, generated examples can call non-confirmation methods, and
logout/relogin does not restore a stale session.

The dotli Playwright e2e suite under `hosts/dotli/apps/host/tests/e2e/`
pairs through the signer-bot service. It requires `SIGNER_BOT_SVC_TOKEN`;
`SIGNER_BOT_BASE_URL` and `SIGNER_BOT_NETWORK` default to dotli CI's
`https://signing-bot-dev.novasama-tech.org/` and `paseo-next-v2`. Without the
token, do not treat the full suite as locally runnable. Use
`E2E_DOTLI_SMOKE=1 make e2e-dotli` for the no-phone QR smoke path.

For a fully automated local playground diagnosis run, use:

```bash
SIGNER_BOT_SVC_TOKEN=... \
make e2e-dotli
```

`make e2e-dotli` starts dotli preview and the playground, signs out any
restored host session, signs in through signer-bot by extracting the QR payload,
runs the playground Diagnosis screen, auto-accepts host-side Allow/Sign modals,
and writes `hosts/dotli/test-results/e2e-dotli/diagnosis-report.md`.

Root CI runs the same target when it can read the private dotli submodule. It
needs `DOTLI_CHECKOUT_TOKEN` for submodule checkout; without that token, the
job warns and skips dotli e2e rather than failing unrelated PR checks. With
dotli access but without `SIGNER_BOT_SVC_TOKEN`, CI runs the no-phone smoke
path only.

A useful no-phone smoke assertion is:

```bash
E2E_DOTLI_SMOKE=1 make e2e-dotli
```

For manual debugging of that smoke path:

1. Start `make dev DEBUG=1`.
2. Open `http://localhost:5173/localhost:3000?debug=truapi&cachebust=<ts>` with
   service workers blocked.
3. Wait for `globalThis.__truapi?.setLogLevel`, call
   `__truapi.setLogLevel("debug")`, and confirm the console logs
   `[truapi worker] logLevel=debug providers=0`.
4. Click `#auth-button`, wait for `#auth-modal-backdrop.open`, and confirm:
   the modal shows `Login with Polkadot Mobile`, `__truapi.getProviderCount()`
   is greater than zero, worker frame/callback logs appear, and there are no
   page errors.

If `make dev` reports `EADDRINUSE` on `:5173` or the playground moves from
`:3000` to `:3001`, kill stale `preview-server.ts` / `next dev` processes and
restart the tmux session. Port drift causes false-negative local e2e results.

Useful debug signals:

```bash
localStorage.setItem("truapi:logLevel", "debug")
sessionStorage.setItem("dotli:truapi-debug", "1")
```

Reload after setting them. Watch for `Unknown wire discriminant`, missing
`@parity/truapi-host-wasm` imports, worker WASM instantiation failures, and
debug-panel traffic disappearing when the login popup opens.

## Deployment

Pushes to `main` trigger `.github/workflows/deploy-playground.yml`, which builds `playground/` and publishes the static export to `truapi-playground.dot` via `bulletin-deploy`.
Pushes to `main` also trigger `.github/workflows/deploy-docs.yml`, which publishes the explorer (at the Pages root), the playground (under `/playground/`), and the Rust API docs (under `/cargo_doc/`) to GitHub Pages.
