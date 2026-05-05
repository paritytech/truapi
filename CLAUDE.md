# CLAUDE.md

Guidance for Claude Code when working in this repository.

This repo is the single source of truth for the TrUAPI protocol. It vendors `dotli` as a git submodule at `hosts/dotli/` (URL: `git@github.com:paritytech/dotli`, branch: `main`).

## Layout

```
rust/crates/
  truapi/                Rust trait + type definitions for protocol versions v0.1 and v0.2
  truapi-codegen/        rustdoc JSON → TypeScript client + Rust dispatcher
  truapi-macros/         #[wire(id = N)] proc-macro
js/packages/
  truapi-client/         @truapi/client TS package; src/generated/ produced by truapi-codegen
playground/              Next.js interactive explorer; deploys to truapi-playground.dot
hosts/dotli/             dotli submodule (paritytech/dotli@main)
docs/                    design docs, RFCs, feature proposals
scripts/codegen.sh       regenerate the TS client from the Rust crate
```

## Code style

- Every `pub` Rust item (functions, methods, types, traits, modules, constants) carries a doc comment (`///` or `//!`). Keep it short and focused on intent or invariants, not on what the signature already says.
- In Rust format strings, prefer inlined variables: `"log value: {value:?}"` over `"log value: {:?}", value`.

## First-time setup

```bash
# Check out the dotli submodule
git submodule update --init --recursive

# Build the TypeScript client (triggers tsc via `prepare`)
( cd js/packages/truapi-client && npm install )

# Install playground dependencies (picks up @truapi/client via the file: link)
( cd playground && yarn install --frozen-lockfile )
```

## Regenerating the TS client

When the Rust trait surface changes, rerun:

```bash
./scripts/codegen.sh
```

That runs `cargo +nightly rustdoc -p truapi --output-format json` followed by `cargo run -p truapi-codegen` to repopulate `js/packages/truapi-client/src/generated/`. Commit the regenerated files alongside the Rust changes.

After regenerating, rebuild the client and refresh the playground's link copy:

```bash
( cd js/packages/truapi-client && npm run build )
( cd playground && rm -rf node_modules/@truapi && yarn install )
```

(yarn 1.x copies `file:` deps at install time, so the playground's `node_modules/@truapi/client` is a snapshot.)

## Local development

### Rust

```bash
cargo build --workspace
cargo +nightly fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### TypeScript client

```bash
cd js/packages/truapi-client
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

The playground must be opened from inside a TrUAPI host. Locally, navigate to `https://dot.li/localhost:3000` inside the Polkadot Desktop Host.

## Working inside the dotli submodule

`hosts/dotli/` is a standalone repo on branch `main`. Commits there are pushed to `git@github.com:paritytech/dotli` independently; the parent repo then bumps the pinned submodule SHA:

```bash
git -C hosts/dotli add -A && git -C hosts/dotli commit -m "…"
git -C hosts/dotli push origin main
git add hosts/dotli && git commit -m "Bump dotli submodule"
```

Do not open PRs against either repo without explicit instruction.

## Deployment

Pushes to `main` trigger `.github/workflows/deploy.yml`, which builds `playground/` and publishes the static export to `truapi-playground.dot` via `bulletin-deploy`.
