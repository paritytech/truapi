# @parity/truapi-host-wasm

WASM-backed TrUAPI host runtime. It embeds the `truapi-server` Rust core (compiled to WASM)
behind a Web Worker provider, plus per-environment integration entry points. It is the
counterpart to the native Android/iOS host shells.

## Entry points

The package exposes tree-shakeable subpath exports — import only what your environment needs:

| Import                                    | Provides                                                                                                                                       |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `@parity/truapi-host-wasm`                | Shared runtime types plus generated typed host callback contracts.                                                                             |
| `@parity/truapi-host-wasm/web`            | Browser pairing host: `createIframeHost` (iframe MessageChannel handshake) and `createWebWorkerPairingHostRuntime`. |
| `@parity/truapi-host-wasm/worker-runtime` | Web Worker entrypoint (import with your bundler's `?worker` suffix) so the WASM core runs off the page main thread.                            |
| `@parity/truapi-host-wasm/wasm/web`       | The raw browser `wasm-bindgen` glue, if you need to instantiate the core yourself.                                                             |

## Generated WASM artefacts

The ignored bundle under `dist/wasm/web/` is built with host-owned chain access.
Hosts wire their JSON-RPC provider through `chainConnect`; if they omit it,
chain calls fail with the core's standard unavailable error. The bundled WASM is
about 1 MB (release build with `wasm-opt`).

Build them after editing `rust/crates/truapi-server` and before packaging, publishing, or running
tests that load the raw WASM bundle (requires `wasm-pack` on PATH):

```bash
npm run build:wasm   # or `make wasm` from the repo root
```

## Example — browser (Web Worker)

```ts
import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host-wasm/web";

const runtime = await createWebWorkerPairingHostRuntime(new HostWorker(), callbacks, {
  hostConfig,
});

const firstProvider = await runtime.createProvider({ productId: "first.dot" });
const secondProvider = await runtime.createProvider({
  productId: "second.dot",
});
```

`@parity/truapi-host-wasm/web` also exports `createIframeHost` for the
protocol-iframe MessageChannel handshake. Host code creates one worker runtime
and then opens one provider per product id.

## Testing — `createMockHost`

`@parity/truapi-host-wasm/web` exports `createMockHost`, an in-memory implementation of the
full generated `HostCallbacks` surface — the JS sibling of `truapi-platform`'s `MockPlatform`.
Feed its callbacks to `createWebWorkerProvider` to run the **real WASM core** against a mocked
OS seam: storage is in-memory, permissions answer from a fixed policy, navigation and
notifications are recorded, and the chain connection is silent (or replays canned frames).

```ts
import {
  createMockHost,
  mockRuntimeConfig,
} from "@parity/truapi-host-wasm/web";

const mock = createMockHost(); // optional MockHostConfig
const provider = await createWebWorkerProvider(
  new HostWorker(),
  mock.callbacks,
  {
    runtimeConfig: mockRuntimeConfig(),
  },
);
// the real WASM core now talks to a mocked OS; assert via mock.navigations(), etc.
```

Coverage of the callback surface is `tsc`-enforced: the callbacks object is typed
`Required<HostCallbacks>`, so a capability added to the generated surface fails the type
check until the mock covers it. `wasm-bridge.test.ts` drives the real WASM core against the
mock headlessly (no browser, no worker). Public API: `createMockHost`, `mockRuntimeConfig`,
`MockHost`, `MockHostConfig`, `PermissionPolicy`.

### One call — `createMockClient`

`@parity/truapi-host-wasm/testing` exports `createMockClient`, which collapses
`createMockHost` + `createWebWorkerProvider` + `createClient` into a single call. It returns
the exact product client a product uses in production, plus the mock for assertions. Pass the
core Worker so your bundler owns how it is produced.

```ts
// `?worker` is a Vite/bundler-specific suffix; other bundlers use their own form.
import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
import { createMockClient } from "@parity/truapi-host-wasm/testing";

const { client, mock } = await createMockClient(new HostWorker(), {
  devicePermissions: "allow-all",
});
await client.system.handshake();
// ... drive the product client; assert via mock.navigations(), etc.
```

The full browser end-to-end harness that exercises this across a Web Worker and an iframe
lives in the `@parity/truapi-mock-e2e` package.

## Publishing

The npm publish workflow is not wired yet. A release-process discussion is needed before adding a
publish job to `.github/workflows/`. Until then, consumers depend on the package via the workspace
`file:` link or by publishing locally with `npm pack`.

## Architecture

```text
JS host code
  protocol handlers / typed callbacks
  (types from @parity/truapi-host-wasm)
       |
       v
createWebWorkerPairingHostRuntime
  shared worker runtime: pairing session, chain runtime, WASM instance
       |
       +-- createProvider({ productId }) -> product core / WireProvider
       |
       +-- createProvider({ productId }) -> product core / WireProvider
```
