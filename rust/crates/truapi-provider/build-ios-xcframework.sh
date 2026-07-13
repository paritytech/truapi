#!/usr/bin/env bash
# Build the truapi-provider xcframework and Swift bindings for iOS from the
# crate's `uniffi` feature.
#
# Produces (under the workspace target/ dir):
#   - target/TruapiProviderFFI.xcframework   (static lib + module map)
#   - target/ios-bindings/truapi_provider.swift
#
# Add the xcframework and the generated .swift to an Xcode target. By default
# only the simulator slice is built; pass `--device` to add the arm64 device
# slice too.
set -euo pipefail

LIB=libtruapi_provider.a
PROFILE=${PROFILE:-debug}
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"

CARGO_FLAGS=(-p truapi-provider --lib --no-default-features --features uniffi)
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
cargo run -q -p truapi-provider --features cli --bin uniffi-bindgen -- \
  generate --library "target/aarch64-apple-ios-sim/$PROFILE/$LIB" \
  --language swift --out-dir "$BINDINGS"

HEADERS="target/ios-headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$BINDINGS/truapi_providerFFI.h" "$HEADERS/"
cp "$BINDINGS/truapi_providerFFI.modulemap" "$HEADERS/module.modulemap"

OUT="target/TruapiProviderFFI.xcframework"
rm -rf "$OUT"
ARGS=()
for target in "${SLICES[@]}"; do
  ARGS+=(-library "target/$target/$PROFILE/$LIB" -headers "$HEADERS")
done
echo "==> packaging $OUT"
xcodebuild -create-xcframework "${ARGS[@]}" -output "$OUT"
echo "done: $OUT"
