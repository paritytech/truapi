# @parity/truapi-host

WASM-backed TrUAPI host runtime. It embeds the `truapi-server` Rust core (compiled to WASM)
behind a Web Worker provider, plus per-environment integration entry points. It is the
counterpart to the native Android/iOS host shells.

## Entry points

The package exposes tree-shakeable subpath exports — import only what your environment needs:

| Import                               | Provides                                                                                                            |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| `@parity/truapi-host`                | Shared runtime types plus generated typed host callback contracts.                                                  |
| `@parity/truapi-host/web`            | Browser pairing host: `createIframeHost` (iframe MessageChannel handshake) and `createWebWorkerPairingHostRuntime`. |
| `@parity/truapi-host/worker-runtime` | Web Worker entrypoint (import with your bundler's `?worker` suffix) so the WASM core runs off the page main thread. |
| `@parity/truapi-host/wasm/web`       | The raw browser `wasm-bindgen` glue, if you need to instantiate the core yourself.                                  |

## Generated WASM artefacts

The ignored bundle under `dist/wasm/web/` is built with host-owned chain access.
Hosts wire their JSON-RPC provider through `chainConnect`; if they omit it,
chain calls fail with the core's standard unavailable error. Release builds use
the workspace size-optimized Rust profile plus `wasm-opt -Oz`, validate that
debug/name/producers custom sections were stripped, and emit `.wasm.gz` and
`.wasm.br` sidecars for hosts that serve precompressed assets.

Build them after editing `rust/crates/truapi-server` and before packaging, publishing, or running
tests that load the raw WASM bundle (requires `wasm-pack` on PATH):

```bash
npm run build:wasm   # or `make wasm` from the repo root
```

## Example — browser (Web Worker)

```ts
import HostWorker from "@parity/truapi-host/worker-runtime?worker";
import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host/web";

const runtime = await createWebWorkerPairingHostRuntime(
  new HostWorker(),
  callbacks,
  {
    hostConfig,
  },
);

const firstProvider = await runtime.createProvider({ productId: "first.dot" });
const secondProvider = await runtime.createProvider({
  productId: "second.dot",
});
```

`@parity/truapi-host/web` also exports `createIframeHost` for the
protocol-iframe MessageChannel handshake. Host code creates one worker runtime
and then opens one provider per product id.

## Testing — in-memory mock host

`@parity/truapi-host/web` also exports `createMockHost`, the JS sibling of
`truapi-platform`'s `MockPlatform`. It returns a complete
`RequiredHostCallbacks` set (in-memory storage, fixed permission policy, a
silent-or-scripted chain connection, recorded navigations/notifications) plus
accessor oracles for assertions, so tests and host simulators can drive the real
WASM core against a mocked OS seam with no device and no network. `MockHostConfig`
tunes permissions, feature support, theme, confirmation, and chain responses;
`mockRuntimeConfig` builds a matching `ProductRuntimeConfig`. Hand `host.callbacks`
straight to `createWebWorkerPairingHostRuntime` or to `createWasmRawCallbacks`.
Signing and login still need a paired wallet, so those flows park under the
default silent chain.

## Publishing

This package is published by the root `Release` workflow through
`paritytech/npm_publish_automation`. Do not run `npm publish` locally. Cut a
`release:` PR with a changeset for `@parity/truapi-host`; the workflow builds
the generated host bindings, the browser WASM bundle, packs the tarball, and
publishes it when the `@parity/truapi-host@<version>` tag does not already
exist.

## Architecture

```text
JS host code
  protocol handlers / typed callbacks
  (types from @parity/truapi-host)
       |
       v
createWebWorkerPairingHostRuntime
  shared worker runtime: pairing session, chain runtime, WASM instance
       |
       +-- createProvider({ productId }) -> product core / WireProvider
       |
       +-- createProvider({ productId }) -> product core / WireProvider
```
