//! Network provider backends for the [`truapi_platform::ChainProvider`]
//! capability, shared across every host platform.
//!
//! [`EmbeddedChainProvider`] maps chain genesis hashes to a per-chain
//! [`ChainSource`]. Backends hand the caller the raw JSON-RPC string pipe the
//! trait demands; request correlation and subscription routing stay with the
//! consumer (truapi-server's `HostRpcClient`).
//!
//! Per-target backend matrix:
//!
//! - `ws` feature — [`ChainSource::RpcNode`], a remote JSON-RPC node over
//!   WebSocket. On native targets it runs on a jsonrpsee transport and needs
//!   an ambient tokio runtime; on `wasm32` the same API is served by the
//!   browser's `WebSocket`.
//! - `smoldot` feature — [`ChainSource::LightClient`], an embedded
//!   [smoldot](https://github.com/paritytech/smoldot) light client. On native
//!   targets it runs on smoldot's default platform (OS threads, TCP + plain
//!   WebSocket dialing); on `wasm32` it runs on a vendored browser platform
//!   (JS event loop, browser `WebSocket` — including `wss` bootnodes).
//! - `networks` feature — a bundled catalog so `connect(genesis_hash)`
//!   resolves the whole network (relay wiring + statement placement included)
//!   from the genesis hash alone, with no prior registration.
//! - `js` feature — a JavaScript-facing API ([`js`]) on `wasm32`, so web
//!   hosts can consume the provider directly without a Rust caller.

// The `uniffi` feature pulls in UniFFI's generated scaffolding, which contains
// `unsafe` extern-"C" glue; scope the allowance to that feature so every other
// build keeps the crate's no-unsafe guarantee (see `[lints] unsafe_code`).
#![cfg_attr(
    all(feature = "uniffi", not(target_arch = "wasm32")),
    allow(unsafe_code)
)]

// The crate is inert without a backend: only the registry would compile, and
// `connect` could never succeed. Fail the build loudly instead.
#[cfg(not(any(feature = "ws", feature = "smoldot")))]
compile_error!("truapi-provider requires at least one backend feature: `ws` or `smoldot`");

mod config;
mod error;
#[cfg(all(feature = "uniffi", not(target_arch = "wasm32")))]
mod ffi;
#[cfg(all(feature = "js", target_arch = "wasm32"))]
pub mod js;
#[cfg(feature = "smoldot")]
mod light;
#[cfg(all(feature = "smoldot", target_arch = "wasm32"))]
mod light_platform_web;
#[cfg(feature = "networks")]
mod networks;
mod provider;
#[cfg(all(feature = "ws", not(target_arch = "wasm32")))]
mod ws;
#[cfg(all(feature = "ws", target_arch = "wasm32"))]
#[path = "ws_web.rs"]
mod ws;

pub use config::ChainSource;
#[cfg(feature = "smoldot")]
pub use config::LightClientBuilder;
#[cfg(feature = "networks")]
pub use networks::{NetworkChains, known_networks};
pub use provider::{EmbeddedChainProvider, EmbeddedChainProviderBuilder};

#[cfg(all(feature = "uniffi", not(target_arch = "wasm32")))]
uniffi::setup_scaffolding!();
