// Copyright 2019-2026 Parity Technologies (UK) Ltd.
// This file is dual-licensed as Apache-2.0 or GPL-3.0; see LICENSE-APACHE.

//! Browser [`smoldot_light::platform::PlatformRef`] implementation, vendored
//! from subxt-lightclient 0.50.1 (`src/platform/wasm_*`, Apache-2.0): tasks
//! spawn on the JS event loop and peers are dialed over the browser's
//! `WebSocket` (which, unlike native sockets, can reach `wss` bootnodes).

mod helpers;
mod platform;
mod socket;

pub(crate) use platform::SubxtPlatform;
