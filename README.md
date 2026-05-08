# TrUAPI

The TrUAPI (Triangle User-Agent Programming Interface) Protocol mediates all communication between a host application and products running in sandboxes inside it.

This repository is the single source of truth for the protocol:

- **`rust/crates/truapi/`** — Rust trait and type definitions for protocol versions v0.1 and v0.2.
- **`rust/crates/truapi-codegen/`** — code generator that turns rustdoc JSON into the TypeScript client.
- **`rust/crates/truapi-macros/`** — proc-macro for `#[wire(id = N)]` annotations.
- **`js/packages/truapi/`** — the typed TypeScript client (`@parity/truapi`), with `src/generated/` produced by `truapi-codegen`.
- **`explorer/`** — static GitHub Pages documentation explorer generated from the Rust API source.
- **`playground/`** — interactive Next.js explorer/playground for the protocol, deployed to [`truapi-playground.dot`](https://truapi-playground.dot.li/).

## Layout

```
rust/crates/
  truapi/                Rust trait + type definitions (v01, v02)
  truapi-codegen/        rustdoc JSON → TS client + Rust dispatcher
  truapi-macros/         #[wire(id = N)] proc-macro
js/packages/
  truapi/         @parity/truapi TS package
explorer/               Vite documentation explorer for GitHub Pages
playground/              Next.js interactive playground
docs/                    design docs, RFCs, feature proposals
scripts/codegen.sh       regenerate the TS client from the Rust crate
```

## Regenerating the TS client

```bash
./scripts/codegen.sh
```

Under the hood this runs:

```bash
cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --playground-output js/packages/truapi/src/playground \
  --explorer-output js/packages/truapi/src/explorer
```

Commit the regenerated `src/generated/` alongside the Rust changes.

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
cd js/packages/truapi
npm install
npm run build
npm test
```

### Playground

```bash
cd playground
yarn install --frozen-lockfile
yarn dev
```

Open `https://dot.li/localhost:3000` inside the Polkadot Desktop Host. See [`playground/README.md`](playground/README.md) for full deployment instructions.

### Explorer

```bash
cd explorer
npm install
npm run dev
```

The explorer is a static GitHub Pages app. Its registry data is generated into `@parity/truapi/explorer/registry`.

## Deployment

Pushes to `main` trigger [`.github/workflows/deploy.yml`](.github/workflows/deploy.yml), which builds the playground and publishes its static export to the `truapi-playground.dot` DotNS name.

## Protocol versions

- **v0.1** — initial protocol version.
- **v0.2** — current protocol version. See [`docs/design/v02-changes.md`](docs/design/v02-changes.md) for the rationale behind each change.

## License

MIT
