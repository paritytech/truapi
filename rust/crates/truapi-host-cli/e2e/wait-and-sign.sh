#!/usr/bin/env bash
# Signing-host side of the two-pane demo: wait for the pairing host (run by
# the orchestrator in E2E_HANDOFF_FILE mode) to emit the relay URL + deeplink,
# then answer the handshake and serve the SSO session.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

HANDOFF="${1:-/tmp/truapi-e2e-handoff.txt}"
BIN="target/debug/truapi-host"

echo "SIGNING PANE: waiting for pairing host to present a deeplink ($HANDOFF)..."
while [ ! -s "$HANDOFF" ]; do sleep 0.2; done

{ read -r RELAY; read -r DEEPLINK; } < "$HANDOFF"
echo "SIGNING PANE: got deeplink, answering pairing on $RELAY"
echo
exec "$BIN" signing-host --relay "$RELAY" --deeplink "$DEEPLINK"
