#!/usr/bin/env bash
# Regenerate js/packages/truapi/src/generated/* from rust/crates/truapi.
#
# Pipeline:
#   1. cargo +nightly rustdoc -p truapi --output-format json -> target/doc/truapi.json
#   2. cargo run -p truapi-codegen -- --input target/doc/truapi.json
#                                     --output js/packages/truapi/src/generated
#                                     --playground-output js/packages/truapi/src/playground
#                                     --explorer-output js/packages/truapi/src/explorer
#                                     --version V2
#                                     --codec-version 1
#
# Run from the repo root.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --playground-output js/packages/truapi/src/playground \
  --explorer-output js/packages/truapi/src/explorer \
  --version V2 \
  --codec-version 1

npm exec --yes -- prettier --write \
  "js/packages/truapi/src/generated/**/*.ts" \
  "js/packages/truapi/src/playground/**/*.ts" \
  "js/packages/truapi/src/explorer/**/*.ts"

# Rebuild dist/ so downstream consumers (in particular the playground,
# which picks up @parity/truapi via yarn 1.x file: snapshot) see the
# regenerated bindings without a separate npm run build step.
npm run build --prefix js/packages/truapi

echo "Generated client at js/packages/truapi/src/generated/"
echo "Generated playground metadata at js/packages/truapi/src/playground/"
echo "Generated explorer registry at js/packages/truapi/src/explorer/"
