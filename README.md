<div align="center">

# TrUAPI

> The following is a prototype, reference implementation, and proof-of-concept. This open source code is provided for research, experimentation, and developer education only. This code has not been audited, is actively experimental, and may contain bugs, vulnerabilities, or incomplete features. Use at your own risk.

_The protocol that lets product webviews talk to their Polkadot host._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/paritytech/truapi/ci.yml?branch=main&style=flat-square&label=ci)](https://github.com/paritytech/truapi/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-rustdoc-blue?style=flat-square)](https://paritytech.github.io/truapi)
[![Playground](https://img.shields.io/badge/playground-live-success?style=flat-square)](https://truapi-playground.dot.li/)

</div>

<!-- TODO: Add hero screenshot of the playground showing methods + a live call/response. Capture with a screenshot tool, save to `assets/screenshots/playground.png`, then place it here. -->

TrUAPI (Triangle User-Agent Programming Interface) is the API surface that hosts like the Polkadot Desktop Browser expose to the products that run inside them. One Rust crate defines the contract, a code generator produces a typed TypeScript client, and hosts and products implement against the same shared types.

## Try it

Browse the published Rust API docs at [paritytech.github.io/truapi](https://paritytech.github.io/truapi).

The interactive playground lets you browse every method, edit request payloads, and call or subscribe to them live against a connected host. It also drives an end-to-end **Diagnosis** that produces a per-host pass/fail report ([playground/README.md → Diagnosis](playground/README.md#diagnosis)). The explorer aggregates those reports into a cross-host **Compatibility** matrix ([explorer/README.md → Host compatibility matrix](explorer/README.md#host-compatibility-matrix)).

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
  productAccountId: { dotNsIdentifier: "my-product.dot", derivationIndex: { tag: "Index", value: 0 } },
});
```

See [`js/packages/truapi/README.md`](js/packages/truapi/README.md) for the full client reference.

## Repository layout

```
rust/crates/
  truapi/                Rust traits, versioned envelopes, and latest payload re-exports
  truapi-codegen/        rustdoc JSON to TypeScript client + Rust dispatcher
  truapi-macros/         #[wire(id = N)] proc-macro
  truapi-platform/       Host syscall traits used by truapi-server (storage, navigation, consent, ...)
  truapi-server/         Host runtime: dispatcher, typed SCALE logic, chain signing, WASM surface
js/packages/
  truapi/                  @parity/truapi TypeScript client
  truapi-host/            @parity/truapi-host: WASM-backed host runtime; entries `.`
                          (shared host types), `/web` (iframe + Web Worker),
                          `/worker-runtime`
js/container/              TS lockdown container for the iOS host web view; bundles into
                           ios/truapi-host/Sources/TrUAPIHost/Resources/truapi-container.js
android/truapi-host/       Kotlin host adapter package over the truapi-server UniFFI core
ios/truapi-host/           Swift host adapter package over the truapi-server UniFFI core
playground/                Interactive Next.js playground (truapi-playground.dot)
hosts/dotli/               dotli host, vendored as a submodule
docs/                      Design docs, RFCs, feature proposals
scripts/codegen.sh         Regenerate the TS client from the Rust source
scripts/battery.sh         Run the generated battery against both headless CLI host roles
```

The Swift host adapter (the `TrUAPIHost` SPM package over the truapi-server
UniFFI core) lives under [`ios/truapi-host/`](ios/truapi-host), with its SPM
manifest at the repo root (`Package.swift`) so apps can consume it as a git-URL
dependency. Its `scripts/rebuild.sh` regenerates the committed bindings and
container bundle (`make xcframework` + `make uniffi`); see
[`ios/truapi-host/README.md`](ios/truapi-host/README.md).

### JS Host SDKs

JS hosts integrate the Rust core through [`@parity/truapi-host`](js/packages/truapi-host),
a single package with tree-shakeable subpath entries:

- `@parity/truapi-host` (the `.` entry) exposes shared host runtime types and generated callback contracts.
- `@parity/truapi-host/web` wires the WASM provider into a browser host: the iframe
  MessageChannel handshake (`createIframeHost`) plus `createWebWorkerProvider`.
- `@parity/truapi-host/worker-runtime` is the Web Worker entrypoint so the WASM core can
  run off the page main thread.

## How it works

1. The protocol is defined as Rust traits in [`rust/crates/truapi/`](rust/crates/truapi/), with each method tagged `#[wire(id = N)]` for a stable byte-level dispatch table. Every method's doc comment must carry a ` ```ts ` example, which codegen extracts into the playground's EXAMPLE tab; the build fails if any method is missing one.
2. `truapi-codegen` reads rustdoc JSON for that crate and generates the TypeScript client under git-ignored paths in `js/packages/truapi/`.
3. Higher-level SDKs wrap the typed client; the transport encodes SCALE frames and ships them over `MessagePort` (or `postMessage` in iframe mode) to the host.
4. The host decodes the frame, dispatches to the matching trait method, encodes the response, and ships it back.

Wire ids are append-only: existing ids never change, so deployed products stay compatible across protocol revisions.

## Develop

Common tasks are wrapped in the top-level `Makefile`. Run `make help` for the full list.

```bash
make setup    # submodules + JS dependencies
make build    # Rust workspace + TypeScript client + @parity/truapi-host
make test     # Rust + TypeScript client + @parity/truapi-host tests
make check    # full suite: build, fmt, clippy, test, TS tests, playground build + lint
make wasm     # rebuild truapi-server WASM artifacts under js/packages/truapi-host/dist/wasm/
```

CI regenerates the shared bindings before building and testing both npm
packages, so generated client and host callback changes are checked together.

The native `truapi-host` utility can run pairing and signing hosts against the
real SSO transport for local end-to-end work. Both roles provide a
transcript-based terminal UI with commands such as `/product` and `/script`;
the signing host also provides `/pair` and a non-interactive `exec` form for
automation. See the
[`truapi-host-cli` guide](rust/crates/truapi-host-cli/README.md) for setup,
controls, and examples.

`scripts/battery.sh` drives that CLI from source over every code-generated
example and writes both committed compatibility reports:
`explorer/diagnosis-reports/spa/signing-host-cli.md` from a direct signing-host
run, and `spa/pairing-host-cli.md` from a pairing host that the script pairs with a
signing host it starts itself.

```bash
scripts/battery.sh                  # both phases
scripts/battery.sh --signing-host   # direct phase only
scripts/battery.sh --pairing-host   # paired phase only
make e2e-signing-cli                # same direct signing-host phase
make e2e-pairing-cli                # same paired pairing-host phase
```

To run the playground locally:

```bash
cd playground
yarn dev
```

Open `https://dot.li/localhost:3000` inside the Polkadot Desktop Host. See [`playground/README.md`](playground/README.md) for deployment.

To build the iOS host and open the playground in Simulator:

```bash
make ios-run
```

The target regenerates the UniFFI Swift bindings, builds the matching Rust
simulator library, and builds the sibling `polkadot-app-ios-v2` checkout with the Nightly feature
flags and release Firebase app used by the Nightly TestFlight build. Native
Chat and the Paseo chain catalog come from the same Nightly Remote Config as
TestFlight. The executable keeps the development bundle and app-group identity
so Simulator can reuse its already registered wallet; keychain data cannot be
transferred to the production bundle. The simulator also adds the
`IOS_PASEO_E2E` conveniences needed to start on the real `browse.dot` Browse tab
and activate the embedded signing host. Embedded product host sessions use the
same Paseo People and Bulletin chains selected by the Nightly app
configuration. The launcher starts the playground at `http://localhost:3100`
when needed and uses that local source only after Browse opens
`truapi-playground.dot`. It refuses to launch if the local URL belongs to a
different app. Override the product, URL, or simulator with `IOS_PRODUCT_HOST`,
`IOS_PRODUCT_URL`, or `TRUAPI_IOS_E2E_DEVICE`. The simulator launch reuses the
wallet and registered username already stored by the iOS app. Opening a product
activates the embedded `truapi-host` signing-host session from that wallet; it
does not provision or pair a signer-bot user.

To exercise the shared-core Chat path with the first-party TrUAPI Playground
worker, build and serve the local product, install its worker into the
simulator app's product storage, and open its native Chat application:

```bash
make ios-chat-run
```

The launcher verifies the Chat connection and runs a correlated Chat-only
diagnosis. The worker proves create-room idempotency, observes the new room on
the live list subscription, posts text and custom messages, receives
`!diagnose` through `chat_action_subscribe`, and serves live renderer trees.
The launcher also verifies that a renderer update reaches native code and that
the final Markdown report reaches CoreData. It writes the host-labelled report
to `playground/test-results/ios-chat/diagnosis-report.md`. The product builds
against the workspace-linked `@parity/truapi`. Override the product source,
identity, SPA URL, room, input, or report path with
`IOS_CHAT_PRODUCT_DIR`, `IOS_CHAT_PRODUCT_HOST`, `IOS_CHAT_PRODUCT_URL`,
`TRUAPI_IOS_E2E_CHAT_ROOM_ID`, `TRUAPI_IOS_E2E_CHAT_MESSAGE`, or
`TRUAPI_IOS_E2E_CHAT_REPORT`.

The same harness can run the legacy Product SDK worker from the sibling
`host-playground` checkout. This target builds the current TrUAPI client, links
it over Host Playground's transitive `@parity/truapi`, then builds and runs
that product:

```bash
make ios-chat-host-playground-run
```

Run both integrations with one iOS build using `make ios-chat-all`.

## Regenerate the TypeScript client

When the Rust trait surface changes:

```bash
make codegen      # regenerate the TS client and refresh the playground snapshot
make playground   # rebuild the playground against the refreshed snapshot
```

This repopulates the ignored generated TS under `js/packages/truapi/`, including the playground metadata.
`make dev` and `make e2e-dotli` run this generation step unconditionally before starting their local stacks.
The full `make e2e-dotli` diagnosis builds and launches the local
`truapi-host signing-host` CLI to answer dotli's pairing QR and auto-approve
remote signing requests. It does not require the external signer-bot service.
When `HOST_CLI_SIGNER_MNEMONIC` is absent, the CLI manages a reusable isolated
test identity under `.e2e-dotli/`. Set `E2E_DOTLI_SIGNING_HOST_BASE_PATH` to
use a different state directory while debugging.

## Protocol versions

- **v0.1**: initial protocol version.
- **v0.2**: See [`docs/design/releases/v0.2.md`](docs/design/releases/v0.2.md) for the rationale behind each change.
- **v0.3**: current protocol version.

## Deploy

Pushes to `main` build and deploy:

- The playground to [`truapi-playground.dot`](https://truapi-playground.dot.li/) via [`.github/workflows/deploy-playground.yml`](.github/workflows/deploy-playground.yml).
- The Rust API docs to [https://paritytech.github.io/truapi](https://paritytech.github.io/truapi) via [`.github/workflows/deploy-docs.yml`](.github/workflows/deploy-docs.yml).

## Release

See [`docs/RELEASE_PROCESS.md`](docs/RELEASE_PROCESS.md) for how to ship
`@parity/truapi`, `@parity/truapi-host`, or both packages to npm.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for issue reports, feature proposals, and the RFC process.

## License

[MIT](./LICENSE)
