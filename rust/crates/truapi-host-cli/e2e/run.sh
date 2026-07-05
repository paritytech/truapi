#!/usr/bin/env bash
# Build the headless hosts and run the end-to-end pairing + signing test.
#
#   run.sh              curated signer battery (deterministic gate)
#   E2E_DIAGNOSIS=1 run.sh   full playground diagnosis (gated on signer methods)
#
# Prerequisites for the JS driver (one-time):
#   ./scripts/codegen.sh   (or generate js/packages/truapi/src/generated)
#   (cd js/packages/truapi && bun install && bunx tsc -b)
#   (cd playground && bun install)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

cargo build -p truapi-host-cli
exec bun rust/crates/truapi-host-cli/e2e/run-e2e.ts
