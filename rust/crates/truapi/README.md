# truapi

_Canonical TrUAPI contract: shared traits, shared types, versioned request and response shapes, and subscription primitives._

## What this crate is for

`truapi` is source of truth for protocol surface.

It defines:

- shared data types under `v01`, `v02`, and `versioned`
- domain API traits under `api/`
- per-method `#[wire(id = N)]` annotations that define the byte-level method table
- `Subscription<T>` for streamed host responses
- shared failure types like `CallContext` and `RuntimeFailure`

If you change API shape, start here.

## Architecture

This crate has two layers:

1. **Protocol types** under `v01` and `v02`
2. **Unified host contract** under `api`, where each method takes a `CallContext`, a versioned request type, and returns a versioned response or `Subscription<T>`

Codegen reuses the shared types from this crate.

Wire ids are part of the public protocol after F1. Existing ids are append-only:
do not renumber or reuse them, because the generated Rust and TypeScript wire
tables must stay byte-compatible with deployed products.

## Key modules

- `v02` - current protocol-facing types
- `versioned` - request, response, and subscription item wrappers used by the unified trait surface
- `api` - unified domain traits such as `AccountManagement`, `ChainInteraction`, and `Chat`, plus the composed `TrUApi` trait
- `failure` - runtime failure markers shared by generated dispatchers and host implementations

## Example

Implement one or more unified sub-traits. `TrUApi` is a blanket trait over the full set:

```rust
use truapi::{CallContext, Subscription};
use truapi::api::{AccountManagement, TrUApi};
use truapi::versioned::account::{
    HostAccountConnectionStatusItem,
    HostAccountGetRequest,
    HostAccountGetResponse,
};
use truapi::v02::{Account, RequestCredentialsError};

struct MyHost;

#[async_trait::async_trait]
impl AccountManagement for MyHost {
    async fn host_account_get(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, RequestCredentialsError> {
        Ok(HostAccountGetResponse::V2(Account {
            public_key: Vec::new(),
            name: None,
        }))
    }

    async fn host_account_connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusItem> {
        Subscription::empty()
    }
}

fn _assert_truapi<T: TrUApi>() {}
```

Subscription endpoints use `Subscription<T>` so hosts can stream versioned items back to the runtime.

```rust
use truapi::Subscription;
use truapi::versioned::account::HostAccountConnectionStatusItem;

fn _subscription_shape(
) -> Subscription<HostAccountConnectionStatusItem> {
    Subscription::empty()
}
```

## How other packages use it

- `truapi-codegen` reads rustdoc output from this crate
