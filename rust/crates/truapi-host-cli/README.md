# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi-server`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over the **real People-chain statement store** (the
same node an iOS/web client uses), so tests run against a real signer with no
Novasama-operated dependency.

See [SPEC.md](SPEC.md) for the complete as-built v0.1 behavior and engineering
contract.

Either host can be driven by a **product script** you write: a JS/TS file that
receives a global `truapi` (the `@parity/truapi` client, scoped to a product id)
and calls it like any product would. With `--script`, the CLI runs the script
and exits with its status. Without `--script`, both roles open a full-screen
terminal UI when stdin and stdout are TTYs.

One binary, `truapi-host`:

| Command | Role |
| --- | --- |
| `pairing-host` | Seedless host: serves product frames, emits pairing deeplinks, and can run product scripts. |
| `signing-host` | Wallet-local host: owns signer identity, can run product scripts, accepts pairing deeplinks, registers statement allowance on-chain, signs. |
| `identity-check` | Probe which derivation of a mnemonic carries a registered username. |
| `alloc-check` | Diagnose (or `--submit`) on-chain statement-store allowance: ring membership, chosen slot, and the `set_statement_store_account` extrinsic. |

The repository's `make e2e-dotli` target builds this binary and runs the
dotli/playground Diagnosis suite with a non-interactive signing-host responder.
It verifies the initial pairing, remote signing, host sign-out, and
same-account reconnect without the external signer-bot service.

## Quick start

```bash
make headless install  # build dependencies and install truapi-host once
truapi-host signing-host
```

The signing host opens an interactive terminal where you can paste a pairing
link, type `/pair <link>`, run `/script`, or use `/help` to discover the
available commands. It uses `--mnemonic` / `HOST_CLI_SIGNER_MNEMONIC` if set.
Otherwise it auto-selects or creates a stored account under `--base-path` (default
`$XDG_STATE_HOME/truapi-host` or `~/.local/state/truapi-host`), attests it
through the identity backend, waits for ring readiness, and rotates when the
current account exhausts Statement Store slots.

### Interactive terminal UI

In a TTY, both hosts open the same scrollable transcript above a single command
bar. Host lifecycle events, tracing logs, every incoming SSO request, script
stdout/stderr, commands, and approval prompts all use that transcript, so
background output cannot overwrite input. On `signing-host`, `--deeplink URL`
opens the UI and starts the pairing response after initialization.

Commands always start with `/`:

| Command | Result |
| --- | --- |
| `/pair <url>` | Validate and answer a `polkadotapp://pair?...` deeplink (signing host). |
| `/script` | Reopen the session's last TypeScript scratch script (or create one), then run it. |
| `/script <path>` | Remember and run an existing JS/TS product script through the public frame endpoint. |
| `/login` | Start pairing for the selected product and copy its deeplink to the clipboard. |
| `/logout` | Disconnect the pairing host and discard its old pairing keypair. |
| `/log <level>` | Change tracing to `error`, `warn`, `info`, `debug`, or `trace`. |
| `/product` | Show the currently selected product. |
| `/product <id>` | Switch the product used by future scripts and frame connections. |
| `/session` | Show the current session name, path, and user id (signing host). |
| `/session <name>` | Switch to or create an isolated signing-host session. |
| `/session --list` | List user sessions for the current network. |
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

On `pairing-host`, `/logout` cancels an in-flight pairing, disconnects the
current signing host, and removes the old pairing identity. The next product
login request or operator `/login` generates a new keypair and emits a fresh
link that can be answered by another signing host. `/login` uses the current
`/product` selection, copies the generated deeplink to the system clipboard,
and remains interactive while the TUI renders pairing progress. A clipboard
failure is reported without cancelling pairing. Logout does not clear product
storage, scripts, or the selected product.

Both `pairing-host` and `signing-host` use the same interactive UI and command
bar. It uses a quiet, command-centered transcript: submitted
commands title full-width dividers, script stdout keeps the terminal's normal
foreground, stderr has a small error gutter, and lifecycle work updates
sentence-case status rows in place. A compact
`TrUAPI <role> host · 👤 <name> · 🌐 <network> · 📦 <product>` status sits
below the writing bar. Long product names are ellipsized, while session and log
level stay out of that bar. A borderless, subtly backgrounded composer anchors
autocomplete and the `›` prompt while keeping the native cursor after the
input. When the input is empty, command guidance appears there as a placeholder
instead of occupying status space. Set `NO_COLOR=1` to remove semantic colors
and the surface fill without losing spacing, status symbols, or wording.

Non-interactive `--script` and `exec` runs use the same sentence-case event
copy and status symbols without the full-screen chrome. This keeps captured
logs readable while pairing URLs remain directly extractable by automation.
`/copy` copies readable transcript text without UI chrome or complete pairing
links. Captured script output is plain text: Chalk sees piped stdout, and the
host strips terminal control sequences before adding child output to the
transcript. Raw ANSI styling such as bold is therefore not rendered in the
full-screen UI.

Bare `/script` reopens the last script recorded for the active session,
including a path previously selected with `/script <path>`. If that file is
missing or the session has no script yet, it creates a durable Bun TypeScript
file under the active host state's `scripts/` directory. Its starter imports
`chalk` to demonstrate that scripts can import npm packages directly and let
Bun install missing dependencies automatically, then calls
`truapi.account.getUserId()`.
The TUI temporarily yields the terminal to `$VISUAL`, then `$EDITOR`, or
`vi` when neither is set. After the editor exits successfully, the TUI is
restored and the saved script runs through the public frame endpoint. Editor
settings containing arguments, such as `EDITOR='code --wait'`, are supported.

Managed sessions isolate signer accounts, product/core storage, and permissions.
Once a signer identity is known, its public session name is the Lite username
and its files live under
`<base-path>/<network>/<username>_signing_host`. Provisional and legacy named
sessions are promoted to that user-owned root, so an old name such as `pgtest`
does not remain the durable namespace. The selected username is remembered per
network but is not repeated in the status bar as a separate session field.
`default` remains only as a compatibility/bootstrap location until a username
is resolved. It is hidden from session completion and listing and cannot be
selected with `/session default`. User session names contain lowercase ASCII
letters, digits, `.`, `_`, or `-`; they cannot be paths. Switching prepares the
target while the old session remains active, then stops its pairing responder
and resets product WebSocket connections so clients reconnect against the new
runtime.
New auto-managed accounts use the session name as their Lite username prefix;
characters other than lowercase letters are omitted. For example, session
`pgtest` creates usernames beginning with `pgtest`. An explicit
`--lite-username-prefix` takes precedence, and `default` retains the historical
`headless` prefix.
The selected username and last script reference are cached in `session.json`
inside the displayed session path. Scratch scripts use a portable filename;
explicit scripts use an absolute path. On restart, an
already-provisioned local signer is activated from disk without an
identity-backend or ring-membership round trip, and bare `/script` restores that
session's editor context. A session with no signer yet reports
`<not provisioned>` and the transcript prompts the user to run
`/session <name>`. Inspecting with bare `/session` never starts network
onboarding; naming a different session creates and connects its user.

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
truapi-host signing-host exec '/pair polkadotapp://pair?handshake=...'
```

`exec` does not enable raw mode or emit terminal controls. Command results go
to stdout, diagnostics go to stderr, and the process exits when the command
finishes. Starting `signing-host` without `--script` or `exec` while either
stdin or stdout is not a TTY is an invocation error. The existing `--script`
one-shot mode remains supported.

## Writing a product script

A product script is top-level JavaScript or TypeScript (an ES module) run by
Bun. It can import npm dependencies directly; Bun installs missing packages
automatically. The runner injects three globals before running it:

- **`truapi`** — the `@parity/truapi` client connected to the pairing host and
  scoped to the host's `--product-id`. Call `truapi.account.requestLogin(...)`,
  `truapi.signing.signRaw(...)`, `truapi.localStorage.write(...)`, etc.
- **`host`** — just `host.productId` and `host.productAccount(index?)`. That is
  all it does: it keeps product accounts in sync with the host's `--product-id`
  (hardcoding a mismatched id fails signing with `PermissionDenied`). Use
  `console.log` and `throw` for everything else.
- **`assert`** — throw when its condition is false, using any following values
  as the error message.

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
`headless-playground.dot`) sets the initial product. `/product <id>` changes it
for the lifetime of the process. Switching disconnects active product
WebSockets so clients reconnect with a new product context; the network,
pairing relationship, signing-host session, and wallet identity stay active.
Product-owned storage, permissions, and derived product accounts are scoped by
the selected id, so the newly selected product sees its own state. The next
`/script` also receives the new id through `host.productId`.

Pairing-host state follows the same identity rule under
`<base-path>/<network>/<username>_pairing_host`. Before the first identity is
known it uses the small `<network>/pairing-host` bootstrap; connecting moves
legacy bootstrap data to the first resolved user. After `/logout`, connecting
as a different user swaps to that user's KV/core namespace instead of carrying
the previous user's product data forward.

Product-local KV is persisted independently under each identity root as
`storage/<safe-product-slug>--<hash>.json`. Each document records its normalized
product id and raw product keys. On first use, the older combined
`product-storage.json` in that profile is split into those files and retained
as `product-storage.v1.json.migrated`. Product and core JSON writes use a
flushed temporary file and atomic rename.

Five scripts ship under `js/scripts/`:

- `battery.ts` — the generated full-surface gate. It discovers every method
  from the same code-generated example manifest as the playground Diagnosis,
  attempts all examples (including APIs the browser diagnosis classifies as
  intentionally unsupported), prints test-reporter rows with timings and clean
  failure details, writes the browser-shaped result matrix to
  the role-specific report under `explorer/diagnosis-reports/`, and exits
  nonzero if any example fails. A paired run writes `pairing-host-cli.md`; a
  direct signing-host run writes `signing-host-cli.md`. Override the artifact
  path with `TRUAPI_BATTERY_REPORT_PATH`. Run the complete generated Playground
  surface directly against the signing host:

  ```bash
  target/debug/truapi-host signing-host \
    --product-id truapi-playground.dot \
    --script rust/crates/truapi-host-cli/js/scripts/battery.ts \
    --auto-accept
  ```

  To exercise the paired topology, run the same script with `pairing-host`,
  then answer its emitted link from a second terminal:

  ```bash
  # Terminal 1
  target/debug/truapi-host pairing-host \
    --product-id truapi-playground.dot \
    --script rust/crates/truapi-host-cli/js/scripts/battery.ts \
    --auto-accept

  # Terminal 2
  target/debug/truapi-host signing-host \
    --deeplink '<pairing link>' \
    --auto-accept
  ```

- `whoami.ts` — calls `getUserId` and prints `WHOAMI <primary username>`; this
  remains available as an explicit `/script <path>` example.
- `signing-smoke.ts` — a focused product-account signing check.
- `ring-vrf-smoke.ts` — calls `getAccountAlias` and `createAccountProof`
  against the Paseo Next v2 LitePeople ring, then verifies both calls return
  the same contextual alias.
- `preimage-smoke.ts` — a focused Bulletin preimage flow check.

The generated examples are baked to the `truapi-playground.dot` product. With
live routing enabled, `Chain/stop_transaction` uses host-owned operation ids and
treats already-finished provider operations as stopped. `Preimage/*` also uses
the real Bulletin Next chain and asks the signing host to claim People-chain
long-term storage before returning the product-scoped Bulletin allowance key.
It needs the playground's deps (`cd playground && bun install`). Repeated live
runs can exhaust the signer's per-period Statement Store or Bulletin allocation
slots; the signing host rotates auto-managed signer accounts when Statement
Store slots are exhausted.

## Confirmations

Both hosts take `--auto-accept`. Without it, confirmations a web/iOS host would
show as a modal (sign requests, permission prompts, and cross-product Ring-VRF
requests) are rendered prominently in the signing-host transcript and answered
directly with `y` or `n` (typed `yes`/`no` plus Enter also works). Approval
cards summarize and redact signing payloads rather than dumping debug objects.
The current command draft is
restored afterward; Esc safely rejects. Concurrent approvals are serialized.
In non-interactive `exec` mode, a TTY gets a plain yes/no prompt and non-TTY
stdin safely rejects instead of hanging. Same-product Ring-VRF requests do not
prompt, matching the iOS signing host. Pass `--auto-accept` for unattended
runs; every auto-approved decision is still printed.

## Logging

Use the global `--log-level` option (`error`, `warn`, `info`, `debug`, or
`trace`) before or after the subcommand, or `/log <level>` in the terminal UI.
Every decoded inbound SSO request and every published response is visible
regardless of the selected level. Stable response entries include the request
name, statement and remote message ids, protocol outcome, and elapsed time;
encoded protocol errors include their reason. Response-publication failures
are shown separately. `debug` adds decoded request/response summaries and
`trace` adds complete payload and transport metadata. Undecodable requests are
warnings with the available identifiers so protocol-version mismatches can be
diagnosed.

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

# Terminal 1 — pairing host runs a product script and prints its pairing link:
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
