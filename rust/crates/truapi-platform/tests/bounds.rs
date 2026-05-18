//! Compile-time check that the `Platform` super-trait composes its capability
//! traits with `Send + Sync + 'static` bounds. `Platform` itself is not
//! object-safe (the capability traits use `async fn` returning
//! `impl Future`); the runtime consumes implementors via generics, not
//! `dyn Trait`.

use truapi_platform::Platform;

fn _assert_platform_bounds<T: Platform + Send + Sync + 'static>() {}
