# truapi-codegen

_Reads rustdoc JSON for the `truapi` crate and generates client and runtime code._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)

`truapi-codegen` keeps generated code aligned with the Rust protocol definition. It reads rustdoc JSON, extracts the TrUAPI API surface, and writes:

- TypeScript types for every protocol type in `truapi`.
- TypeScript domain client classes for every unified trait.
- The TypeScript wire dispatch table.
- The Rust host dispatcher and wire dispatch table consumed by `truapi-server`.

## Generated output

Generated client methods keep API codecs local, encode payload bytes, and hand only wire frames to the transport:

```ts
export interface TrUApiTransport {
  request<Response>(params: {
    method: string;
    payload: Uint8Array;
    decodeResponse: (payload: Uint8Array) => Response;
  }): Promise<Response>;
}

export class AccountManagementClient {
  constructor(private readonly transport: TrUApiTransport) {}

  async accountGet(
    request: T.ProductAccountId,
  ): Promise<Result<T.Account, T.RequestCredentialsError>> {
    const result = await this.transport.request<
      S.ResultPayload<T.Account, T.RequestCredentialsError>
    >({
      method: "host_account_get",
      payload: T.HostAccountGetRequest.enc({ tag: "V2", value: request }),
      decodeResponse: (payload) =>
        S.indexedTaggedUnion({
          V2: [1, S.Result(T.Account, T.RequestCredentialsError)] as const,
        }).dec(payload).value,
    });
    return result.success ? ok(result.value) : err(result.value);
  }
}
```

## Architecture

The generator runs in three stages:

1. **Parse**: read JSON emitted by nightly rustdoc.
2. **Normalize**: extract the API model, including each method's `#[wire(id = N)]`.
3. **Emit**: generators write TypeScript client output and, when requested, Rust runtime output.

Missing or duplicate wire ids fail generation. Subscription methods reserve four consecutive ids for `_start`, `_stop`, `_interrupt`, and `_receive`.

## CLI

```bash
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --rust-output rust/crates/truapi-server/src/generated \
  --client-version V2 \
  --codec-version 1
```

## Typical workflow

```bash
cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --rust-output rust/crates/truapi-server/src/generated \
  --client-version V2 \
  --codec-version 1
```

The repo wraps both steps in [`scripts/codegen.sh`](../../../scripts/codegen.sh), which is what you should run from the repo root.

## When to run it

Run after any trait or type change in [`truapi`](../truapi/). If you only change runtime behavior without changing the protocol shape, regeneration is not needed.

## License

[MIT](../../../LICENSE)
