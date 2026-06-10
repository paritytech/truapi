# @parity/truapi

_Typed TypeScript client for products that talk to a TrUAPI host._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)
[![Types](https://img.shields.io/badge/types-included-3178C6?style=flat-square&logo=typescript)](./package.json)

This package gives a product running inside a Polkadot host (Desktop Browser, Triangle webview) a fully typed client for every TrUAPI method. The transport, SCALE codecs, generated types, and generated domain clients are all bundled together.

## Install

```bash
npm install @parity/truapi
```

## Quick start

```ts
import {
  createClient,
  createMessagePortProvider,
  createTransport,
  type Client,
  type HostAccountGetResponse,
} from "@parity/truapi";

const provider = createMessagePortProvider(port);
const transport = createTransport(provider);
const truapi: Client = createClient(transport);

const result = await truapi.accountManagement.accountGet({
  productAccountId: { dotNsIdentifier: "my-product.dot", derivationIndex: 0 },
});

if (result.isErr()) throw result.error;
const account: HostAccountGetResponse = result.value;
```

Request methods take the inner request value directly. The transport adds the wire-level version wrapper and unwraps versioned responses before the generated method returns.

## Subscriptions

Streaming methods return a small Observable-compatible object:

```ts
import type { Subscription, RemoteChainHeadFollowItem } from "@parity/truapi";

const sub: Subscription = truapi.chainInteraction
  .chainHeadFollow({ request: { genesisHash, withRuntime: false } })
  .subscribe({
    next(event: RemoteChainHeadFollowItem) {
      console.log(event);
    },
    error(error: Error) {
      console.error(error);
    },
    complete() {
      console.log("stream ended");
    },
  });

sub.unsubscribe();
```

## What's in the package

- **Transport providers** for `MessagePort` pipes (used by both webview hosts and iframe hosts).
- **TrUAPI transport** that handles request, response, subscription, and handshake framing.
- **Generated domain clients and types** produced from the Rust API contract.
- **SCALE codec helpers** used by the generated code, also re-exported for direct use.

## Wire format

Frames are SCALE encoded:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant table is generated from Rust `#[wire(request_id = N)]` and `#[wire(start_id = N)]` annotations and is written to `src/generated/wire-table.ts`. The package also exports a reverse lookup for debug labels: `WIRE_TAG_BY_ID` (discriminant → `<method>_<kind>` tag) and `describeWireId(id)` (falls back to `wire_<id>` for unknown discriminants).

## Generated files

`src/generated/`, `src/playground/codegen/`, and `test/generated/examples/` are produced by [`truapi-codegen`](../../../rust/crates/truapi-codegen/) from the Rust crate and are ignored by git. Do not edit generated files directly. Run from the repo root:

```bash
./scripts/codegen.sh
```

## Develop

```bash
npm install
npm run build
npm test
```

On a clean checkout, the first build or test run will generate the ignored TypeScript outputs from the Rust sources, so Rust stable + nightly must be installed locally. `npm test` runs the package's smoke tests under [bun](https://bun.sh/), so bun must also be installed (`curl -fsSL https://bun.sh/install | bash`). The tests load the source `.ts` files directly without a build step.

## License

[MIT](../../../LICENSE)
