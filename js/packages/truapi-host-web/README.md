# @parity/truapi-host-web

Browser TrUAPI host wrapper:

- `createIframeHost`, embed a product iframe, transfer a `MessagePort`
  into it via the `truapi-init` handshake, and hand the host-side port
  back to the caller.
- `createWebWorkerProvider`, spawn the truapi-server WASM core inside a
  Web Worker and bridge it into a `Provider` (so smoldot, header
  verification, and dispatcher work run off the page main thread).

## Example

```ts
import { createMessagePortProvider, createTransport } from "@parity/truapi";
import { createHostServer } from "@parity/truapi-host";
import HostWorker from "@parity/truapi-host-shared/worker-runtime?worker";
import { createIframeHost, createWebWorkerProvider } from "@parity/truapi-host-web";

// 1. WASM core inside a Worker, exposed as a Provider.
const coreProvider = await createWebWorkerProvider(new HostWorker(), {
  navigateTo: async (url) => window.open(url, "_blank"),
  pushNotification: async () => {},
  devicePermission: async () => true,
  remotePermission: async () => true,
  featureSupported: async (payload) => payload,
  localStorageRead: async () => undefined,
  localStorageWrite: async () => {},
  localStorageClear: async () => {},
  /* ...remaining account / signing / store callbacks */
} as never);

// 2. Wire the iframe's MessageChannel into the same provider.
createIframeHost({
  iframeUrl: "http://localhost:5174",
  container: document.getElementById("app")!,
  onPort: (port) => {
    const iframeProvider = createMessagePortProvider(port);
    // hand both providers off to your host server / transport pair
    createHostServer(iframeProvider, [
      /* dispatch entries */
    ]);
  },
});
```

The window-level legacy `postMessage` fallback present in earlier
prototypes is intentionally not provided here; products must use the
canonical MessageChannel rail.

## Publishing

TODO: npm publish workflow not yet wired. The `@parity/truapi-host-shared`,
`@parity/truapi-host-web`, and `@parity/truapi-host-electron` packages need a release-process
discussion before we add a publish job to `.github/workflows/`. Until then,
consumers should depend on the package via the workspace `file:` link or by
publishing locally with `npm pack`.
