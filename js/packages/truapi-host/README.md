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
import { errAsync, okAsync } from "neverthrow";
import {
  createTrUApiServer,
  type TrUApiHostHandlers,
  type TrUApiHostServer,
} from "@parity/truapi-host";

const provider = createMessagePortProvider(port);

const handlers: TrUApiHostHandlers = {
  account: {
    getAccount: {
      v1(ctx, request) {
        return okAsync({ account: myStore.lookup(request.productAccountId) });
      },
    },
    // …other AccountHostHandlers methods
  },
  // …other service handlers
};

const server: TrUApiHostServer = createTrUApiServer(provider, handlers);

// Tear down when the host shuts down:
server.dispose();
```

Each method on a service is a per-version handler map keyed by `v1`, `v2`, … . The dispatcher decodes the inbound versioned envelope, routes to the matching `vN` handler with the already-unwrapped request, awaits the returned `ResultAsync<Ok, Err>`, and re-wraps the outcome as the wire `Result` payload. Add a new `vN` entry when the host starts speaking a new wire version. Each handler receives a `CallContext` carrying the inbound `requestId` so it can correlate audit logs and per-call state.

### Handlers must not throw

Every outcome a handler can produce, including permission denials, backend timeouts, and any other failure mode, must be expressed as a typed `ResultAsync<Ok, Err>` outcome (use `okAsync(...)` / `errAsync(...)` from `neverthrow`). For subscriptions, emit failures via `observer.error?.(new SubscriptionError("...", { reason }))` for `ResultSubscription` streams, or `observer.complete?.()` for plain `Subscription` streams.

The dispatcher does install `HostServerHooks.onRequestHandlerError` and `onSubscriptionStartError` for defensive purposes (e.g. a `TypeError` from an upstream bug), but if either fires the client sees a hung request or never-started stream, not a typed failure, so treat any invocation as a programming error to fix at the source, not a normal control-flow path.

## Subscriptions

Subscription handlers return an `ObservableLike<Item, Reason>` typed against the versioned item wrapper. The dispatcher subscribes when the start frame arrives, bridges the resulting `Observer` callbacks onto wire frames, and unsubscribes when the client stops the stream (or the transport closes). The shape mirrors what `@parity/truapi` clients receive on the other side.

```ts
import type { ObservableLike } from "@parity/truapi-host";

const handlers: TrUApiHostHandlers = {
  account: {
    connectionStatusSubscribe: {
      v1(ctx) {
        return {
          subscribe(observer) {
            const unsubscribe = myStore.onStatusChange((status) => {
              observer.next?.(status);
            });
            return { unsubscribe, subscriptionId: "" };
          },
        };
      },
    },
    // …
  },
};
```

For methods declared as `ResultSubscription` on the Rust side, the returned `ObservableLike<Item, Reason>` carries a typed `Reason`. Emit a typed interrupt by calling `observer.error?.(new SubscriptionError("...", { reason }))` (`SubscriptionError` is re-exported from `@parity/truapi`); the dispatcher pulls the typed reason out and encodes it as the interrupt frame. For plain `Subscription` methods, `observer.complete?.()` ends the stream and emits an untyped interrupt frame on the wire.

## What's in the package

- **`createTrUApiServer(provider, handlers)` factory** that attaches a typed dispatcher to a `Provider` from `@parity/truapi`.
- **Generated handler interfaces**, one per service trait (`AccountHostHandlers`, `ChainHostHandlers`, `ChatHostHandlers`, …), composed into `TrUApiHostHandlers`.
- **`CallContext` and `HostServerHooks` types** plus `ObservableLike` / `Observer` / `Subscription` re-exported from `@parity/truapi` for handler signatures, per-call state, and protocol-drift visibility.
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
