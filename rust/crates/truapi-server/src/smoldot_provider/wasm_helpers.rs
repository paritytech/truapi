// SPDX-License-Identifier: Apache-2.0
// Vendored from subxt-lightclient 0.50.0 (Apache-2.0 OR GPL-3.0), elected
// under Apache-2.0. See ../../THIRD_PARTY_NOTICES.md for attribution.

use super::wasm_socket::WasmSocket;

use core::time::Duration;
use futures_util::{FutureExt, future};

/// Returns the current wall-clock time as a duration since the Unix epoch.
pub fn now_from_unix_epoch() -> Duration {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| {
            panic!("Invalid systime cannot be configured earlier than `UNIX_EPOCH`")
        })
}

/// Monotonic instant type used by the wasm smoldot platform.
pub type Instant = web_time::Instant;

/// Returns the current monotonic instant used by the wasm smoldot platform.
pub fn now() -> Instant {
    web_time::Instant::now()
}

/// Boxed delay future returned by `sleep`.
pub type Delay = future::BoxFuture<'static, ()>;

/// Creates a future that resolves after the provided duration.
pub fn sleep(duration: Duration) -> Delay {
    futures_timer::Delay::new(duration).boxed()
}

/// Smoldot expects a single concrete stream type with pinned access; the
/// wrapper hides the buffer/socket pair behind a `pin_project` projection.
#[pin_project::pin_project]
pub struct Stream(
    #[pin]
    pub  smoldot::libp2p::with_buffers::WithBuffers<
        future::BoxFuture<'static, Result<WasmSocket, std::io::Error>>,
        WasmSocket,
        Instant,
    >,
);
