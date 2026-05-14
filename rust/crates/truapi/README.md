# truapi

*Source of truth for the TrUAPI protocol: shared traits, versioned types, and the wire dispatch table.*

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)

`truapi` is the canonical Rust definition of the TrUAPI protocol. If you are changing the API surface, this crate is where it starts.

It defines:

- **Versioned data types** under `v01`, `v02`, and `versioned`.
- **Domain API traits** under `api/`, plus the composed `TrUApi` trait.
- **Wire ids** via per-method `#[wire(id = N)]` annotations that pin the byte-level method table.
- **Subscription primitives** through `Subscription<T>` for streamed host responses.
- **Authoring types** like `CallContext`, `CallError<D>`, and `CancellationToken`.

The TypeScript client and the host dispatcher are both generated from this crate.

## Architecture

The crate has two layers:

1. **Protocol types** under `v01` and `v02`.
2. **Unified host contract** under `api`, where each method takes a `CallContext`, a versioned request type, and returns a versioned response with `CallError<D>` or a `Subscription<T>`.

Wire ids are part of the public protocol after F1: existing ids are append-only. Do not renumber or reuse them. The generated Rust dispatcher and the generated TypeScript wire table must stay byte-compatible with deployed products.

## Key modules

| Module | Role |
| ------ | ---- |
| `v02` | Current protocol-facing types. |
| `versioned` | Request, response, and subscription item wrappers for the unified trait surface. |
| `api` | Unified domain traits (`AccountManagement`, `ChainInteraction`, `Chat`, ...) and the composed `TrUApi` trait. |
| `failure` | Framework-level `CallError<D>` and lifecycle context types. |

## Example

Implement one or more of the unified sub-traits. `TrUApi` is a blanket trait over the full set:

```rust
use truapi::{CallContext, CallError, Subscription};
use truapi::api::{AccountManagement, TrUApi};
use truapi::versioned::account::{
    HostAccountConnectionStatusSubscribeItem,
    HostAccountGetError,
    HostAccountGetRequest,
    HostAccountGetResponse,
};
use truapi::v01::Account;

struct MyHost;

#[async_trait::async_trait]
impl AccountManagement for MyHost {
    async fn host_account_get(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Ok(HostAccountGetResponse::V1(Account {
            public_key: Vec::new(),
            name: None,
        }))
    }

    async fn host_account_connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::empty()
    }
}

fn _assert_truapi<T: TrUApi>() {}
```

Subscription endpoints return `Subscription<T>` so hosts can stream versioned items back to the runtime:

```rust
use truapi::Subscription;
use truapi::versioned::account::HostAccountConnectionStatusSubscribeItem;

fn _subscription_shape() -> Subscription<HostAccountConnectionStatusSubscribeItem> {
    Subscription::empty()
}
```

## Used by

- [`truapi-codegen`](../truapi-codegen/) reads rustdoc JSON for this crate to generate the TypeScript client.

## License

[MIT](../../../LICENSE)
