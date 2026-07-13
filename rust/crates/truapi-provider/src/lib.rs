//! Network provider backends for the [`truapi_platform::ChainProvider`]
//! capability, shared across every host platform.
//!
//! [`NativeChainProvider`] maps chain genesis hashes to a per-chain
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

mod config;
mod provider;

#[cfg(all(feature = "js", target_arch = "wasm32"))]
pub mod js;
#[cfg(feature = "smoldot")]
mod light;
#[cfg(all(feature = "smoldot", target_arch = "wasm32"))]
mod light_platform_web;
#[cfg(feature = "networks")]
mod networks;
#[cfg(all(feature = "ws", not(target_arch = "wasm32")))]
mod ws;
#[cfg(all(feature = "ws", target_arch = "wasm32"))]
#[path = "ws_web.rs"]
mod ws;

pub use config::ChainSource;
#[cfg(feature = "networks")]
pub use networks::{NetworkChains, known_networks};
pub use provider::{NativeChainProvider, NativeChainProviderBuilder};
