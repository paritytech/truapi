# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi-server`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over the **real People-chain statement store** (the
same node an iOS/web client uses), so tests run against a real signer with no
Novasama-operated dependency.

Either host can be driven by a **product script** you write: a JS/TS file that
receives a global `truapi` (the `@parity/truapi` client, scoped to a product id)
and calls it like any product would. With `--script`, the CLI runs the script
and exits with its status. Without `--script`, the host stays in an interactive
shell until you quit.

One binary, `truapi-host`:

| Command | Role |
| --- | --- |
| `pairing-host` | Seedless host: serves product frames, emits pairing deeplinks, and can run product scripts. |
| `signing-host` | Wallet-local host: owns signer identity, can run product scripts, accepts pairing deeplinks, registers statement allowance on-chain, signs. |
| `identity-check` | Probe which derivation of a mnemonic carries a registered username. |
| `alloc-check` | Diagnose (or `--submit`) on-chain statement-store allowance: ring membership, chosen slot, and the `set_statement_store_account` extrinsic. |

## Quick start

```bash
make headless install                        # build deps + install truapi-host (once)
rust/crates/truapi-host-cli/e2e/run.sh       # run js/scripts/battery.ts end-to-end
rust/crates/truapi-host-cli/e2e/run.sh path/to/my-script.ts   # or a custom script
```

`run.sh` starts a pairing host running the product script, hands the emitted
pairing deeplink to a signing host, and exits with the script's status. The
signing host uses `--mnemonic` / `HOST_CLI_SIGNER_MNEMONIC` if set. Otherwise it
auto-selects or creates a stored account under `--base-path` (default
`$XDG_STATE_HOME/truapi-host` or `~/.local/state/truapi-host`), attests it
through the identity backend, waits for ring readiness, and rotates when the
current account exhausts Statement Store slots. Override the product with
`PRODUCT_ID=...` and the pairing frame port with `FRAME=...`.

## Writing a product script

A product script is top-level code (an ES module). The runner injects two
globals before running it:

- **`truapi`** — the `@parity/truapi` client connected to the pairing host and
  scoped to the host's `--product-id`. Call `truapi.account.requestLogin(...)`,
  `truapi.signing.signRaw(...)`, `truapi.localStorage.write(...)`, etc.
- **`host`** — just `host.productId` and `host.productAccount(index?)`. That is
  all it does: it keeps product accounts in sync with the host's `--product-id`
  (hardcoding a mismatched id fails signing with `PermissionDenied`). Use
  `console.log` and `throw` for everything else.

Write it top-level and `throw` (or reject) to fail the run:

```ts
const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || login.value !== "Success") throw new Error("login failed");

const res = await truapi.signing.signRaw({
  account: host.productAccount(),
  payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
});
res.match(
  (v) => console.log("signature", v.signature),
  (e) => { throw new Error(JSON.stringify(e)); },
);
```

`--product-id` (a `.dot` name or `localhost` identifier; default
`headless-playground.dot`) scopes product-owned APIs like `truapi.localStorage.*`
and the accounts `host.productAccount()` returns.

Two scripts ship under `js/scripts/`:

- `battery.ts` — the curated signer gate (login + raw/payload signing,
  create-transaction, entropy). This is `run.sh`'s default.
- `diagnosis.ts` — runs the playground's own generated example sources
  (`runExample`) and writes a `web.md`-shape report to
  `explorer/diagnosis-reports/headless-pairing.md`, gating on the signer-critical
  methods. The generated examples are baked to the `truapi-playground.dot`
  product, so run it with that product id:

  ```bash
  PRODUCT_ID=truapi-playground.dot E2E_LIVE_CHAIN=1 \
    rust/crates/truapi-host-cli/e2e/run.sh rust/crates/truapi-host-cli/js/scripts/diagnosis.ts
  ```

  With live routing enabled, `Chain/stop_transaction` uses host-owned operation
  ids and treats already-finished provider operations as stopped. `Preimage/*`
  also uses the real Bulletin Next chain and asks the signing host to claim
  People-chain long-term storage before returning the product-scoped Bulletin
  allowance key. It needs the playground's deps
  (`cd playground && bun install`). Repeated live runs can exhaust the
  signer's per-period Statement Store or Bulletin allocation slots; the
  signing host now rotates auto-managed signer accounts when Statement Store
  slots are exhausted.

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
per-pairing device key. The shared native allocator in `truapi-server`
handles metadata-driven signed-extension encoding, ring fetch, slot scan,
ring-VRF proof, extrinsic assembly, and submit. The signing account must be an attested LitePeople member,
and may sit in an old ring, so the signing host scans back from the current ring
index (slow, one-time per pairing). Auto-managed accounts are stored in
`accounts.json` under `--base-path`; mnemonics are plaintext local test secrets
and the file is written with `0600` permissions on Unix. `alloc-check` verifies
membership and can submit a test registration.

## Manual use (two terminals)

```bash
make headless install

# Terminal 1 — pairing host runs a product script and prints PAIRING_DEEPLINK:
truapi-host pairing-host --product-id myapp.dot --script js/scripts/battery.ts --auto-accept

# Terminal 2 — hand the deeplink to a signing host (registers allowance, signs).
# The wallet mnemonic comes from --mnemonic / $HOST_CLI_SIGNER_MNEMONIC when set;
# otherwise the CLI auto-selects or creates an attested account.
truapi-host signing-host --deeplink '<deeplink>' --auto-accept
HOST_CLI_SIGNER_MNEMONIC="spin battle …" truapi-host signing-host --deeplink '<deeplink>' --auto-accept

# Inspect on-chain statement-store allowance for a mnemonic:
truapi-host alloc-check --mnemonic "spin battle …" --lookback 100
```

Both hosts take `--network` (default `paseo-next-v2`). The network preset owns
the identity backend URL, People RPC, Bulletin RPC, and genesis hashes; there is
no public `--statement-store` flag.

## Scope / gaps

- **Chain methods** route to real `wss://` nodes from the selected `--network`
  when `E2E_LIVE_CHAIN=1`; off by default. A rustls crypto provider is
  installed at startup for the TLS connections.
- **Ring-VRF product-account aliases** are implemented natively via the
  `verifiable` crate (`get_account_alias`); on wasm they remain `Unavailable`.
- **`get_user_id`** resolves the signing account's username from People-chain
  `Resources.Consumers`. Auto-managed signing accounts register fresh lite
  usernames via the identity backend (`src/attestation.rs`); first registration
  is backend-async and can take minutes (ring onboarding). `truapi-host
  identity-check --mnemonic <m>` probes which derivation carries a username.
- `set_statement_store_account` and Bulletin long-term-storage resource
  allocation are implemented over SSO on native headless hosts.
- Everything else the browser host exercises passes: signing (raw, payload,
  create-transaction, and their legacy variants), statement store, entropy,
  aliases, preimage, storage, permissions, notifications, theme, system, chain
  (with `E2E_LIVE_CHAIN=1`), and user id, subject to live chain availability
  and allowance-slot capacity.
