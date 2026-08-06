//! Entry point for `uniffi-bindgen` (library mode), used to generate the
//! Swift bindings from the compiled `truapi-provider-ffi` library.

fn main() {
    uniffi::uniffi_bindgen_main()
}
