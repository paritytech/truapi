# @parity/host-shared

Shared TrUAPI host runtime layer. Provides:

- a WASM-backed `Provider` factory (`createNodeWasmProvider`,
  `createWasmProvider`) compatible with `@parity/truapi`
- a Web Worker entrypoint at `dist/worker-runtime.js` that owns the
  truapi-server WASM core off the page main thread
- generic dispatcher re-exports from `@parity/truapi-host` so hosts can
  install a single shared package and get both the WASM bridge and the
  typed handler dispatcher

## Pre-built WASM artefacts

The committed bundles under `dist/wasm/web/` and `dist/wasm/node/` are
built without smoldot (`wasm-pack build --no-default-features`). Hosts
that already manage chain access through their own JSON-RPC provider
wire `chainConnect` into the callbacks and never touch smoldot. The
bundled WASM is about 1 MB (release build with `wasm-opt`).

To rebuild after editing `rust/crates/truapi-server`:

```bash
npm run build:wasm
```

This rerun requires `wasm-pack` on PATH.

## Example (Node/Electron)

```ts
import { createNodeWasmProvider, createHostServer } from "@parity/host-shared";

const provider = await createNodeWasmProvider({
  navigateTo: async (url) => {
    /* shell.openExternal(url) */
  },
  pushNotification: async () => {},
  devicePermission: async () => true,
  remotePermission: async () => true,
  featureSupported: async (payload) => payload,
  localStorageRead: async () => undefined,
  localStorageWrite: async () => {},
  localStorageClear: async () => {},
  accountGet: async () => new Uint8Array(),
  accountGetAlias: async () => new Uint8Array(),
  accountCreateProof: async () => new Uint8Array(),
  getLegacyAccounts: async () => new Uint8Array(),
  accountConnectionStatusSubscribe: () => {},
  getUserId: async () => new Uint8Array(),
  signPayload: async () => new Uint8Array(),
  signRaw: async () => new Uint8Array(),
  statementStoreSubscribe: () => {},
  statementStoreSubmit: async () => new Uint8Array(),
  statementStoreCreateProof: async () => new Uint8Array(),
  preimageLookupSubscribe: () => {},
});

const server = createHostServer(provider, [
  /* dispatch entries */
]);
```

For web hosts see `@parity/host-web`'s `createWebWorkerProvider`.

## Architecture

```text
JS host code
  protocol handlers / typed callbacks
       |
       v
createHostServer (from @parity/truapi-host) <-- bytes --> Provider
                                                            |
                                                            v
                                          createWasmProvider / Worker
                                                            |
                                                            v
                                                truapi-server WASM core
```
