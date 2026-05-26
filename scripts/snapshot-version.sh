#!/usr/bin/env bash
# Snapshots the current TrUAPI surface as a historical version archive for
# the explorer site.
#
# Reads the version string from js/packages/truapi/package.json (kept in sync
# with the truapi crate version) and writes the explorer's services + types
# snapshot to:
#
#   js/packages/truapi/src/explorer/codegen/versions/<version>/services.ts
#   js/packages/truapi/src/explorer/codegen/versions/<version>/types.ts
#
# The playground services.ts is regenerated with --strip-examples so historical
# archives stay small (the explorer doesn't render examples for non-main
# versions). After writing the snapshot, the registry at
# js/packages/truapi/src/explorer/versions.ts is rebuilt to include it.
#
# Usage:
#   scripts/snapshot-version.sh           # refuse if snapshot dir exists
#   scripts/snapshot-version.sh --force   # overwrite existing snapshot

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FORCE=0
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    -h|--help)
      sed -n '2,20p' "$0"
      exit 0
      ;;
    *)
      echo "snapshot-version: unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

VERSION="$(node -e 'console.log(require("./js/packages/truapi/package.json").version)')"
if [ -z "$VERSION" ]; then
  echo "snapshot-version: could not read version from js/packages/truapi/package.json" >&2
  exit 1
fi

SNAPSHOT_DIR="js/packages/truapi/src/explorer/codegen/versions/$VERSION"
if [ -d "$SNAPSHOT_DIR" ] && [ "$FORCE" -ne 1 ]; then
  echo "snapshot-version: $SNAPSHOT_DIR already exists. Pass --force to overwrite." >&2
  exit 1
fi

TMP_DIR="$(mktemp -d -t truapi-snapshot.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

cargo +nightly rustdoc -p truapi -- -Z unstable-options --output-format json >/dev/null

cargo run --quiet -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output "$TMP_DIR/generated" \
  --playground-output "$TMP_DIR/playground" \
  --explorer-output "$TMP_DIR/explorer" \
  --strip-examples \
  --codec-version 1

mkdir -p "$SNAPSHOT_DIR"
cp "$TMP_DIR/playground/codegen/services.ts" "$SNAPSHOT_DIR/services.ts"
cp "$TMP_DIR/explorer/codegen/types.ts"      "$SNAPSHOT_DIR/types.ts"

# Snapshot files import from their local directory, not from the live
# `../../../playground/...` path the playground codegen emits.
sed -i "s|from '../services-types.js'|from '../../../../playground/services-types.js'|" \
  "$SNAPSHOT_DIR/services.ts"
sed -i "s|from '../data-types.js'|from '../../../data-types.js'|" \
  "$SNAPSHOT_DIR/types.ts"

node "$ROOT/scripts/regen-explorer-versions.mjs"

npm exec --yes -- prettier --write \
  "$SNAPSHOT_DIR/services.ts" \
  "$SNAPSHOT_DIR/types.ts" \
  "js/packages/truapi/src/explorer/versions.ts" >/dev/null

echo "Snapshot $VERSION written to $SNAPSHOT_DIR"
