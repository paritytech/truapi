#!/usr/bin/env bash
# Fleet runner (first slice): N virtual users, each a pairing host on its own
# port, paired against the signing-bot (the persona pool), started on a ramp,
# all emitting per-VU metrics (distinct VU_INDEX) to one shared JSONL.
#
#   VUS=3 RAMP=3 e2e/fleet.sh                 # 3 VUs, 3s apart, battery.ts each
#   SCRIPT=path/to/flow.ts VUS=5 e2e/fleet.sh
#
# Env:
#   VUS                  number of virtual users (default 3)
#   RAMP                 seconds between VU starts (default 3)
#   SCRIPT               product script each VU runs (default battery.ts)
#   PRODUCT_ID           product id each VU serves (default headless-playground.dot)
#   BASE_PORT            first frame-server port; VU i uses BASE_PORT+i (default 9955)
#   SIGNER_BOT_BASE_URL  signing-bot base URL (default http://localhost:3737)
#   SIGNER_BOT_NETWORK   pairing network (default paseo-next-v2)
#   SIGNER_BOT_SVC_TOKEN bearer token; sent only if set (a local dev bot needs none)
#   RUN_ID               shared run id (default fleet-<epoch>)
#   METRICS_JSONL        shared metrics sink (default /tmp/fleet-metrics.jsonl)
#
# Each VU pairs against the bot, which auto-provisions an attested user and
# signs, so scale is bounded by the bot's per-user attestation, not the host.
# The default target is an unauthenticated local bot; point it at an
# authenticated one by setting SIGNER_BOT_BASE_URL and SIGNER_BOT_SVC_TOKEN.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
BIN="$ROOT/target/debug/truapi-host"
SCRIPT="${SCRIPT:-$ROOT/rust/crates/truapi-host-cli/js/scripts/battery.ts}"
VUS="${VUS:-3}"
RAMP="${RAMP:-3}"
PRODUCT_ID="${PRODUCT_ID:-headless-playground.dot}"
BASE_PORT="${BASE_PORT:-9955}"
BOT="${SIGNER_BOT_BASE_URL:-http://localhost:3737}"
NETWORK="${SIGNER_BOT_NETWORK:-paseo-next-v2}"
export RUN_ID="${RUN_ID:-fleet-$(date +%s)}"
export METRICS_JSONL="${METRICS_JSONL:-/tmp/fleet-metrics.jsonl}"

[ -x "$BIN" ] || { echo "missing $BIN — build first (cargo build -p truapi-host-cli)" >&2; exit 2; }

# Per-VU host pids and logs live here; the trap kills leaked hosts and clears
# the logs on any exit, including Ctrl-C mid-ramp.
WORKDIR="$(mktemp -d)"
cleanup() {
  for f in "$WORKDIR"/*.pid; do
    [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null || true
  done
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

: > "$METRICS_JSONL"

# One VU: start its pairing host, hand the deeplink to the bot, wait for it.
run_vu() {
  local i="$1" port="$2" log="$WORKDIR/vu-$1.log"
  VU_INDEX="$i" "$BIN" pairing-host --product-id "$PRODUCT_ID" \
    --script "$SCRIPT" --frame-listen "127.0.0.1:$port" --auto-accept >"$log" 2>&1 &
  local ph=$!
  echo "$ph" >"$WORKDIR/vu-$i.pid"
  local deeplink=""
  for _ in $(seq 1 240); do
    deeplink="$(grep -m1 -oE 'PAIRING_DEEPLINK .+' "$log" | cut -d' ' -f2- || true)"
    [ -n "$deeplink" ] && break
    kill -0 "$ph" 2>/dev/null || break
    sleep 0.5
  done
  if [ -z "$deeplink" ]; then
    echo "VU$i: no deeplink (host died?)"; tail -3 "$log"; return 1
  fi
  local auth=()
  if [ -n "${SIGNER_BOT_SVC_TOKEN:-}" ]; then
    auth=(-H "authorization: Bearer $SIGNER_BOT_SVC_TOKEN")
  fi
  curl -s -m 180 -X POST "$BOT/api/pair" -H 'content-type: application/json' \
    ${auth[@]+"${auth[@]}"} \
    -d "{\"handshake\":\"$deeplink\",\"network\":\"$NETWORK\"}" -o /dev/null \
    -w "VU$i: bot /api/pair=%{http_code}\n" || true
  local rc=0
  wait "$ph" || rc=$?
  echo "VU$i: pairing host exit=$rc"
}

echo "fleet: VUS=$VUS ramp=${RAMP}s script=$(basename "$SCRIPT") run_id=$RUN_ID"
pids=()
for i in $(seq 0 $((VUS - 1))); do
  run_vu "$i" "$((BASE_PORT + i))" &
  pids+=($!)
  sleep "$RAMP"
done
for p in "${pids[@]}"; do wait "$p" || true; done
echo "fleet complete -> $METRICS_JSONL"
