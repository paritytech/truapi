//! Runtime-toggled debug logging.
//!
//! Enabled by the host setting a process-wide flag. Output goes to
//! `console.log` on wasm and `eprintln!` on native, prefixed with
//! `[truapi]` so it's easy to grep for.
//!
//! The macro is a no-op when disabled: format args are not evaluated,
//! so callers can `truapi_debug!("payload={value:?}")` without paying for the
//! formatting on hot paths. Disabled by default; the host opts in via
//! [`set_enabled`] (exposed to JS as `setDebugEnabled`).
//!
//! Output is plaintext to the console/stderr, so never pass secret material
//! (key bytes, session tokens, signatures) to [`truapi_debug!`].

use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Turn the [`truapi_debug!`] macro on or off. Idempotent.
pub fn set_enabled(enabled: bool) {
    DEBUG_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether debug logging is currently active. Cheap atomic read, safe to
/// call on hot paths so the macro can early-out before formatting.
pub fn is_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/// Native variant of `emit`: writes to stderr.
#[cfg(not(target_arch = "wasm32"))]
pub fn emit(line: &str) {
    eprintln!("{line}");
}

/// Wasm variant of `emit`: routes to the browser console.
#[cfg(target_arch = "wasm32")]
pub fn emit(line: &str) {
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(line));
}

/// Emit a debug log line when [`is_enabled`] is true.
#[macro_export]
macro_rules! truapi_debug {
    ($($arg:tt)*) => {{
        if $crate::debug_log::is_enabled() {
            $crate::debug_log::emit(&format!("[truapi] {}", format_args!($($arg)*)));
        }
    }};
}
