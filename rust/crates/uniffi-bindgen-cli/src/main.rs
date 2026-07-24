//! Thin CLI wrapper around `uniffi::uniffi_bindgen_main()`.
//!
//! Lets the TrUAPI workspace regenerate Kotlin and Swift bindings from
//! `truapi-server`'s UniFFI scaffolding without depending on a globally
//! installed `uniffi-bindgen`.

fn main() {
    uniffi::uniffi_bindgen_main();
}
