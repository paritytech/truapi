# @parity/host-electron

Electron TrUAPI host wrapper. Exposes `createElectronProvider`, which
wraps an Electron `MessagePortMain` as a `Provider` from
`@parity/truapi`. Pair it with `createNodeWasmProvider` from
`@parity/host-shared` and `createHostServer` from `@parity/truapi-host`
to assemble a full Electron host.

## Architecture

1. preload script transfers `port2` into renderer
2. main process keeps `port1`
3. `createElectronProvider({ port: port1 })` returns a `Provider`
4. host code feeds that provider into `createHostServer`

## Example

```ts
import { createNodeWasmProvider } from "@parity/host-shared";
import { createHostServer } from "@parity/truapi-host";
import { createElectronProvider } from "@parity/host-electron";

const coreProvider = await createNodeWasmProvider(callbacks);
const rendererProvider = createElectronProvider({ port: messagePortMain });

const server = createHostServer(rendererProvider, [
  /* dispatch entries */
]);
```
