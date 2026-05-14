# @parity/truapi-host

_Typed TypeScript dispatcher for hosts that serve TrUAPI methods._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)
[![Types](https://img.shields.io/badge/types-included-3178C6?style=flat-square&logo=typescript)](./package.json)

This package gives a Polkadot host (Desktop Browser, Triangle webview) a fully typed inbound dispatcher for every TrUAPI method. The dispatcher, generated handler interfaces, and versioned envelope wrap/unwrap are all bundled together. It is the host-side counterpart to [`@parity/truapi`](../truapi/), generated from the same rustdoc JSON so wire ids, codecs, and types match exactly.

## Install

```bash
npm install @parity/truapi-host
```

## Quick start

```ts
import { createMessagePortProvider } from "@parity/truapi";
import {
  createTrUApiServer,
  type TrUApiHostHandlers,
  type TrUApiHostServer,
} from "@parity/truapi-host";

const provider = createMessagePortProvider(port);

const handlers: TrUApiHostHandlers = {
  account: {
    async getAccount(ctx, request) {
      if (request.tag === "V1") {
        const account = await myStore.lookup(request.value.productAccountId);
        return {
          tag: "V1",
          value: { success: true, value: { account } },
        };
      }
      // The wire version is one this host build doesn't speak. Reply in the
      // highest version we do speak with the matching error type, which
      // mirrors the Rust `HostAccountGetError::Unknown { reason }` variant.
      return {
        tag: "V1",
        value: {
          success: false,
          value: {
            tag: "Unknown",
            value: { reason: `unsupported wire version: ${request.tag}` },
          },
        },
      };
    },
    // …other AccountHostHandlers methods
  },
  // …other service handlers
};

const server: TrUApiHostServer = createTrUApiServer(provider, handlers);

// Tear down when the host shuts down:
server.dispose();
```

Each handler receives a `CallContext` (carrying the inbound `requestId` so handlers can correlate audit logs and per-call state) followed by the request struct still in its versioned envelope. Unlike the client, a host serves clients across every protocol version it has shipped, so the handler is responsible for matching on `request.tag` and producing a response wrapped in the matching version tag. The dispatcher only handles the SCALE codec; it never collapses versions or constructs Result wrappers for you.

## Subscriptions

Subscription handlers receive a `SubscriptionSink` typed against the versioned item wrapper, and return a cleanup function. Wrap each emitted value in the version tag matching the client's request:

```ts
import type { SubscriptionSink } from "@parity/truapi-host";
import type { VersionedHostAccountConnectionStatusSubscribeItem } from "@parity/truapi";

const handlers: TrUApiHostHandlers = {
  account: {
    connectionStatusSubscribe(
      ctx,
      sink: SubscriptionSink<VersionedHostAccountConnectionStatusSubscribeItem>,
    ) {
      const unsubscribe = myStore.onStatusChange((status) => {
        if (!sink.isClosed) sink.send({ tag: "V1", value: status });
      });
      return () => unsubscribe();
    },
    // …
  },
};
```

Methods declared as `ResultSubscription` on the Rust side also expose `sink.interrupt(reason)`, taking the versioned reason wrapper, which emits a typed interrupt frame and tears the subscription down. The dispatcher tracks per-subscription state by inbound `requestId` and invokes the returned cleanup on stop frames, transport close, or interrupt.

## What's in the package

- **`createTrUApiServer(provider, handlers)` factory** that attaches a typed dispatcher to a `Provider` from `@parity/truapi`.
- **Generated handler interfaces**, one per service trait (`AccountHostHandlers`, `ChainHostHandlers`, `ChatHostHandlers`, …), composed into `TrUApiHostHandlers`.
- **`SubscriptionSink`, `CallContext`, and `HostServerHooks` types** for handler signatures, per-call state, and protocol-drift visibility.
- **Hand-written `server-core`** that owns the dispatch table, active subscription state, and provider plumbing.

## Out of scope

The dispatcher exposes 1:1 wire primitives. Subscription multiplexing, deduplication, buffering, replay-to-late-subscribers, and connection-status policy are intentionally not in scope, products and hosts layer their own policy on top when needed.

## Wire format

Frames are SCALE encoded:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant table is generated from Rust `#[wire(request_id = N)]` and `#[wire(start_id = N)]` annotations and is re-exported from [`@parity/truapi/wire-table`](../truapi/) so the client and host always agree on ids.

## Generated files

`src/generated/` is produced by [`truapi-codegen`](../../../rust/crates/truapi-codegen/) from the Rust crate and is ignored by git. Do not edit generated files directly. Run from the repo root:

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
