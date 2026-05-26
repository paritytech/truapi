#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

required=(
  "js/packages/truapi/src/generated/client.ts"
  "js/packages/truapi/src/generated/types.ts"
  "js/packages/truapi/src/generated/wire-table.ts"
  "js/packages/truapi/src/playground/codegen/services.ts"
  "js/packages/truapi/src/explorer/codegen/types.ts"
)

missing=0
for path in "${required[@]}"; do
  if [ ! -f "$path" ]; then
    missing=1
    break
  fi
done

if [ "$missing" -eq 0 ] && find playground/test/generated/examples -name '*.ts' -print -quit >/dev/null 2>&1; then
  exit 0
fi

TRUAPI_SKIP_PACKAGE_BUILD=1 ./scripts/codegen.sh
