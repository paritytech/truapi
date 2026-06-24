#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

required=(
  "js/packages/truapi-host/src/generated/server.ts"
  "js/packages/truapi-host/src/generated/types-by-version.ts"
  "js/packages/truapi-host/src/generated/host-callbacks.ts"
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

if [ "${TRUAPI_REQUIRE_GENERATED:-0}" = "1" ]; then
  echo "ensure-generated: generated files are missing and TRUAPI_REQUIRE_GENERATED=1, so codegen will not run." >&2
  echo "These files are expected to be restored from the 'codegen-output' CI artifact." >&2
  echo "If you added a generated output, add its path to the upload-artifact step in .github/workflows/ci.yml." >&2
  exit 1
fi

TRUAPI_SKIP_PACKAGE_BUILD=1 ./scripts/codegen.sh
