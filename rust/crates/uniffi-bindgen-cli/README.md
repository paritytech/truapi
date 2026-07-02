# uniffi-bindgen-cli

Thin CLI wrapper around `uniffi::uniffi_bindgen_main()` for generating native bindings (Swift and Kotlin) from UniFFI inputs in this workspace.

This crate exists so TrUAPI's native host SDKs (`android`, `ios`) can regenerate bindings via workspace-local tooling rather than relying on a globally installed `uniffi-bindgen`.

It does not add custom logic. It forwards directly into UniFFI's standard CLI entry point.

## Usage

```bash
cargo run -p uniffi-bindgen-cli -- generate \
  --library target/debug/libtruapi_server.so \
  --language kotlin \
  --out-dir android/truapi-host/src/main/kotlin/generated
```

Swift bindings land in `ios/truapi-host/Sources/TrUAPIHost/truapi_server.swift`
with the C header / module map under
`ios/truapi-host/Sources/truapi_serverFFI/include/`. The CLI emits all three
files into one directory, then the modulemap is renamed to `module.modulemap`
and colocated with the header so SwiftPM's `systemLibrary` target picks them up.
The simplest path is `make uniffi` from the repo root; see
[`ios/truapi-host/README.md`](../../../ios/truapi-host/README.md) for the exact
generate-and-relocate steps.

See `uniffi-bindgen --help` for the full CLI surface.
