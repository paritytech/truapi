# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi-server`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over the **real People-chain statement store** (the
same node an iOS/web client uses), so the playground's own tests can run against
a real signer with no Novasama-operated dependency.

One binary, `truapi-host`, with these roles:

| Command | Role |
| --- | --- |
| `pairing-host` | Seedless host: presents a pairing deeplink, serves product frames over WebSocket. |
| `signing-host` | Wallet-local host: answers a pairing deeplink, registers statement allowance on-chain, auto-signs (the signing-bot replacement). |
| `identity-check` | Probe which derivation of a mnemonic carries a registered username. |
| `alloc-check` | Diagnose (or `--submit`) on-chain statement-store allowance: ring membership, chosen slot, and the `set_statement_store_account` extrinsic. |

The signing host reuses the `truapi-server` signing-host runtime and its SSO
responder (`runtime/signing_host/sso_responder.rs`); the pairing host is the
existing `PairingHostRuntime`. Both connect to the People-chain statement store
over a native WebSocket `JsonRpcConnection`, and the pairing host bridges product
byte-frames to the runtime over a second WebSocket (one binary message per SCALE
`ProtocolMessage`, matching the browser transport).

## Statement-store allowance

The real statement store enforces per-account allowance. Before pairing, the
signing host grants it on-chain exactly as a real client does: it proves its
LitePeople ring membership with a bandersnatch ring-VRF and submits an unsigned
General (v5) `Resources.set_statement_store_account` extrinsic for each account
that submits statements — its own `//wallet//sso` account and the pairing host's
per-pairing device key. The port lives in `src/alloc/` (metadata-driven
signed-extension encoding, ring fetch, slot scan, ring-VRF proof, extrinsic
assembly, submit). The signing account must be an attested LitePeople member;
`alloc-check` verifies this and can submit a test registration.

## End-to-end test

`e2e/run-e2e.ts` boots a pairing host, and (once login begins) a signing host
that registers allowance on-chain, then drives the real `@parity/truapi` client
against the pairing host over the real statement store. Two modes:

- default: a curated battery of signer-backed methods (login, get account, raw
  and payload signing, transaction construction, entropy) — a deterministic
  gate, 7/7.
- `E2E_DIAGNOSIS=1`: runs the playground's own generated example sources through
  the playground's `runExample`, i.e. literally the playground diagnosis. Gated
  on the signer-critical methods; Asset Hub `Chain/*` methods and deferred
  features are reported but not gated unless `E2E_LIVE_CHAIN=1` routes them to a
  real node.

```bash
# One-time JS setup (generated client + built package + playground deps):
./scripts/codegen.sh
( cd js/packages/truapi && bun install && bunx tsc -b )
( cd playground && bun install )

# Run it:
bash rust/crates/truapi-host-cli/e2e/run.sh              # curated battery
E2E_DIAGNOSIS=1 bash rust/crates/truapi-host-cli/e2e/run.sh   # full diagnosis
```

## Manual use

```bash
cargo build -p truapi-host-cli
BIN=target/debug/truapi-host

# Both hosts default to the real People-chain statement store
# (wss://paseo-people-next-system-rpc.polkadot.io); override with --statement-store.
$BIN pairing-host --frame-listen 127.0.0.1:9955 &
# A product connects to ws://127.0.0.1:9955 and calls account.requestLogin;
# the pairing host prints `PAIRING_DEEPLINK <deeplink>`. Hand it to the signer,
# which registers on-chain allowance then answers the handshake:
$BIN signing-host --deeplink '<deeplink>'

# Inspect on-chain statement-store allowance for a mnemonic:
$BIN alloc-check --lookback 100          # ring membership + free slot (read-only)
```

Signing is auto-approved via the platform's `UserConfirmation`; pass
`--reject` to the signing host to refuse every sensitive action (negative
tests).

## Playground diagnosis coverage

`E2E_DIAGNOSIS=1` writes `explorer/diagnosis-reports/headless.md` in the same
table shape as `web.md` (the dotli browser host), for a direct diff. Run with
`E2E_LIVE_CHAIN=1` to route `Chain/*` to real paseo-next-v2 nodes:

```bash
E2E_LIVE_CHAIN=1 E2E_DIAGNOSIS=1 bun rust/crates/truapi-host-cli/e2e/run-e2e.ts
```

With live chain on, the diagnosis is **43 passed, 1 failed, 20 skipped**. The
headless stack matches the browser host on every method it passes and also
passes 3 the browser host fails (`Signing/sign_raw_with_legacy_account`,
`Signing/sign_payload_with_legacy_account`,
`Statement Store/create_proof_authorized`). The one remaining failure is
environmental, not a host defect:

- `Chain/stop_transaction` — the example passes a hardcoded bogus `operationId`;
  the real RPC node rejects it (`-32602`), whereas the browser host's smoldot
  tolerates unknown ids.

## Scope / gaps

- **Chain methods** route to real `wss://` nodes when `E2E_LIVE_CHAIN=1`
  (`src/chain.rs`, `PASEO_NEXT_V2_CHAIN_ENDPOINTS`); off by default so the
  curated battery stays hermetic and network-free. A rustls crypto provider is
  installed at startup for the TLS connections.
- **Ring-VRF product-account aliases** are implemented natively via the
  `verifiable` crate (`get_account_alias`); on wasm they remain `Unavailable`.
- **`get_user_id`** resolves the signing account's username from People-chain
  `Resources.Consumers`. Since SSO and identity both run over the real People
  chain, the username always resolves; the signing host presents its
  `//wallet//sso` account as its statement identity. `truapi-host signing-host
  --username <base>` registers a fresh lite username via the identity backend
  (`src/attestation.rs`); first registration is backend-async and can take
  minutes (ring onboarding), so the e2e uses an account that is already
  registered. `truapi-host identity-check --mnemonic <m>` probes which
  derivation carries a username.
- **Statement-store allowance** is registered on-chain before pairing (see
  above). The signing account must be an attested LitePeople ring member; it may
  sit in an old ring, so the signing host scans back from the current ring index
  to find it (slow, but one-time per pairing). `set_statement_store_account`
  resource-allocation over SSO is still reported `NotAvailable`.
- Everything else the browser host exercises passes: signing (raw, payload,
  create-transaction, and their legacy variants), statement store, entropy,
  aliases, preimage, storage, permissions, notifications, theme, system, chain
  (with `E2E_LIVE_CHAIN=1`), and user id.
