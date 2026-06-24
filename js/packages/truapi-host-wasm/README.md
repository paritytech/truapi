# @parity/truapi-host-wasm

WASM-backed TrUAPI host runtime. It embeds the `truapi-server` Rust core (compiled to WASM) and
provides the `Provider` factories that drive it, plus per-environment integration entry points.
It is the counterpart to the native Android/iOS host shells.

> This is distinct from [`@parity/truapi-host`](../truapi-host), which is the host-side codegen +
> dispatcher for hosts that bring their **own** runtime and do not embed the shared Rust core.

## Entry points

The package exposes tree-shakeable subpath exports — import only what your environment needs:

| Import                                    | Provides                                                                                                            |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `@parity/truapi-host-wasm`                | Core: `createHostCoreProvider`, `createHostServer`, and the dispatcher adapter. Typed callback contracts come from `@parity/truapi-host/callbacks`. |
| `@parity/truapi-host-wasm/web`            | Browser host: `createIframeHost` (iframe MessageChannel handshake) and `createWebWorkerProvider`.                   |
| `@parity/truapi-host-wasm/worker-runtime` | Web Worker entrypoint (import with your bundler's `?worker` suffix) so the WASM core runs off the page main thread. |
| `@parity/truapi-host-wasm/wasm/web`       | The raw browser `wasm-bindgen` glue, if you need to instantiate the core yourself.                                  |

## Generated WASM artefacts

The ignored bundle under `dist/wasm/web/` is built without smoldot
(`wasm-pack build --no-default-features`). Hosts that already manage chain access through their own
JSON-RPC provider wire `chainConnect` into the callbacks and never touch smoldot. The bundled WASM
is about 1 MB (release build with `wasm-opt`).

Build them after editing `rust/crates/truapi-server` and before packaging, publishing, or running
tests that load the raw WASM bundle (requires `wasm-pack` on PATH):

```bash
npm run build:wasm   # or `make wasm` from the repo root
```

## Example — browser (Web Worker)

```ts
import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
import { createWebWorkerProvider } from "@parity/truapi-host-wasm/web";

const provider = await createWebWorkerProvider(new HostWorker(), callbacks, {
  runtimeConfig,
});
```

`@parity/truapi-host-wasm/web` also exports `createIframeHost` for the protocol-iframe
MessageChannel handshake.

## Publishing

The npm publish workflow is not wired yet. A release-process discussion is needed before adding a
publish job to `.github/workflows/`. Until then, consumers depend on the package via the workspace
`file:` link or by publishing locally with `npm pack`.

## Architecture

```text
JS host code
  protocol handlers / typed callbacks
  (types from @parity/truapi-host/callbacks)
       |
       v
createHostServer (re-exported from @parity/truapi-host) <-- bytes --> Provider
                                                                        |
                                                                        v
                                                   createHostCoreProvider / Worker
                                                                        |
                                                                        v
                                                            truapi-server WASM core
```
