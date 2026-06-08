# @parity/truapi-host-wasm

WASM-backed TrUAPI host runtime. It embeds the `truapi-server` Rust core (compiled to WASM) and
provides the `Provider` factories that drive it, plus per-environment integration entry points.
It is the counterpart to the native Android/iOS host shells.

> This is distinct from [`@parity/truapi-host`](../truapi-host), which is the host-side codegen +
> dispatcher for hosts that bring their **own** runtime and do not embed the shared Rust core.

## Entry points

The package exposes tree-shakeable subpath exports — import only what your environment needs:

| Import                                     | Provides                                                                                                                |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `@parity/truapi-host-wasm`                 | Core: `createWasmProvider`, `createNodeWasmProvider`, `createHostServer`, the dispatcher adapter, and the shared types. |
| `@parity/truapi-host-wasm/web`             | Browser host: `createIframeHost` (iframe MessageChannel handshake) and `createWebWorkerProvider`.                       |
| `@parity/truapi-host-wasm/electron`        | `createElectronProvider` — wraps an Electron `MessagePortMain` as a `Provider`.                                         |
| `@parity/truapi-host-wasm/worker-runtime`  | Web Worker entrypoint (import with your bundler's `?worker` suffix) so the WASM core runs off the page main thread.     |
| `@parity/truapi-host-wasm/wasm/{web,node}` | The raw `wasm-bindgen` glue, if you need to instantiate the core yourself.                                              |

## Pre-built WASM artefacts

The committed bundles under `dist/wasm/web/` and `dist/wasm/node/` are built without smoldot
(`wasm-pack build --no-default-features`). Hosts that already manage chain access through their own
JSON-RPC provider wire `chainConnect` into the callbacks and never touch smoldot. The bundled WASM
is about 1 MB (release build with `wasm-opt`).

To rebuild after editing `rust/crates/truapi-server` (requires `wasm-pack` on PATH):

```bash
npm run build:wasm   # or `make wasm` from the repo root
```

## Example — Node / Electron

```ts
import {
  createNodeWasmProvider,
  createHostServer,
} from "@parity/truapi-host-wasm";
import { createElectronProvider } from "@parity/truapi-host-wasm/electron";

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
  // Optional: presentPairing, readSession/writeSession/clearSession,
  // subscribeSessionStore, confirmation, preimage, theme, and chain callbacks.
});

const server = createHostServer(provider, [
  /* dispatch entries */
]);
```

## Example — browser (Web Worker)

```ts
import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
import { createWebWorkerProvider } from "@parity/truapi-host-wasm/web";

const provider = await createWebWorkerProvider(new HostWorker(), callbacks);
```

`@parity/truapi-host-wasm/web` also exports `createIframeHost` for the protocol-iframe
MessageChannel handshake.

## Publishing

TODO: npm publish workflow not yet wired. A release-process discussion is needed before adding a
publish job to `.github/workflows/`. Until then, consumers depend on the package via the workspace
`file:` link or by publishing locally with `npm pack`.

## Architecture

```text
JS host code
  protocol handlers / typed callbacks
       |
       v
createHostServer (re-exported from @parity/truapi-host) <-- bytes --> Provider
                                                                        |
                                                                        v
                                                      createWasmProvider / Worker
                                                                        |
                                                                        v
                                                            truapi-server WASM core
```
