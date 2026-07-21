#!/usr/bin/env bash
# Full-bundle driver: the real playground bundle, served statically and run
# browserless under bun + happy-dom, drives a pairing host through the
# MessagePort->WS bridge; a signing host answers the pairing deeplink; every
# operation lands in METRICS_JSONL.
#
#   make headless && e2e/full-bundle.sh
#
# Env:
#   PRODUCT_ID               product id the pairing host serves (default truapi-playground.dot)
#   FRAME                    frame-server address (default 127.0.0.1:9955)
#   METRICS_JSONL            metrics sink (default /tmp/full-bundle-metrics.jsonl)
#   SKIP_BUILD               set to reuse an existing playground/out
#   HOST_CLI_SIGNER_MNEMONIC / TRUAPI_HOST_BASE_PATH  as in run.sh (e2e/.env supported)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
BIN="$ROOT/target/debug/truapi-host"
ENV_FILE="$(dirname "$0")/.env"
[ -f "$ENV_FILE" ] && { set -a; . "$ENV_FILE"; set +a; }

PRODUCT_ID="${PRODUCT_ID:-truapi-playground.dot}"
FRAME="${FRAME:-127.0.0.1:9955}"
export METRICS_JSONL="${METRICS_JSONL:-/tmp/full-bundle-metrics.jsonl}"

[ -x "$BIN" ] || { echo "missing $BIN — run: make headless" >&2; exit 2; }

if [ -z "${SKIP_BUILD:-}" ] || [ ! -d "$ROOT/playground/out" ]; then
  (cd "$ROOT/js/packages/truapi" && npm run build)
  (cd "$ROOT/playground" && yarn build)
fi

LOG="$(mktemp)"
PAIR_PID=""
SIGNER_PID=""
WATCHER_PID=""
KEEPALIVE_PID=""
cleanup() {
  [ -n "$WATCHER_PID" ] && kill "$WATCHER_PID" 2>/dev/null || true
  [ -n "$SIGNER_PID" ] && kill "$SIGNER_PID" 2>/dev/null || true
  # Independent of $SIGNER_PID: if the script exits before the main flow
  # reaps the signer (e.g. an early/interrupted exit), this is the only path
  # left that still catches it.
  if [ -f "$LOG.signer" ]; then
    kill "$(cat "$LOG.signer")" 2>/dev/null || true
    rm -f "$LOG.signer"
  fi
  if [ -n "$PAIR_PID" ]; then
    pkill -TERM -P "$PAIR_PID" 2>/dev/null || true
    kill -TERM "$PAIR_PID" 2>/dev/null || true
  fi
  # The keepalive tail runs under a process-substitution wrapper shell; kill
  # the wrapper's children before the wrapper itself (killing only the wrapper
  # reparents the tail to init and leaks it — verified empirically).
  if [ -n "$KEEPALIVE_PID" ]; then
    pkill -TERM -P "$KEEPALIVE_PID" 2>/dev/null || true
    kill -TERM "$KEEPALIVE_PID" 2>/dev/null || true
  fi
  # Success path removes $LOG itself before this trap runs; if it's still
  # here, this exit was a failure -- keep it for post-mortem instead of
  # silently deleting the only evidence.
  [ -f "$LOG" ] && echo "host log preserved: $LOG" >&2
}
trap cleanup EXIT

: > "$METRICS_JSONL"
# stdin keepalive: with </dev/null the interactive loop sees EOF, returns, and
# with_frame_server aborts the frame server (verified in Task 3). tail -f
# keeps stdin open without ever writing to it. The substitution is exec'd onto
# fd 3 instead of inlined on the host command so its pid lands in $! and the
# EXIT trap can reap it — inlined, the tail outlives cleanup as an orphan.
exec 3< <(tail -f /dev/null)
KEEPALIVE_PID=$!
"$BIN" pairing-host --product-id "$PRODUCT_ID" --frame-listen "$FRAME" \
  --auto-accept <&3 > >(tee "$LOG") 2>&1 &
PAIR_PID=$!

# Don't race the shim run against a socket that isn't bound yet: wait
# for the host's own "listening" line, checking every 0.5s for up to 60s that
# it hasn't died in the meantime.
for _ in $(seq 1 120); do
  grep -q '^FRAMES_LISTENING' "$LOG" && break
  kill -0 "$PAIR_PID" 2>/dev/null || { echo "pairing host exited before FRAMES_LISTENING" >&2; exit 1; }
  sleep 0.5
done
grep -q '^FRAMES_LISTENING' "$LOG" || { echo "pairing host: no FRAMES_LISTENING within 60s" >&2; exit 1; }

# The deeplink appears only once the bundle calls requestLogin, so watch for
# it concurrently with the shim run and answer with the signing host.
(
  for _ in $(seq 1 600); do
    deeplink="$(grep -m1 -oE 'PAIRING_DEEPLINK .+' "$LOG" | cut -d' ' -f2- || true)"
    if [ -n "$deeplink" ]; then
      "$BIN" signing-host --deeplink "$deeplink" --auto-accept &
      echo "$!" > "$LOG.signer"
      exit 0
    fi
    sleep 0.5
  done
  echo "watcher: no deeplink within 300s" >&2
) &
WATCHER_PID=$!

STATUS=0
(cd "$ROOT/rust/crates/truapi-host-cli/js" && { [ -d node_modules ] || bun install; } \
  && TRUAPI_FRAME_URL="ws://$FRAME" FULL_BUNDLE_DEADLINE_MS="${FULL_BUNDLE_DEADLINE_MS:-1200000}" \
     bun scripts/load-bundle.ts) || STATUS=$?
[ -f "$LOG.signer" ] && SIGNER_PID="$(cat "$LOG.signer")" && rm -f "$LOG.signer"

[ "$STATUS" -eq 0 ] || exit "$STATUS"
[ -s "$METRICS_JSONL" ] || { echo "no metrics recorded in $METRICS_JSONL" >&2; exit 1; }
# A single "category":"signing" grep passes even if every signing op failed.
# Gate on each battery-critical op individually succeeding (op field always
# precedes outcome in HostMetricRecord's field order, so one pattern per line
# suffices -- see rust/crates/truapi-host-cli/src/metrics.rs).
for op in signing_sign_raw signing_sign_payload signing_create_transaction; do
  grep -q "\"op\":\"$op\".*\"outcome\":\"success\"" "$METRICS_JSONL" || {
    echo "no successful $op recorded in $METRICS_JSONL" >&2; exit 1; }
done
echo "full-bundle spike OK -> $METRICS_JSONL ($(wc -l < "$METRICS_JSONL") records)"
rm -f "$LOG"
