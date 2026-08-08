#!/usr/bin/env bash
# Headless end-to-end run: a pairing host drives a product script against a
# signing host, pairing over the real People-chain statement store.
#
#   make headless                  # build once
#   e2e/run.sh                     # generates pairing-host-cli.md (default)
#   e2e/run.sh path/to/script.ts   # runs a custom product script
#
# Env:
#   PRODUCT_ID               product id the pairing host serves (default truapi-playground.dot)
#   HOST_CLI_SIGNER_MNEMONIC optional wallet mnemonic; when unset, signing-host auto-manages one
#   TRUAPI_HOST_BASE_PATH    optional root for generated accounts and host state
#   TRUAPI_PAIRING_BASE_PATH optional pairing-host state root; defaults to a fresh temporary root
#   TRUAPI_E2E_LOAD_ENV      set to 0 to ignore the gitignored e2e/.env (default 1)
#   FRAME                    frame-server address (default 127.0.0.1:9955)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
BIN="$ROOT/target/debug/truapi-host"

# Load HOST_CLI_SIGNER_MNEMONIC / TRUAPI_HOST_BASE_PATH (and any other vars)
# from a gitignored e2e/.env if present.
ENV_FILE="$(dirname "$0")/.env"
if [ "${TRUAPI_E2E_LOAD_ENV:-1}" = 1 ] && [ -f "$ENV_FILE" ]; then
  set -a
  . "$ENV_FILE"
  set +a
fi

SCRIPT="${1:-$ROOT/rust/crates/truapi-host-cli/js/scripts/battery.ts}"
PRODUCT_ID="${PRODUCT_ID:-truapi-playground.dot}"
FRAME="${FRAME:-127.0.0.1:9955}"

PAIRING_BASE_PATH_OWNED=0
if [ -n "${TRUAPI_PAIRING_BASE_PATH:-}" ]; then
  PAIRING_BASE_PATH="$TRUAPI_PAIRING_BASE_PATH"
else
  PAIRING_BASE_PATH="$(mktemp -d /tmp/truapi-e2e-pairing.XXXXXX)"
  PAIRING_BASE_PATH_OWNED=1
fi

[ -x "$BIN" ] || { echo "missing $BIN — run: make headless" >&2; exit 2; }

LOG="$(mktemp)"
SIGNER_PID=""
PAIR_PID=""
stop_pairing_host() {
  [ -n "$PAIR_PID" ] || return 0
  pkill -TERM -P "$PAIR_PID" 2>/dev/null || true
  kill -TERM "$PAIR_PID" 2>/dev/null || true
  sleep 0.5
  pkill -KILL -P "$PAIR_PID" 2>/dev/null || true
  kill -KILL "$PAIR_PID" 2>/dev/null || true
}
cleanup() {
  [ -n "$SIGNER_PID" ] && kill "$SIGNER_PID" 2>/dev/null || true
  stop_pairing_host
  rm -f "$LOG"
  if [ "$PAIRING_BASE_PATH_OWNED" -eq 1 ]; then
    rm -rf -- "$PAIRING_BASE_PATH"
  fi
}
trap cleanup EXIT

# The pairing host runs the product script; the script's
# `truapi.account.requestLogin` makes the host emit a pairing deeplink, which we
# hand to a signing host. The pairing host exits with the script's status.
"$BIN" pairing-host --product-id "$PRODUCT_ID" --script "$SCRIPT" \
  --frame-listen "$FRAME" --base-path "$PAIRING_BASE_PATH" \
  --auto-accept > >(tee "$LOG") 2>&1 &
PAIR_PID=$!

deeplink=""
for _ in $(seq 1 600); do
  deeplink="$(grep -m1 -oE 'polkadotapp://pair\?handshake=[[:xdigit:]]+' "$LOG" || true)"
  [ -n "$deeplink" ] && break
  kill -0 "$PAIR_PID" 2>/dev/null || break
  sleep 0.5
done
[ -n "$deeplink" ] || { echo "pairing host did not emit a deeplink" >&2; exit 1; }

# The signing host reads HOST_CLI_SIGNER_MNEMONIC from the env when set.
# Otherwise it auto-selects or creates an attested account under its base path.
"$BIN" signing-host --auto-accept exec "/deeplink $deeplink" &
SIGNER_PID=$!

pid_running() {
  local stat
  stat="$(ps -p "$1" -o stat= 2>/dev/null || true)"
  [ -n "$stat" ] && [ "${stat#Z}" = "$stat" ]
}

while :; do
  if ! pid_running "$PAIR_PID"; then
    wait "$PAIR_PID"
    exit $?
  fi
  if ! pid_running "$SIGNER_PID"; then
    stop_pairing_host
    exit 1
  fi
  sleep 0.5
done
