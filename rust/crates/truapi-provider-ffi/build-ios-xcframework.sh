#!/usr/bin/env bash
# Build the TruapiProviderFFI xcframework and Swift bindings for iOS.
#
# Produces (under the workspace target/ dir):
#   - target/TruapiProviderFFI.xcframework   (static lib + module map)
#   - target/ios-bindings/truapi_provider_ffi.swift
#
# Add the xcframework and the generated .swift to an Xcode target. By default
# only the simulator slice is built; pass `--device` to add the arm64 device
# slice too.
set -euo pipefail

CRATE=truapi-provider-ffi
LIB=libtruapi_provider_ffi.a
PROFILE=${PROFILE:-debug}
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"

CARGO_FLAGS=(-p "$CRATE" --lib)
[ "$PROFILE" = release ] && CARGO_FLAGS+=(--release)

SLICES=(aarch64-apple-ios-sim)
[ "${1:-}" = "--device" ] && SLICES+=(aarch64-apple-ios)

for target in "${SLICES[@]}"; do
  rustup target add "$target" >/dev/null 2>&1 || true
  echo "==> building $target ($PROFILE)"
  cargo build "${CARGO_FLAGS[@]}" --target "$target"
done

BINDINGS="target/ios-bindings"
rm -rf "$BINDINGS" && mkdir -p "$BINDINGS"
echo "==> generating Swift bindings"
cargo run -q -p "$CRATE" --features cli --bin uniffi-bindgen -- \
  generate --library "target/aarch64-apple-ios-sim/$PROFILE/$LIB" \
  --language swift --out-dir "$BINDINGS"

HEADERS="target/ios-headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$BINDINGS/truapi_provider_ffiFFI.h" "$HEADERS/"
cp "$BINDINGS/truapi_provider_ffiFFI.modulemap" "$HEADERS/module.modulemap"

OUT="target/TruapiProviderFFI.xcframework"
rm -rf "$OUT"
ARGS=()
for target in "${SLICES[@]}"; do
  ARGS+=(-library "target/$target/$PROFILE/$LIB" -headers "$HEADERS")
done
echo "==> packaging $OUT"
xcodebuild -create-xcframework "${ARGS[@]}" -output "$OUT"
echo "done: $OUT"
