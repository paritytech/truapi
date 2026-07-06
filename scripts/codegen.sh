#!/usr/bin/env bash
# Regenerate js/packages/truapi/src/generated/* from rust/crates/truapi.
#
# Pipeline:
#   1. cargo +nightly rustdoc -p truapi --output-format json -> target/doc/truapi.json
#   2. cargo run -p truapi-codegen -- --input target/doc/truapi.json
#                                     --output js/packages/truapi/src/generated
#                                     --playground-output js/packages/truapi/src/playground
#                                     --client-examples-output playground/test/generated/examples
#                                     --rust-output rust/crates/truapi-server/src/generated
#                                     --platform-input target/doc/truapi_platform.json
#                                     --platform-ts-output js/packages/truapi-host-wasm/src/generated
#                                     --platform-wasm-adapter-output js/packages/truapi-host-wasm/src/generated
#                                     --platform-rust-output rust/crates/truapi-server/src/wasm
#                                     --codec-version 1
#
# The client surface defaults to the latest wire version any versioned
# wrapper exposes; pass `--client-version V<N>` to pin to an older one.
#
# Run from the repo root.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json
cargo +nightly rustdoc -p truapi-platform -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --playground-output js/packages/truapi/src/playground \
  --client-examples-output playground/test/generated/examples \
  --rust-output rust/crates/truapi-server/src/generated \
  --platform-input target/doc/truapi_platform.json \
  --platform-ts-output js/packages/truapi-host-wasm/src/generated \
  --platform-wasm-adapter-output js/packages/truapi-host-wasm/src/generated \
  --platform-rust-output rust/crates/truapi-server/src/wasm \
  --explorer-output js/packages/truapi/src/explorer \
  --codec-version 1

node scripts/regen-explorer-versions.mjs

npm exec --yes -- prettier --write \
  "js/packages/truapi/src/generated/**/*.ts" \
  "js/packages/truapi/src/playground/**/*.ts" \
  "js/packages/truapi/src/explorer/**/*.ts" \
  "playground/test/generated/examples/**/*.ts" \
  "js/packages/truapi-host-wasm/src/generated/**/*.ts"

# Rebuild dist/ so downstream consumers (in particular the playground,
# which picks up @parity/truapi via yarn 1.x file: snapshot) see the
# regenerated bindings without a separate npm run build step.
#
# The build runs twice: the first pass emits the freshly-generated client
# sources to dist/, then `bundle-truapi-dts.mjs` snapshots dist/*.d.ts into
# src/playground/codegen/truapi-dts.ts so Monaco can register the package as
# an ambient module without HTTP fetches. The second pass compiles the new
# truapi-dts.ts itself.
if [ "${TRUAPI_SKIP_PACKAGE_BUILD:-0}" != "1" ]; then
  # npm workspaces hoist node_modules to the repo root, so check there.
  if [ ! -d node_modules ]; then
    npm ci
  fi
  npm run build --prefix js/packages/truapi
  node scripts/bundle-truapi-dts.mjs
  npm run build --prefix js/packages/truapi
fi

echo "Generated client at js/packages/truapi/src/generated/"
echo "Generated playground metadata at js/packages/truapi/src/playground/codegen/"
echo "Generated client examples at playground/test/generated/examples/"
echo "Generated Rust dispatcher at rust/crates/truapi-server/src/generated/"
echo "Generated host-callbacks WASM adapter at js/packages/truapi-host-wasm/src/generated/"
echo "Generated Rust WASM bridge at rust/crates/truapi-server/src/wasm/generated_bridge.rs"
