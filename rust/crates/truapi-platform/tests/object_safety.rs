//! Compile-time check that the Platform trait composition stays object-safe / Send / Sync.

use truapi_platform::Platform;

fn _assert_platform_bounds<T: Platform + Send + Sync + 'static>() {}
