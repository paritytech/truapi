#!/usr/bin/env bash
# Regenerate js/packages/truapi/src/generated/* from rust/crates/truapi
# plus js/packages/truapi-host-shared/src/generated/* from rust/crates/truapi-platform.
#
# Pipeline:
#   1. cargo +nightly rustdoc -p truapi --output-format json -> target/doc/truapi.json
#   2. cargo +nightly rustdoc -p truapi-platform --output-format json -> target/doc/truapi_platform.json
#   3. cargo run -p truapi-codegen -- --input target/doc/truapi.json
#                                     --output js/packages/truapi/src/generated
#                                     --playground-output js/packages/truapi/src/playground
#                                     --client-examples-output js/packages/truapi/test/generated/examples
#                                     --host-output js/packages/truapi-host/src/generated
#                                     --platform-input target/doc/truapi_platform.json
#                                     --platform-ts-output js/packages/truapi-host-shared/src/generated
#                                     --codec-version 1
#
# The client surface defaults to the latest wire version any versioned
# wrapper exposes; pass `--client-version V<N>` to pin to an older one.
# The host package always covers every wire version a wrapper has shipped.
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
  --client-examples-output js/packages/truapi/test/generated/examples \
  --host-output js/packages/truapi-host/src/generated \
  --platform-input target/doc/truapi_platform.json \
  --platform-ts-output js/packages/truapi-host-shared/src/generated \
  --codec-version 1

npm exec --yes -- prettier --write \
  "js/packages/truapi/src/generated/**/*.ts" \
  "js/packages/truapi/src/playground/**/*.ts" \
  "js/packages/truapi/test/generated/examples/**/*.ts" \
  "js/packages/truapi-host/src/generated/**/*.ts" \
  "js/packages/truapi-host-shared/src/generated/**/*.ts"

# Rebuild dist/ so downstream consumers (in particular the playground,
# which picks up @parity/truapi via yarn 1.x file: snapshot) see the
# regenerated bindings without a separate npm run build step.
if [ "${TRUAPI_SKIP_PACKAGE_BUILD:-0}" != "1" ]; then
  if [ ! -d js/packages/truapi/node_modules ]; then
    npm ci --prefix js/packages/truapi
  fi
  npm run build --prefix js/packages/truapi
  if [ ! -d js/packages/truapi-host/node_modules ]; then
    npm install --prefix js/packages/truapi-host
  fi
  npm run build --prefix js/packages/truapi-host
fi

echo "Generated client at js/packages/truapi/src/generated/"
echo "Generated playground metadata at js/packages/truapi/src/playground/codegen/"
echo "Generated client examples at js/packages/truapi/test/generated/examples/"
echo "Generated host package at js/packages/truapi-host/src/generated/"
echo "Generated host-callbacks at js/packages/truapi-host-shared/src/generated/"
