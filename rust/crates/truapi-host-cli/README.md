# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi-server`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over a local statement-store, so the playground's
own tests can run against a real signer with no Novasama-operated dependency.

One binary, `truapi-host`, with three roles:

| Command | Role |
| --- | --- |
| `relay` | In-memory statement-store the two hosts pair over (dev test double). |
| `pairing-host` | Seedless host: presents a pairing deeplink, serves product frames over WebSocket. |
| `signing-host` | Wallet-local host: answers a pairing deeplink, auto-signs (the signing-bot replacement). |

The signing host reuses the `truapi-server` signing-host runtime and its new
SSO responder (`runtime/signing_host/sso_responder.rs`); the pairing host is the
existing `PairingHostRuntime`. Both talk to the relay over a native WebSocket
`JsonRpcConnection`, and the pairing host bridges product byte-frames to the
runtime over a second WebSocket (one binary message per SCALE `ProtocolMessage`,
matching the browser transport).

## End-to-end test

`e2e/run-e2e.ts` boots the relay, a pairing host, and (once login begins) a
signing host, then drives the real `@parity/truapi` client against the pairing
host. Two modes:

- default: a curated battery of signer-backed methods (login, get account, raw
  and payload signing, transaction construction, entropy) — a deterministic
  gate, 7/7.
- `E2E_DIAGNOSIS=1`: runs the playground's own generated example sources through
  the playground's `runExample`, i.e. literally the playground diagnosis. Gated
  on the signer-critical methods; chain-node methods and deferred features
  (ring-VRF alias, identity, live-chain transaction assembly) are reported but
  not gated, since the hermetic relay is a statement store, not a full node.

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

$BIN relay --listen 127.0.0.1:9944 &
$BIN pairing-host --relay ws://127.0.0.1:9944 --frame-listen 127.0.0.1:9955 &
# A product connects to ws://127.0.0.1:9955 and calls account.requestLogin;
# the pairing host prints `PAIRING_DEEPLINK <deeplink>`. Hand it to the signer:
$BIN signing-host --relay ws://127.0.0.1:9944 --deeplink '<deeplink>'
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
  `Resources.Consumers`, so the pairing host needs `--resolve-identity` (which
  points identity lookups at the real People chain while SSO stays on the
  relay). The signing host presents its `//wallet//sso` account as its
  statement identity. `truapi-host signing-host --username <base>` registers a
  fresh lite username via the identity backend (`src/attestation.rs`); first
  registration is backend-async and can take minutes (ring onboarding), so the
  e2e uses an account that is already registered. `truapi-host identity-check
  --mnemonic <m>` probes which derivation carries a username.
- **On-chain resource allocation** returns `Unavailable` on the signing host.
- Everything else the browser host exercises passes: signing (raw, payload,
  create-transaction, and their legacy variants), statement store, entropy,
  aliases, preimage, storage, permissions, notifications, theme, system, chain
  (with `E2E_LIVE_CHAIN=1`), and user id.
