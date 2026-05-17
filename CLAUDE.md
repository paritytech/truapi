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
  truapi-server/         Rust runtime hosts implement; ships as WASM (browser/node) and via UniFFI (iOS/Android)
  uniffi-bindgen-cli/    Thin CLI wrapper around uniffi::uniffi_bindgen_main()
js/packages/
  truapi/                @parity/truapi TS package; generated TS lives under ignored paths
  truapi-host/           @parity/truapi-host host-side codegen + dispatcher
host-libs/
  js/shared/             @parity/host-shared: WASM-backed Provider, worker entrypoint, pre-built WASM under dist/wasm/
  js/web/                @parity/host-web: iframe MessageChannel + Web Worker WASM provider
  js/electron/           @parity/host-electron: Electron MessagePortMain provider
  android/               Kotlin shell + UniFFI bindings (generated under src/main/kotlin/generated)
  ios/                   Swift shell + UniFFI bindings (generated under Sources/Generated)
playground/              Next.js interactive playground; deploys to truapi-playground.dot
hosts/dotli/             dotli submodule
docs/                    design docs, RFCs, feature proposals
scripts/codegen.sh       regenerate the TS client from the Rust crate
```

### Crate + binding invariants

- `truapi` is canonical and untouched except for additive `Display` impls. New
  syscall traits and host-side runtime types live in `truapi-platform` and
  `truapi-server`, not in `truapi`.
- All types exposed by `truapi-platform` and `truapi-server` come from
  `truapi::versioned::*` and `truapi::v01::*`. The runtime crates re-export
  rather than redefine.
- Pre-built `truapi-server` WASM artifacts are committed under
  `host-libs/js/shared/dist/wasm/{web,node}/`. Regenerate via `make wasm`
  whenever `rust/crates/truapi-server/` changes; CI verifies the committed
  bundle is up to date.
- UniFFI bindings under `host-libs/{android,ios}/` are generated from the
  `truapi-server` cdylib via `make uniffi`. The generated Swift modulemap may
  need a one-time relocation into `Sources/truapi_serverFFI/include/` — the
  `make uniffi` target prints a reminder.

## Code style

- Every `pub` Rust item (functions, methods, types, traits, modules, constants) carries a doc comment (`///` or `//!`).
  Keep it short and focused on intent or invariants, not on what the signature already says.
- Do not add code comments or doc comments that narrate migrations, compatibility shims, or historical changes. Comments should describe only the current code.
- Remove legacy compatibility code by default. Keep or add it only when explicitly requested.
- In Rust format strings, prefer inlined variables: `"log value: {value:?}"` over `"log value: {:?}", value`.

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

## Deployment

Pushes to `main` trigger `.github/workflows/deploy-playground.yml`, which builds `playground/` and publishes the static export to `truapi-playground.dot` via `bulletin-deploy`.
Pushes to `main` also trigger `.github/workflows/deploy-docs.yml`, which publishes the Rust API docs to GitHub Pages.
