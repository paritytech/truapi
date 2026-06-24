#!/usr/bin/env bash
# Regenerate generated TrUAPI artifacts from rust/crates/truapi plus host
# callback TypeScript from rust/crates/truapi-platform.
#
# Pipeline:
#   1. cargo +$RUSTDOC_TOOLCHAIN rustdoc -p truapi --output-format json -> target/doc/truapi.json
#   2. cargo +$RUSTDOC_TOOLCHAIN rustdoc -p truapi-platform --output-format json -> target/doc/truapi_platform.json
#   3. cargo run -p truapi-codegen -- --input target/doc/truapi.json
#                                     --output js/packages/truapi/src/generated
#                                     --playground-output js/packages/truapi/src/playground
#                                     --client-examples-output playground/test/generated/examples
#                                     --host-output js/packages/truapi-host/src/generated
#                                     --rust-output rust/crates/truapi-server/src/generated
#                                     --platform-input target/doc/truapi_platform.json
#                                     --platform-ts-output js/packages/truapi-host/src/generated
#                                     --platform-wasm-adapter-output js/packages/truapi-host-wasm/src/generated
#                                     --explorer-output js/packages/truapi/src/explorer
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

# rustdoc JSON output requires nightly. `cargo +nightly` is preferred (CI
# installs a fresh one), but a given day's rolling nightly can be broken on a
# platform (e.g. an LLVM-init SIGSEGV). Probe for a usable toolchain and fall
# back to the newest installed dated `nightly-YYYY-MM-DD` that actually runs.
# Set RUSTDOC_TOOLCHAIN to pin one explicitly and skip the probe.
toolchain_runs() {
  rustc "+$1" --crate-name probe --crate-type cdylib --print=file-names - \
    </dev/null >/dev/null 2>&1
}

select_rustdoc_toolchain() {
  if [ -n "${RUSTDOC_TOOLCHAIN:-}" ]; then
    echo "$RUSTDOC_TOOLCHAIN"
    return
  fi
  local candidate
  for candidate in nightly $(rustup toolchain list 2>/dev/null \
    | grep -oE '^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}' | sort -ru); do
    if toolchain_runs "$candidate"; then
      echo "$candidate"
      return
    fi
  done
  echo "no working nightly toolchain found; install one with \`rustup toolchain install nightly\` or set RUSTDOC_TOOLCHAIN" >&2
  exit 1
}

RUSTDOC_TOOLCHAIN="$(select_rustdoc_toolchain)"
echo "Using rustdoc toolchain: $RUSTDOC_TOOLCHAIN"

cargo "+$RUSTDOC_TOOLCHAIN" rustdoc -p truapi -- -Z unstable-options --output-format json
cargo "+$RUSTDOC_TOOLCHAIN" rustdoc -p truapi-platform -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --playground-output js/packages/truapi/src/playground \
  --client-examples-output playground/test/generated/examples \
  --host-output js/packages/truapi-host/src/generated \
  --rust-output rust/crates/truapi-server/src/generated \
  --platform-input target/doc/truapi_platform.json \
  --platform-ts-output js/packages/truapi-host/src/generated \
  --platform-wasm-adapter-output js/packages/truapi-host-wasm/src/generated \
  --explorer-output js/packages/truapi/src/explorer \
  --codec-version 1

rm -f js/packages/truapi-host-wasm/src/generated/host-callbacks.ts

rustfmt --edition 2024 rust/crates/truapi-server/src/generated/*.rs

node scripts/regen-explorer-versions.mjs

npm exec --yes -- prettier --write \
  "js/packages/truapi/src/generated/**/*.ts" \
  "js/packages/truapi/src/playground/**/*.ts" \
  "js/packages/truapi/src/explorer/**/*.ts" \
  "playground/test/generated/examples/**/*.ts" \
  "js/packages/truapi-host/src/generated/**/*.ts" \
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
  npm run build --prefix js/packages/truapi-host
  npm run build --prefix js/packages/truapi-host-wasm
fi

echo "Generated client at js/packages/truapi/src/generated/"
echo "Generated playground metadata at js/packages/truapi/src/playground/codegen/"
echo "Generated client examples at playground/test/generated/examples/"
echo "Generated host package at js/packages/truapi-host/src/generated/"
echo "Generated Rust dispatcher at rust/crates/truapi-server/src/generated/"
echo "Generated host-callbacks at js/packages/truapi-host/src/generated/"
echo "Generated host-callbacks WASM adapter at js/packages/truapi-host-wasm/src/generated/"
