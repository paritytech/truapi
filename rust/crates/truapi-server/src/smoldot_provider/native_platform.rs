//! Native platform wiring for `smoldot_light`.
//!
//! `DefaultPlatform` ships with smoldot-light (std feature, default on) and
//! spawns its own background threads for network I/O.

use std::sync::Arc;

use smoldot_light::platform::default::DefaultPlatform;

/// Alias for the platform reference type smoldot-light expects.
pub type PlatformRefAlias = Arc<DefaultPlatform>;

/// Creates the native smoldot platform implementation used by the server.
pub fn make_platform() -> PlatformRefAlias {
    DefaultPlatform::new("truapi".into(), env!("CARGO_PKG_VERSION").into())
}
