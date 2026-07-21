# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi-server`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over the **real People-chain statement store** (the
same node an iOS/web client uses), so tests run against a real signer with no
Novasama-operated dependency.

Either host can be driven by a **product script** you write: a JS/TS file that
receives a global `truapi` (the `@parity/truapi` client, scoped to a product id)
and calls it like any product would. With `--script`, the CLI runs the script
and exits with its status. Without `--script`, `pairing-host` keeps its line
prompt while `signing-host` opens a full-screen terminal UI when stdin and
stdout are TTYs.

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

### Signing-host terminal UI

In a TTY, `truapi-host signing-host` opens a scrollable transcript above a
single command bar. Host lifecycle events, tracing logs, every incoming SSO
request, script stdout/stderr, commands, and approval prompts all use that
transcript, so background output cannot overwrite input. `--deeplink URL`
opens the same UI and starts the pairing response after initialization.

Commands always start with `/`:

| Command | Result |
| --- | --- |
| `/deeplink <url>` | Validate and answer a `polkadotapp://pair?...` deeplink. |
| `/script` | Open a new TypeScript scratch script in the terminal editor, then run it. |
| `/script <path>` | Run an existing JS/TS product script through the public frame endpoint. |
| `/log <level>` | Change tracing to `error`, `warn`, `info`, `debug`, or `trace`. |
| `/session` | Show the current session name, path, and user id. |
| `/session <name>` | Switch to an existing session or create an isolated one. |
| `/session --list` | List sessions for the current network. |
| `/help` | Show commands and keyboard shortcuts. |
| `/clear` | Clear the visible transcript. |
| `/copy` | Copy the retained transcript to the system clipboard. |
| `/quit` | Shut down cleanly. |

Typing `/` opens autocomplete. Up/Down selects a completion; with the menu
closed it navigates process-local command history. Tab inserts a completion,
and `/script` completes filesystem paths. Ctrl-U/Ctrl-D scroll by half a
viewport, End restores auto-follow, Esc closes autocomplete, and Ctrl-C clears
input, cancels a running command, or exits when idle. Deeplinks are deliberately
not persisted in history across processes.

Bare `/script` creates a durable scratch file under the active session's
`scripts/` directory, seeded with a `truapi.account.getUserId()` example.
The TUI temporarily yields the terminal to `$VISUAL`, then `$EDITOR`, or
`vi` when neither is set. After the editor exits successfully, the TUI is
restored and the saved script runs through the public frame endpoint. Editor
settings containing arguments, such as `EDITOR='code --wait'`, are supported.

Named sessions isolate signer accounts, product/core storage, and permissions
under
`<base-path>/<network>/signing-host/sessions/<name>`. The selected name is
remembered per network and shown in the top bar. `default` preserves the
pre-session account and storage locations for backward compatibility. Session
names contain lowercase ASCII letters, digits, `.`, `_`, or `-`; they
cannot be paths. Switching prepares the target while the old session remains
active, then stops its pairing responder and resets product WebSocket
connections so clients reconnect against the new runtime.
New auto-managed accounts use the session name as their Lite username prefix;
characters other than lowercase letters are omitted. For example, session
`pgtest` creates usernames beginning with `pgtest`. An explicit
`--lite-username-prefix` takes precedence, and `default` retains the historical
`headless` prefix.
The selected username is cached in `session.json` inside the displayed
session path. On restart, an already-provisioned local signer is activated from
disk without an identity-backend or ring-membership round trip, so `/session`
can report `user.id` immediately. A session with no signer yet reports
`<not provisioned>`; inspecting it never starts network onboarding.

Select or create a session at startup with:

```bash
truapi-host signing-host --session alice
```

`--session` cannot be combined with `--account` or `--mnemonic`. A host
started with an explicit mnemonic reports an `ephemeral` session and does not
allow runtime switching.

Only one operational command runs at once, but SSO traffic and approvals keep
flowing while it runs. Without a TTY, use one-shot `exec` mode (parent options
come first):

```bash
truapi-host signing-host exec '/session'
truapi-host signing-host --auto-accept exec '/script ./js/scripts/ring-vrf-smoke.ts'
truapi-host signing-host exec '/deeplink polkadotapp://pair?handshake=...'
```

`exec` does not enable raw mode or emit terminal controls. Command results go
to stdout, diagnostics go to stderr, and the process exits when the command
finishes. Starting `signing-host` without `--script` or `exec` while either
stdin or stdout is not a TTY is an invocation error. The existing `--script`
one-shot mode remains supported.

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
if (
  !login.isOk() ||
  (login.value !== "Success" && login.value !== "AlreadyConnected")
) throw new Error("login failed");

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

Six scripts ship under `js/scripts/`:

- `battery.ts` — the curated signer gate (login + raw/payload signing,
  create-transaction, entropy). This is `run.sh`'s default.
- `whoami.ts` — calls `getUserId` and prints `WHOAMI <primary username>`; this
  remains available as an explicit `/script <path>` example.
- `signing-smoke.ts` — a focused product-account signing check.
- `ring-vrf-smoke.ts` — calls `getAccountAlias` and `createAccountProof`
  against the Paseo Next v2 LitePeople ring, then verifies both calls return
  the same contextual alias.
- `preimage-smoke.ts` — a focused Bulletin preimage flow check.
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

Both hosts take `--auto-accept`. Without it, confirmations a web/iOS host would
show as a modal (sign requests, permission prompts, and cross-product Ring-VRF
requests) are rendered prominently in the signing-host transcript and answered
with `y`/`yes` or `n`/`no` in the command bar. The current command draft is
restored afterward; Esc safely rejects. Concurrent approvals are serialized.
In non-interactive `exec` mode, a TTY gets a plain yes/no prompt and non-TTY
stdin safely rejects instead of hanging. Same-product Ring-VRF requests do not
prompt, matching the iOS signing host. `run.sh` passes `--auto-accept` to both
for unattended runs. Every auto-approved decision is still printed.

## Logging

Use the global `--log-level` option (`error`, `warn`, `info`, `debug`, or
`trace`) before or after the subcommand, or `/log <level>` in the terminal UI.
Every decoded inbound SSO request is visible regardless of the selected level:
the stable request name plus statement request and remote message ids are
logged at `info`. `debug` adds the decoded summary and `trace` adds the complete
decoded payload and transport metadata. Undecodable requests are warnings with
the available identifiers so protocol-version mismatches can be diagnosed.

```bash
truapi-host signing-host --log-level trace --deeplink '<deeplink>' --auto-accept
```

Debug and trace output may contain product signing payloads. `RUST_LOG` takes
precedence at startup and remains available for module-specific filters, except
that the noisy `rustls` and `tungstenite::protocol` tracing targets are always
excluded from CLI log output. Without `RUST_LOG`, `--log-level` and `/log`
apply to TrUAPI targets while other third-party dependencies remain at `warn`.

## Statement-store allowance

The real statement store enforces per-account allowance. Before pairing, the
signing host grants it on-chain exactly as a real client does: it proves its
LitePeople ring membership with a bandersnatch ring-VRF and submits an unsigned
General (v5) `Resources.set_statement_store_account` extrinsic for each account
that submits statements — its own `//wallet//sso` account and the pairing host's
per-pairing device key. The shared native implementation lives in
`truapi-server/src/runtime/statement_allowance/` (metadata-driven
signed-extension encoding, ring fetch, slot scan, ring-VRF proof, extrinsic
assembly, submit). The signing account must be an attested LitePeople member,
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
- **Ring-VRF product-account aliases and proofs** are implemented by the
  signing host via the `verifiable` crate (`get_account_alias` and
  `create_account_proof`).
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
