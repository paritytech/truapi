<div align="center">

# TrUAPI

*The protocol that lets product webviews talk to their Polkadot host.*

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/paritytech/truapi/ci.yml?branch=main&style=flat-square&label=ci)](https://github.com/paritytech/truapi/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-rustdoc-blue?style=flat-square)](https://paritytech.github.io/truapi)
[![Playground](https://img.shields.io/badge/playground-live-success?style=flat-square)](https://truapi-playground.dot.li/)

</div>

<!-- TODO: Add hero screenshot of the playground showing methods + a live call/response. Capture with a screenshot tool, save to `assets/screenshots/playground.png`, then place it here. -->

TrUAPI (Triangle User-Agent Programming Interface) is the API surface that hosts like the Polkadot Desktop Browser expose to the products that run inside them. One Rust crate defines the contract, a code generator produces a typed TypeScript client, and hosts and products implement against the same shared types.

## Try it

Browse the published Rust API docs at [paritytech.github.io/truapi](https://paritytech.github.io/truapi).

The interactive playground lets you browse every method, edit request payloads, and call or subscribe to them live against a connected host.

**Live:** [truapi-playground.dot.li](https://truapi-playground.dot.li/) (open from inside the Polkadot Desktop Browser)

## Usage

`@parity/truapi` is the low-level generated protocol client. Product apps should normally use a higher-level product SDK, such as [`paritytech/product-sdk`](https://github.com/paritytech/product-sdk), while SDK and host-integration layers can depend on this package directly.

```bash
npm install @parity/truapi
```

```ts
import {
  createClient,
  createMessagePortProvider,
  createTransport,
} from "@parity/truapi";

const transport = createTransport(createMessagePortProvider(port));
const truapi = createClient(transport);

const result = await truapi.accountManagement.accountGet({
  productAccountId: { dotNsIdentifier: "my-product.dot", derivationIndex: 0 },
});
```

See [`js/packages/truapi/README.md`](js/packages/truapi/README.md) for the full client reference.

## Repository layout

```
rust/crates/
  truapi/                Rust trait and type definitions (v01, v02)
  truapi-codegen/        rustdoc JSON to TypeScript client + Rust dispatcher
  truapi-macros/         #[wire(id = N)] proc-macro
js/packages/
  truapi/                @parity/truapi TypeScript client
playground/              Interactive Next.js playground (truapi-playground.dot)
hosts/dotli/             dotli host, vendored as a submodule
docs/                    Design docs, RFCs, feature proposals
scripts/codegen.sh       Regenerate the TS client from the Rust source
```

## How it works

1. The protocol is defined as Rust traits in [`rust/crates/truapi/`](rust/crates/truapi/), with each method tagged `#[wire(id = N)]` for a stable byte-level dispatch table.
2. `truapi-codegen` reads rustdoc JSON for that crate and generates the TypeScript client under git-ignored paths in `js/packages/truapi/`.
3. Higher-level SDKs wrap the typed client; the transport encodes SCALE frames and ships them over `MessagePort` (or `postMessage` in iframe mode) to the host.
4. The host decodes the frame, dispatches to the matching trait method, encodes the response, and ships it back.

Wire ids are append-only: existing ids never change, so deployed products stay compatible across protocol revisions.

## Develop

Common tasks are wrapped in the top-level `Makefile`. Run `make help` for the full list.

```bash
make setup    # submodules + JS dependencies
make build    # Rust workspace + TypeScript client
make test     # Rust + TypeScript client tests
make check    # full suite: build, fmt, clippy, test, TS tests, playground build + lint
```

To run the playground locally:

```bash
cd playground
yarn dev
```

Open `https://dot.li/localhost:3000` inside the Polkadot Desktop Host. See [`playground/README.md`](playground/README.md) for deployment.

## Regenerate the TypeScript client

When the Rust trait surface changes:

```bash
make codegen      # regenerate the TS client and refresh the playground snapshot
make playground   # rebuild the playground against the refreshed snapshot
```

This repopulates the ignored generated TS under `js/packages/truapi/`, including the playground metadata.

## Protocol versions

- **v0.1**: initial protocol version.
- **v0.2**: current protocol version. See [`docs/design/releases/v0.2.md`](docs/design/releases/v0.2.md) for the rationale behind each change.

## Deploy

Pushes to `main` build and deploy:

- The playground to [`truapi-playground.dot`](https://truapi-playground.dot.li/) via [`.github/workflows/deploy-playground.yml`](.github/workflows/deploy-playground.yml).
- The Rust API docs to [https://paritytech.github.io/truapi](https://paritytech.github.io/truapi) via [`.github/workflows/deploy-docs.yml`](.github/workflows/deploy-docs.yml).

## Release

See [`docs/RELEASE_PROCESS.md`](docs/RELEASE_PROCESS.md) for how to ship a new `@parity/truapi` version to npm.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for issue reports, feature proposals, and the RFC process.

## License

[MIT](./LICENSE)
