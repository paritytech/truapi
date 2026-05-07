# @parity/truapi

TypeScript package for products that talk to a TrUAPI host.

It contains:

- transport providers for `MessagePort` and WebSocket pipes
- the TrUAPI request, response, subscription, and handshake transport
- generated domain clients and protocol types from the Rust API contract
- SCALE codec helpers used by the generated code

## Usage

```ts
import {
  createClient,
  createMessagePortProvider,
  createTransport,
} from "@parity/truapi";

const provider = createMessagePortProvider(port);
const transport = createTransport(provider);
const truapi = createClient(transport);

const account = await truapi.accountManagement.accountGet([
  "my-product.dot",
  0,
]);
```

Request methods take the inner request value directly. The transport handles the
wire-level version wrapper and unwraps versioned responses before generated
methods return.

Subscription methods take an object because they need both request data and
callbacks:

```ts
const sub = truapi.chainInteraction.chainHeadFollow({
  request: { genesisHash, withRuntime: false },
  onData(event) {
    console.log(event);
  },
  onInterrupt(error) {
    console.error(error);
  },
});

sub.unsubscribe();
```

## Wire Format

Frames are SCALE encoded:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant table is generated from Rust `#[wire(id = N, introduced =
V<N>)]` annotations and lives in `src/generated/wire-table.ts`.

## Generated Files

`src/generated/` is produced by `truapi-codegen` from the Rust crate. Do not edit
generated files directly; run from the repo root:

```bash
./scripts/codegen.sh
```

## Development

```bash
npm install
npm run build
npm test
```

`npm test` runs the package smoke tests under [bun](https://bun.sh/), so it
must be installed locally (`curl -fsSL https://bun.sh/install | bash`). The
tests load the source `.ts` files directly without a build step.
