#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

required=(
  "js/packages/truapi-host/src/generated/server.ts"
  "js/packages/truapi-host/src/generated/types-by-version.ts"
)

missing=0
for path in "${required[@]}"; do
  if [ ! -f "$path" ]; then
    missing=1
    break
  fi
done

if [ "$missing" -eq 0 ]; then
  exit 0
fi

TRUAPI_SKIP_PACKAGE_BUILD=1 ./scripts/codegen.sh
