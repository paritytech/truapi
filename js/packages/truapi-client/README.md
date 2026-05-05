# @truapi/client

*Typed TrUAPI client package: transport adapters, SCALE codecs, and generated domain clients for products running inside sandboxed hosts.*

## What this package is for

`@truapi/client` is product-side entry point.

It exports:

- transport primitives like `createMessagePortProvider`
- binary transport support through `byteProtocolCodecAdapter`
- request lifecycle via `createTransport`
- generated client classes such as `AccountManagementClient`
- generated protocol types
- SCALE helpers under `scale`

## Architecture

The package has three layers:

1. **Provider** - raw message pipe like `MessagePort`
2. **Transport** - request, response, subscription lifecycle
3. **Generated clients** - domain-level API methods backed by codecs

`Provider` knows how bytes or structured-clone messages move.
`createTransport` knows request ids, subscriptions, and payload codecs.
Generated clients know method names and type codecs.

Payloads are always SCALE-encoded bytes at the transport boundary. The default
codec adapter is `byteProtocolCodecAdapter`, so the canonical web/Electron
`MessagePort` transport, native bridges, and localhost WebSocket bridges all
use the F1 envelope:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant table is generated from Rust `#[wire(id = N)]` annotations and
matches the Rust runtime table.

## Example

```ts
import {
  AccountManagementClient,
  createMessagePortProvider,
  createTransport,
} from '@truapi/client'

const provider = createMessagePortProvider(windowPort)
const transport = createTransport(provider)
const accounts = new AccountManagementClient(transport)

const result = await accounts.accountGet(['my-product.dot', 0])

if (result.success) {
  console.log(result.value)
}
```

Tests or in-process harnesses can opt into structured-clone objects explicitly:

```ts
import { createTransport, structuredCloneCodecAdapter } from '@truapi/client'

const transport = createTransport(provider, structuredCloneCodecAdapter)
```

## Generated output

Generated client classes, types, and the wire table live under
`src/generated/`. They are produced by `truapi-codegen` from Rust protocol
definitions.

## Related packages

- `truapi` defines protocol shape
- `truapi-codegen` generates client code
- `@truapi/host-shared`, `@truapi/host-web`, and `@truapi/host-electron` expose matching host-side building blocks
