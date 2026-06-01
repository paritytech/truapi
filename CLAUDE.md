# CLAUDE.md

Guidance for Claude Code when working in this repository.

This repo is the single source of truth for the TrUAPI protocol. It vendors `dotli` as a git submodule at `hosts/dotli/`.

## Layout

```
rust/crates/
  truapi/                Rust trait + type definitions for protocol versions v0.1 and v0.2
  truapi-codegen/        rustdoc JSON → TypeScript client + Rust dispatcher
  truapi-macros/         #[wire(id = N)] proc-macro
js/packages/
  truapi/         @parity/truapi TS package; generated TS lives under ignored paths
playground/              Next.js interactive playground; deploys to truapi-playground.dot
hosts/dotli/             dotli submodule
docs/                    design docs, RFCs, feature proposals
scripts/codegen.sh       regenerate the TS client from the Rust crate
```

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
npm test                         # registry lookup/routing unit tests (bun)
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

## Deployment

Pushes to `main` trigger `.github/workflows/deploy-playground.yml`, which builds `playground/` and publishes the static export to `truapi-playground.dot` via `bulletin-deploy`.
Pushes to `main` also trigger `.github/workflows/deploy-docs.yml`, which publishes the explorer (at the Pages root), the playground (under `/playground/`), and the Rust API docs (under `/cargo_doc/`) to GitHub Pages.
