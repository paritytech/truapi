# @parity/truapi-provider

Network provider backends for the TrUAPI `ChainProvider` contract, compiled to
WebAssembly for browser hosts. It embeds a [smoldot](https://github.com/smol-dot/smoldot)
light client and a remote WebSocket JSON-RPC backend behind one API, and bundles
the chain-spec catalog so a host can `connect(genesisHash)` without shipping its
own specs.

It is the browser counterpart to the native (iOS/Android) provider that the same
crate exposes over UniFFI, so chain access behaves identically across hosts.

## Usage

The bundle is `wasm-bindgen` glue plus a `.wasm` binary. Instantiate the module
once per page/worker, then open a connection per genesis hash. Connections share
the single embedded light client.

```js
import init, { ChainProviderBuilder } from "@parity/truapi-provider";
import wasmUrl from "@parity/truapi-provider/truapi_provider_bg.wasm?url";

await init({ module_or_path: wasmUrl });

// A bundled network is resolved from the genesis hash alone (relay wiring and
// statement-store placement come from the catalog); no per-chain registration.
const provider = new ChainProviderBuilder().build();
const connection = await provider.connect("0x77af…");

connection.send(
  '{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}',
);
const response = await connection.nextResponse(); // undefined once closed
connection.close();
```

Add a remote node instead of the light client with
`builder.addRpcChain(genesisHash, "wss://node.example")`, or a light client for
an unbundled chain with `builder.addLightChain(genesisHash, specification)`.

## Building

The `dist/` bundle is generated and gitignored. Rebuild it from the Rust crate:

```bash
npm run build:wasm      # wasm-pack --target web, features "js networks"
```

`wasm-pack` is required (`cargo install wasm-pack`). Set `TRUAPI_WASM_PROFILE=dev`
for a fast unoptimized build. The repo's `make wasm` target rebuilds this bundle
alongside the host runtime.

## Publishing

Publishing goes through `paritytech/npm_publish_automation`, so no local npm
token is needed:

- **Stable release** — a `release:`-prefixed commit on `main` triggers
  [`release.yml`](../../../.github/workflows/release.yml), which publishes the
  committed version to `latest`.
- **Dev snapshot** — run the [`Dev publish`](../../../.github/workflows/dev-publish.yml)
  workflow (`workflow_dispatch`, optional `npm_tag_suffix`). It stamps a unique
  `<next-patch>-dev-<YYYYMMDD>[-suffix].<N>` version and publishes it under a
  `dev-<YYYYMMDD>[-suffix]` dist-tag, so `latest` is never moved. Consumers pin
  the exact version it prints.

## License

MIT AND Apache-2.0. See [LICENSE](LICENSE), [LICENSE-APACHE](LICENSE-APACHE), and
[NOTICE](NOTICE).
