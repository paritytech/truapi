# truapi-codegen

_Code generator that reads rustdoc JSON from `truapi` and emits generated TypeScript client code._

## What this crate is for

`truapi-codegen` keeps generated client code aligned with Rust protocol definitions.

It reads rustdoc JSON, extracts TrUAPI API shape, then writes:

- generated TypeScript types
- generated TypeScript client classes
- generated TypeScript wire table

That output looks like this in practice.

Generated TypeScript client methods keep the API codecs local, encode payload bytes, and hand only wire frames to the transport:

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
          V2: [1, S.result(T.Account, T.RequestCredentialsError)] as const,
        }).dec(payload).value,
    });
    return result.success ? ok(result.value) : err(result.value);
  }
}
```

## Architecture

The generator has three stages:

1. `rustdoc` parses JSON emitted by nightly rustdoc
2. extracted API model is normalized, including each method's `#[wire(id = N)]`
3. generators write TypeScript output

Missing or duplicate wire ids fail generation. Subscription methods reserve four
consecutive ids for `_start`, `_stop`, `_interrupt`, and `_receive`.

## CLI

```bash
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --version V2 \
  --codec-version 1
```

## Example workflow

```bash
cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- --input target/doc/truapi.json --output js/packages/truapi/src/generated --version V2 --codec-version 1
```

## When to use it

Run this after trait or type changes in `truapi`. If you only change runtime behavior without changing protocol shape, you usually do not need to regenerate.
