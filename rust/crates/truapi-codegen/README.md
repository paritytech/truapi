# truapi-codegen

*Code generator that reads rustdoc JSON from `truapi` and emits generated TypeScript client code plus optional Rust dispatcher glue.*

## What this crate is for

`truapi-codegen` keeps generated client and dispatcher code aligned with Rust protocol definitions.

It reads rustdoc JSON, extracts TrUAPI API shape, then writes:

- generated TypeScript types
- generated TypeScript client classes
- generated TypeScript wire table
- optional generated Rust dispatcher registration and wire table

That output looks like this in practice.

Generated TypeScript client methods wrap wire method names and codecs in small domain classes:

```ts
export interface TrUApiTransport {
  request<Request, Response>(method: string, value: Request, requestCodec: S.Codec<Request>, responseCodec: S.Codec<Response>): Promise<Response>;
}

export class AccountManagementClient {
  constructor(private readonly transport: TrUApiTransport) {}

  async accountGet(request: T.ProductAccountId): Promise<Result<T.Account, T.RequestCredentialsError>> {
    return this.transport.request("host_account_get", request, T.ProductAccountId, S.result(T.Account, T.RequestCredentialsError)) as Promise<Result<T.Account, T.RequestCredentialsError>>;
  }
}
```

Optional Rust output registers the same generated wire methods against the
server dispatcher and emits the runtime wire table used by `ProtocolMessage`
encoding:

```rust
pub(crate) fn register<P>(dispatcher: &mut Dispatcher, host: Arc<P>)
where
    P: TrUApi + 'static,
{
    register_calls(dispatcher, host.clone());
    register_permissions(dispatcher, host.clone());
    register_local_storage(dispatcher, host.clone());
    register_account(dispatcher, host.clone());
    register_signing(dispatcher, host.clone());
    register_chat(dispatcher, host.clone());
    register_statement_store(dispatcher, host.clone());
    register_preimage(dispatcher, host.clone());
    register_payment(dispatcher, host.clone());
    register_entropy(dispatcher, host.clone());
    register_chain(dispatcher, host);
}
```

## Architecture

The generator has three stages:

1. `rustdoc` parses JSON emitted by nightly rustdoc
2. extracted API model is normalized, including each method's `#[wire(id = N)]`
3. generators write TypeScript and optional Rust output

Missing or duplicate wire ids fail generation. Subscription methods reserve four
consecutive ids for `_start`, `_stop`, `_interrupt`, and `_receive`.

## CLI

```bash
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi-client/src/generated
```

Generate Rust dispatcher output too:

```bash
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi-client/src/generated \
  --rust-output rust/crates/truapi-server/src/generated
```

## Example workflow

```bash
cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- --input target/doc/truapi.json --output js/packages/truapi-client/src/generated
```

## When to use it

Run this after trait or type changes in `truapi`. If you only change runtime behavior without changing protocol shape, you usually do not need to regenerate.
