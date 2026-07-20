#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

codegen_required=(
  "js/packages/truapi/src/generated/client.ts"
  "js/packages/truapi/src/generated/types.ts"
  "js/packages/truapi/src/generated/wire-table.ts"
  "js/packages/truapi/src/playground/codegen/services.ts"
  "js/packages/truapi/src/explorer/codegen/types.ts"
  "js/packages/truapi/src/explorer/versions.ts"
)
truapi_dts="js/packages/truapi/src/playground/codegen/truapi-dts.ts"

missing=0
for path in "${codegen_required[@]}"; do
  if [ ! -f "$path" ]; then
    missing=1
    break
  fi
done

example_file="$(find playground/test/generated/examples -type f -name '*.ts' -print -quit 2>/dev/null || true)"
if [ "$missing" -eq 1 ] || [ -z "$example_file" ]; then
  if [ "${TRUAPI_REQUIRE_GENERATED:-0}" = "1" ]; then
    echo "ensure-generated: generated files are missing and TRUAPI_REQUIRE_GENERATED=1, so codegen will not run." >&2
    echo "These files are expected to be restored from the 'codegen-output' CI artifact." >&2
    echo "If you added a generated output, add its path to the upload-artifact step in .github/workflows/ci.yml." >&2
    exit 1
  fi

  TRUAPI_SKIP_PACKAGE_BUILD=1 ./scripts/codegen.sh
fi

if [ -f "$truapi_dts" ]; then
  exit 0
fi

if [ "${TRUAPI_REQUIRE_GENERATED:-0}" = "1" ]; then
  echo "ensure-generated: generated files are missing and TRUAPI_REQUIRE_GENERATED=1, so codegen will not run." >&2
  echo "These files are expected to be restored from the 'codegen-output' CI artifact." >&2
  echo "If you added a generated output, add its path to the upload-artifact step in .github/workflows/ci.yml." >&2
  exit 1
fi

if [ -x node_modules/.bin/tsc ]; then
  tsc_bin="node_modules/.bin/tsc"
elif [ -x js/packages/truapi/node_modules/.bin/tsc ]; then
  tsc_bin="js/packages/truapi/node_modules/.bin/tsc"
else
  echo "ensure-generated: cannot find tsc. Run npm install from the repo root first." >&2
  exit 1
fi

"$tsc_bin" -b js/packages/truapi --force
node scripts/bundle-truapi-dts.mjs
