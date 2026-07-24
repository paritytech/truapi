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
- **Sandbox bootstrap** (`@parity/truapi/sandbox`) that detects the host environment, builds the
  matching provider, and exposes a cached client — see below.
- **Observability surface** — an optional `observe` hook on the transport and a `createWireDebugger`
  relay for inspecting/forwarding wire frames (see below).

## Sandbox bootstrap

`@parity/truapi/sandbox` wires up a client for browser-embedded hosts: it detects whether the app
runs inside a TrUAPI host (iframe or webview), builds the matching provider, and caches the
resulting client. Use it instead of assembling `createTransport` / `createClient` by hand.

```ts
import {
  getClientSync,
  isCorrectEnvironment,
  subscribeConnectionStatus,
} from "@parity/truapi/sandbox";

const client = getClientSync(); // null outside a host container
if (client) {
  // …make host calls
}

// Or drive UI off connection status:
const unsubscribe = subscribeConnectionStatus((status) => {
  // "disconnected" | "connected"
});
```

| Export                                      | Purpose                                         |
| ------------------------------------------- | ----------------------------------------------- |
| `isCorrectEnvironment(): boolean`           | Synchronous host-environment detection.         |
| `getClientSync(): TrUApiClient \| null`     | Cached client; `null` outside a host container. |
| `subscribeConnectionStatus(cb): () => void` | Connected / disconnected status listener.       |

## Observability / debugging

`createTransport` accepts an optional `observe` callback that fires for every outbound and inbound
frame. It surfaces only the frame's `requestId`, inferred lifecycle `role`
(`request`/`response`/`start`/`stop`/`receive`/`interrupt`/`handshake`), `direction`, and `byteLength`
— **never the decoded payload** — so it is host-agnostic and leaks nothing. A throwing observer is
swallowed, and the hook is zero-cost when unset.

```ts
import { createTransport, createWireDebugger } from "@parity/truapi";
import { createWireDebugger as _ } from "@parity/truapi/debug"; // also on the ./debug subpath

const dbg = createWireDebugger(); // groups frames into per-requestId traces; forward/sink optional
const transport = createTransport(provider, { observe: dbg.observe });
// later: dbg.trace(requestId) → the WireTrace for one op (its outbound + inbound frames)
```

Because every frame carries its `requestId`, a product-side telemetry span (e.g.
`@parity/product-sdk-logger`'s `withSpan`) can adopt that id and the product → wire → host path becomes
one correlated trace. `createWireDebugger({ forward })` relays frames to a host debug panel.

`createMethodNameMap(wireTable, services)` builds a reverse map from a bare wire `frameId` to its
dotted method name (`22` → `account.getAccount`), derived from the generated wire-table plus the
client's service names (`Object.keys(createClient(transport))`). Pass it as
`createWireDebugger({ methodNames })` and the formatted frame lines carry real method names.

### Try it out

`examples/wire-debug-demo.mjs` fires a real `account.getAccount` round trip over an in-memory
provider and prints the readable trace — no host or network needed:

```bash
cd js/packages/truapi
bun examples/wire-debug-demo.mjs
```

```text
account.getAccount round trip:

  [wire p:1] → request account.getAccount (id=22, 14B)
  [wire p:1] ← response account.getAccount (id=23, 35B)

result: Ok
  account.publicKey = 0x1111…1111

WireTrace p:1 — 2 frames:
  → request  account.getAccount  (14B)
  ← response account.getAccount  (35B)
```

## Wire format

Frames are SCALE encoded:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant table is generated from Rust `#[wire(request_id = N)]` and `#[wire(start_id = N)]` annotations and is written to `src/generated/wire-table.ts`.

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

On a clean checkout, the first build or test run will generate the ignored TypeScript outputs from the Rust sources, so Rust stable + nightly must be installed locally. `npm test` runs the package's [`bun test`](https://bun.sh/docs/cli/test) suite (`src/**/*.test.ts`) directly against the source `.ts` files (no build step), so [bun](https://bun.sh/) must also be installed.

## License

[MIT](../../../LICENSE)
