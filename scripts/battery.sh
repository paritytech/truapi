#!/usr/bin/env bash
# Run the generated full-surface battery against the headless truapi-host CLI,
# built from source, and write both committed CLI diagnosis reports:
#
#   explorer/diagnosis-reports/signing-host-cli.md   direct signing-host run
#   explorer/diagnosis-reports/pairing-host-cli.md   pairing host, paired with a
#                                                    signing host this script starts
#
# Usage:
#   scripts/battery.sh                    # both phases
#   scripts/battery.sh --signing-host     # direct phase only
#   scripts/battery.sh --pairing-host     # paired phase only
#   scripts/battery.sh --release          # build and run the release binary
#   scripts/battery.sh -- --network foo   # arguments after `--` go to every host process
#
# Environment:
#   E2E_LIVE_CHAIN=1              route Chain/* at the network's real nodes
#   HOST_CLI_SIGNER_MNEMONIC      reuse a known signer instead of an auto-managed account
#   BATTERY_PHASE_TIMEOUT         seconds before a phase is killed (default 900)
#   BATTERY_PAIRING_TIMEOUT       seconds to wait for the pairing link (default 120)
#   TRUAPI_BATTERY_REPORT_PATH    override the report destination; single phase only
#
# Each phase exports TRUAPI_APPROVALS_LOG (under target/battery/) so the hosts
# record every consulted confirmation; the battery's AutoSigning e2e case reads
# it to prove sign_vrf runs prompt-free once AutoSigning is allocated.
#
# Each phase exits nonzero when pairing/bootstrap fails or when a generated
# example fails outside the committed unsupported baseline. Unsupported service
# families still appear as failures in the Markdown reports.
#
# Host transcripts land in target/battery/. The paired phase runs its pairing
# host on a throwaway identity under target/battery/pairing-host-state so every
# run performs a real handshake instead of restoring an earlier session.
#
# Run from anywhere; the script operates on its own checkout.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Homebrew LLVM on DYLD_LIBRARY_PATH makes rustc and rustdoc load a mismatched
# libLLVM, which dies with SIGSEGV in initialize_available_targets.
unset DYLD_LIBRARY_PATH

SCRIPT="rust/crates/truapi-host-cli/js/scripts/battery.ts"
PRODUCT_ID="truapi-playground.dot"
REPORTS="explorer/diagnosis-reports"
LOG_DIR="target/battery"
PAIRING_STATE="target/battery/pairing-host-state"
PHASE_TIMEOUT="${BATTERY_PHASE_TIMEOUT:-900}"
PAIRING_TIMEOUT="${BATTERY_PAIRING_TIMEOUT:-120}"
PROFILE_DIR="debug"
CARGO_ARGS=()
HOST_ARGS=()
RUN_SIGNING=1
RUN_PAIRING=1

usage() {
  awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --signing-host) RUN_PAIRING=0 ;;
    --pairing-host) RUN_SIGNING=0 ;;
    --release) CARGO_ARGS+=(--release); PROFILE_DIR="release" ;;
    --product-id)
      [ $# -ge 2 ] || { echo "battery: --product-id needs a value" >&2; exit 2; }
      PRODUCT_ID="$2"
      shift
      ;;
    --) shift; HOST_ARGS+=("$@"); break ;;
    -h|--help) usage; exit 0 ;;
    *) echo "battery: unknown option '$1' (pass host flags after '--')" >&2; exit 2 ;;
  esac
  shift
done

if [ -n "${TRUAPI_BATTERY_REPORT_PATH:-}" ] && [ "$RUN_SIGNING" = 1 ] && [ "$RUN_PAIRING" = 1 ]; then
  echo "battery: TRUAPI_BATTERY_REPORT_PATH would make both phases write the same file;" >&2
  echo "         pass --signing-host or --pairing-host to run one of them" >&2
  exit 2
fi

# The battery imports the generated example manifest and the playground's
# example runner, so both the codegen output and the playground's dependency
# tree have to exist before a host starts.
generated=(
  "js/packages/truapi/src/generated/client.ts"
  "js/packages/truapi/src/playground/codegen/services.ts"
)
for path in "${generated[@]}"; do
  if [ ! -f "$path" ]; then
    echo "battery: regenerating client ($path missing)"
    ./scripts/codegen.sh
    break
  fi
done

# The playground resolves @parity/truapi through its built dist, so a checkout
# with generated sources but no build still needs one.
if [ ! -f "js/packages/truapi/dist/index.js" ]; then
  echo "battery: building @parity/truapi"
  [ -d node_modules ] || npm ci --ignore-scripts
  npm run build --prefix js/packages/truapi
fi

# The playground pulls @parity/truapi in over yarn's `link:` protocol, which
# bun does not resolve, so this install has to stay on yarn.
if [ ! -f "playground/node_modules/@parity/truapi/package.json" ]; then
  echo "battery: installing playground dependencies"
  ( cd playground && yarn install --frozen-lockfile )
fi

# The paired phase runs two hosts at once, so build once up front instead of
# letting concurrent `cargo run` invocations queue on the build lock.
cargo build ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} -p truapi-host-cli
HOST="target/$PROFILE_DIR/truapi-host"
mkdir -p "$LOG_DIR"

# Kill the phase's host after PHASE_TIMEOUT so a stalled pairing or a hung
# provider cannot park the script forever. The pid lands in GUARD_PID rather
# than on stdout, because a command substitution would wait for the backgrounded
# subshell to close the capture pipe.
GUARD_PID=""
start_watchdog() {
  local pid="$1" label="$2"
  (
    sleep "$PHASE_TIMEOUT"
    if kill -0 "$pid" 2>/dev/null; then
      echo "battery: $label exceeded ${PHASE_TIMEOUT}s, terminating" >&2
      kill -TERM "$pid" 2>/dev/null || true
    fi
  ) &
  GUARD_PID=$!
}

stop_watchdog() {
  [ -n "$GUARD_PID" ] || return 0
  kill "$GUARD_PID" 2>/dev/null || true
  GUARD_PID=""
}

stop_host() {
  local pid="$1"
  kill -0 "$pid" 2>/dev/null || return 0
  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 10); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 1
  done
  kill -KILL "$pid" 2>/dev/null || true
}

SIGNER_PID=""
cleanup() {
  stop_watchdog
  if [ -n "$SIGNER_PID" ]; then
    stop_host "$SIGNER_PID"
  fi
}
trap cleanup EXIT

signing_phase() {
  local log="$LOG_DIR/signing-host-cli.log"
  local report="$ROOT/$REPORTS/signing-host-cli.md"
  echo "battery: signing-host phase (report $REPORTS/signing-host-cli.md)"
  # Consulted-approval transcript backing the AutoSigning prompt-free check.
  export TRUAPI_APPROVALS_LOG="$LOG_DIR/signing-host-approvals.log"
  rm -f "$TRUAPI_APPROVALS_LOG"
  TRUAPI_BATTERY_REPORT_PATH="${TRUAPI_BATTERY_REPORT_PATH:-$report}" \
    "$HOST" signing-host \
    --product-id "$PRODUCT_ID" \
    --script "$SCRIPT" \
    --auto-accept \
    ${HOST_ARGS[@]+"${HOST_ARGS[@]}"} > >(tee "$log") 2>&1 &
  local host_pid=$! rc=0
  start_watchdog "$host_pid" "signing-host phase"
  wait "$host_pid" || rc=$?
  stop_watchdog
  return "$rc"
}

pairing_phase() {
  local log="$LOG_DIR/pairing-host-cli.log"
  local signer_log="$LOG_DIR/pairing-host-cli-signer.log"
  local report="$ROOT/$REPORTS/pairing-host-cli.md"
  echo "battery: pairing-host phase (report $REPORTS/pairing-host-cli.md)"
  : > "$log"
  # A pairing host that restores an earlier session skips the handshake and
  # reports AlreadyConnected, then every remote example fails against the signing
  # host that session was paired with and is no longer running. Start each run
  # from an empty pairing identity so the handshake is always exercised. The
  # signing host keeps the default base path, so it reuses its attested account.
  rm -rf "$PAIRING_STATE"
  # Consulted-approval transcript backing the AutoSigning prompt-free check.
  # The answering signing host inherits the same path, so a VRF prompt on
  # either side of the pair lands in the same file.
  export TRUAPI_APPROVALS_LOG="$LOG_DIR/pairing-host-approvals.log"
  rm -f "$TRUAPI_APPROVALS_LOG"
  TRUAPI_BATTERY_REPORT_PATH="${TRUAPI_BATTERY_REPORT_PATH:-$report}" \
    "$HOST" pairing-host \
    --base-path "$PAIRING_STATE" \
    --product-id "$PRODUCT_ID" \
    --script "$SCRIPT" \
    --auto-accept \
    ${HOST_ARGS[@]+"${HOST_ARGS[@]}"} > >(tee "$log") 2>&1 &
  local host_pid=$! rc=0
  start_watchdog "$host_pid" "pairing-host phase"

  # The pairing host publishes a polkadotapp://pair link and then waits for a
  # signer, so answer it with a second host from this side.
  local deeplink="" waited=0 paired=0
  while [ "$waited" -lt "$PAIRING_TIMEOUT" ]; do
    deeplink="$(grep -o 'polkadotapp://pair?[^[:space:]]*' "$log" | head -1 || true)"
    [ -n "$deeplink" ] && break
    if grep -q "Paired with" "$log"; then
      paired=1
      break
    fi
    kill -0 "$host_pid" 2>/dev/null || break
    sleep 1
    waited=$((waited + 1))
  done

  if [ -n "$deeplink" ]; then
    echo "battery: answering pairing link with a signing host"
    "$HOST" signing-host \
      --auto-accept \
      --product-id "$PRODUCT_ID" \
      --frame-listen 127.0.0.1:0 \
      ${HOST_ARGS[@]+"${HOST_ARGS[@]}"} \
      exec "/pair $deeplink" > >(tee "$signer_log" | sed 's/^/[signer] /') 2>&1 &
    SIGNER_PID=$!
  elif [ "$paired" = 1 ]; then
    echo "battery: pairing host restored a session; no link to answer" >&2
  else
    echo "battery: no pairing link after ${PAIRING_TIMEOUT}s (see $log)" >&2
    stop_watchdog
    stop_host "$host_pid"
    wait "$host_pid" 2>/dev/null || true
    return 1
  fi

  wait "$host_pid" || rc=$?
  stop_watchdog
  if [ -n "$SIGNER_PID" ]; then
    stop_host "$SIGNER_PID"
    SIGNER_PID=""
  fi
  return "$rc"
}

SIGNING_RC="skipped"
PAIRING_RC="skipped"

if [ "$RUN_SIGNING" = 1 ]; then
  SIGNING_RC=0
  signing_phase || SIGNING_RC=$?
fi

if [ "$RUN_PAIRING" = 1 ]; then
  PAIRING_RC=0
  pairing_phase || PAIRING_RC=$?
fi

echo
echo "battery: signing-host exit=$SIGNING_RC · pairing-host exit=$PAIRING_RC"
echo "battery: reports under $REPORTS/, logs under $LOG_DIR/"
[ "$SIGNING_RC" = 0 ] || [ "$SIGNING_RC" = "skipped" ] || exit "$SIGNING_RC"
[ "$PAIRING_RC" = 0 ] || [ "$PAIRING_RC" = "skipped" ] || exit "$PAIRING_RC"
