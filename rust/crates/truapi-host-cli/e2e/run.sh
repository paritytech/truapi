#!/usr/bin/env bash
# Headless end-to-end run: a pairing host drives a product script against a
# signing host, pairing over the real People-chain statement store.
#
#   make headless                  # build once
#   e2e/run.sh                     # runs js/scripts/battery.ts (default)
#   e2e/run.sh path/to/script.ts   # runs a custom product script
#
# Env:
#   PRODUCT_ID               product id the pairing host serves (default headless-playground.dot)
#   HOST_CLI_SIGNER_MNEMONIC wallet mnemonic for the signing host (default: dev mnemonic)
#   FRAME                    frame-server address (default 127.0.0.1:9955)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
BIN="$ROOT/target/debug/truapi-host"
SCRIPT="${1:-$ROOT/rust/crates/truapi-host-cli/js/scripts/battery.ts}"
PRODUCT_ID="${PRODUCT_ID:-headless-playground.dot}"
FRAME="${FRAME:-127.0.0.1:9955}"

# Load HOST_CLI_SIGNER_MNEMONIC (and any other vars) from a gitignored e2e/.env
# if present, so the signing host uses a registered account.
ENV_FILE="$(dirname "$0")/.env"
[ -f "$ENV_FILE" ] && { set -a; . "$ENV_FILE"; set +a; }

[ -x "$BIN" ] || { echo "missing $BIN — run: make headless" >&2; exit 2; }

LOG="$(mktemp)"
SIGNER_PID=""
cleanup() {
  [ -n "$SIGNER_PID" ] && kill "$SIGNER_PID" 2>/dev/null || true
  rm -f "$LOG"
}
trap cleanup EXIT

# The pairing host runs the product script; the script's
# `truapi.account.requestLogin` makes the host emit a pairing deeplink, which we
# hand to a signing host. The pairing host exits with the script's status.
"$BIN" pairing-host --product-id "$PRODUCT_ID" --script "$SCRIPT" \
  --frame-listen "$FRAME" --auto-accept 2>&1 | tee "$LOG" &
PAIR_PID=$!

deeplink=""
for _ in $(seq 1 600); do
  deeplink="$(grep -m1 -oE 'PAIRING_DEEPLINK .+' "$LOG" | cut -d' ' -f2- || true)"
  [ -n "$deeplink" ] && break
  kill -0 "$PAIR_PID" 2>/dev/null || break
  sleep 0.5
done
[ -n "$deeplink" ] || { echo "pairing host did not emit a deeplink" >&2; exit 1; }

# The signing host reads HOST_CLI_SIGNER_MNEMONIC from the env (else the dev
# mnemonic). It must be a registered LitePeople ring member for allowance.
"$BIN" signing-host --deeplink "$deeplink" --auto-accept &
SIGNER_PID=$!

wait "$PAIR_PID"
