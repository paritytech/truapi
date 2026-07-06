# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi-server`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over the **real People-chain statement store** (the
same node an iOS/web client uses), so tests run against a real signer with no
Novasama-operated dependency.

The pairing host is driven by a **product script** you write: a JS/TS file that
receives a global `truapi` (the `@parity/truapi` client, scoped to a product id)
and calls it like any product would. The CLI runs the script and exits with its
status, so `truapi-host pairing-host --script foo.ts` *is* the test — there is no
separate bun orchestrator.

One binary, `truapi-host`:

| Command | Role |
| --- | --- |
| `pairing-host` | Seedless host: serves product frames and runs your `--script` with `truapi` injected. |
| `signing-host` | Wallet-local host: answers a pairing deeplink, registers statement allowance on-chain, signs. |
| `identity-check` | Probe which derivation of a mnemonic carries a registered username. |
| `alloc-check` | Diagnose (or `--submit`) on-chain statement-store allowance: ring membership, chosen slot, and the `set_statement_store_account` extrinsic. |

## Quick start

```bash
make headless                                # build the CLI + JS client (once)
rust/crates/truapi-host-cli/e2e/run.sh       # run js/scripts/battery.ts end-to-end
rust/crates/truapi-host-cli/e2e/run.sh path/to/my-script.ts   # or a custom script
```

`run.sh` starts a pairing host running the product script, hands the emitted
pairing deeplink to a signing host, and exits with the script's status. It uses
the dev mnemonic by default (a registered LitePeople member); override with
`SIGNER_MNEMONIC=...`, the product with `PRODUCT_ID=...`, and the port with
`FRAME=...`.

## Writing a product script

A product script is an ES module. The runner injects two globals before it runs:

- **`truapi`** — the `@parity/truapi` client connected to the pairing host and
  scoped to the host's `--product-id`. Call `truapi.account.requestLogin(...)`,
  `truapi.signing.signRaw(...)`, `truapi.localStorage.write(...)`, etc.
- **`host`** — helpers: `host.productId`, `host.productAccount(index?)`,
  `host.log(...)` (stderr), `host.assert(cond, msg)`.

Export a default function to receive the `host` context; throw (or reject) to
fail the run. Minimal example:

```ts
export default async function (host) {
  const login = await truapi.account.requestLogin({ reason: undefined });
  host.assert(login.isOk() && login.value === "Success", "login failed");

  const res = await truapi.signing.signRaw({
    account: host.productAccount(),
    payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
  });
  res.match(
    (v) => host.log("signature", v.signature),
    (e) => { throw new Error(JSON.stringify(e)); },
  );
}
```

`--product-id` (a `.dot` name or `localhost` identifier; default
`headless-playground.dot`) scopes product-owned APIs like `truapi.localStorage.*`
and the accounts `host.productAccount()` returns.

Two scripts ship under `js/scripts/`:

- `battery.ts` — the curated signer gate (login + raw/payload signing,
  create-transaction, entropy). This is `run.sh`'s default.
- `diagnosis.ts` — runs the playground's own generated example sources
  (`runExample`) and writes a `web.md`-shape report to
  `explorer/diagnosis-reports/headless.md`, gating on the signer-critical
  methods. The generated examples are baked to the `truapi-playground.dot`
  product, so run it with that product id:

  ```bash
  PRODUCT_ID=truapi-playground.dot E2E_LIVE_CHAIN=1 \
    rust/crates/truapi-host-cli/e2e/run.sh rust/crates/truapi-host-cli/js/scripts/diagnosis.ts
  ```

  With a live chain, this is **43 passed, 1 failed, 20 skipped**; the lone
  failure is `Chain/stop_transaction` (the example sends a deliberately-invalid
  operation id the real RPC node rejects, which the browser host's smoldot
  tolerates). It needs the playground's deps (`cd playground && bun install`).

## Confirmations

Both hosts take `--auto-accept`. Without it, every confirmation a web/iOS host
would show as a modal (sign requests, permission prompts) is printed on the CLI
and answered `y/n` on stdin. `run.sh` passes `--auto-accept` to both for
unattended runs.

## Statement-store allowance

The real statement store enforces per-account allowance. Before pairing, the
signing host grants it on-chain exactly as a real client does: it proves its
LitePeople ring membership with a bandersnatch ring-VRF and submits an unsigned
General (v5) `Resources.set_statement_store_account` extrinsic for each account
that submits statements — its own `//wallet//sso` account and the pairing host's
per-pairing device key. The port lives in `src/alloc/` (metadata-driven
signed-extension encoding, ring fetch, slot scan, ring-VRF proof, extrinsic
assembly, submit). The signing account must be an attested LitePeople member,
and may sit in an old ring, so the signing host scans back from the current ring
index (slow, one-time per pairing). `alloc-check` verifies membership and can
submit a test registration.

## Manual use (two terminals)

```bash
make headless
BIN=target/debug/truapi-host

# Terminal 1 — pairing host runs a product script and prints PAIRING_DEEPLINK:
$BIN pairing-host --product-id myapp.dot --script js/scripts/battery.ts --auto-accept

# Terminal 2 — hand the deeplink to a signing host (registers allowance, signs):
$BIN signing-host --deeplink '<deeplink>' --auto-accept

# Inspect on-chain statement-store allowance for a mnemonic:
$BIN alloc-check --lookback 100          # ring membership + free slot (read-only)
```

Both hosts default `--statement-store` to the real People chain
(`wss://paseo-people-next-system-rpc.polkadot.io`); override with
`--statement-store`.

## Scope / gaps

- **Chain methods** route to real `wss://` nodes when `E2E_LIVE_CHAIN=1`
  (`src/chain.rs`, `PASEO_NEXT_V2_CHAIN_ENDPOINTS`); off by default. A rustls
  crypto provider is installed at startup for the TLS connections.
- **Ring-VRF product-account aliases** are implemented natively via the
  `verifiable` crate (`get_account_alias`); on wasm they remain `Unavailable`.
- **`get_user_id`** resolves the signing account's username from People-chain
  `Resources.Consumers`. `truapi-host signing-host --username <base>` registers a
  fresh lite username via the identity backend (`src/attestation.rs`); first
  registration is backend-async and can take minutes (ring onboarding), so the
  e2e uses an already-registered account. `truapi-host identity-check --mnemonic
  <m>` probes which derivation carries a username.
- `set_statement_store_account` resource-allocation over SSO is still reported
  `NotAvailable`.
- Everything else the browser host exercises passes: signing (raw, payload,
  create-transaction, and their legacy variants), statement store, entropy,
  aliases, preimage, storage, permissions, notifications, theme, system, chain
  (with `E2E_LIVE_CHAIN=1`), and user id.
