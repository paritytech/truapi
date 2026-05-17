# uniffi-bindgen-cli

Thin CLI wrapper around `uniffi::uniffi_bindgen_main()` for generating native bindings (Swift and Kotlin) from UniFFI inputs in this workspace.

This crate exists so TrUAPI's native host SDKs (`host-libs/android`, `host-libs/ios`) can regenerate bindings via workspace-local tooling rather than relying on a globally installed `uniffi-bindgen`.

It does not add custom logic. It forwards directly into UniFFI's standard CLI entry point.

## Usage

```bash
cargo run -p uniffi-bindgen-cli -- generate \
  --library target/debug/libtruapi_server.so \
  --language kotlin \
  --out-dir host-libs/android/src/generated

cargo run -p uniffi-bindgen-cli -- generate \
  --library target/debug/libtruapi_server.dylib \
  --language swift \
  --out-dir host-libs/ios/TrUAPIHost/Sources/Generated
```

See `uniffi-bindgen --help` for the full CLI surface.
