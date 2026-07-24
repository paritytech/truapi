# TrUAPI Headless Host CLI v0.1 specification

- Status: as-built behavior reference
- Binary: `truapi-host`
- Implementation: `rust/crates/truapi-host-cli/`
- Protocol implementation: `truapi` and `truapi-server`

This document specifies the first complete version of the native headless
TrUAPI host CLI. It was derived from the Rust and TypeScript implementation,
the test suite, the checked-in compatibility reports, and observed runs of both
host roles.

The crate [README](README.md) is the user guide. This document is the complete
behavioral and engineering reference. It describes what v0.1 does, including
its operational limits; it does not contain a roadmap or requirements for
unimplemented features.

## 1. Purpose and scope

`truapi-host` runs real TrUAPI host roles without a browser UI, desktop shell,
phone automation service, or external signing bot. It is intended for:

- local product development;
- protocol and host diagnosis;
- direct signing-host tests;
- paired end-to-end tests; and
- generation of CLI host compatibility reports.

It embeds the real `truapi-server` dispatcher and host logic. Product scripts
use the public `@parity/truapi` client and exchange the same SCALE protocol
messages as a product connected to another host.

The CLI replaces the platform/operating-system seam with native implementations
for persistence, chain RPC, approvals, notifications, navigation, theme, and
terminal presentation. It also owns account onboarding, process orchestration,
the product-frame WebSocket bridge, and the Bun script runner.

It is local test infrastructure, not:

- a production wallet or secure custody product;
- a general-purpose mnemonic manager;
- a mock protocol server;
- a replacement for dot.li or Polkadot Desktop;
- a Chat, Coin Payment, or Payment backend; or
- an arbitrary-network RPC proxy.

## 2. Roles and runtime topologies

### 2.1 Pairing host

The pairing host is seedless and product-facing:

```text
product script or client
       │
       │ TrUAPI SCALE frames over WebSocket
       ▼
pairing-host
       │
       │ People-chain Statement Store / SSO
       ▼
signing-host
       │
       └── wallet entropy, product accounts, signing, aliases, proofs
```

The pairing host:

- serves product connections;
- starts SSO login and emits a `polkadotapp://pair?...` deeplink;
- persists the paired session and product state;
- receives a root entropy source from the signing host;
- delegates signing and other authority operations over SSO; and
- can run scripts before, during, or after pairing.

It never stores the signing host's raw mnemonic or raw root entropy.

### 2.2 Direct signing host

The signing host can also be product-facing:

```text
product script or client
       │
       │ TrUAPI SCALE frames over WebSocket
       ▼
signing-host
       │
       ├── local wallet entropy and product authority
       ├── People-chain Statement Store
       └── Bulletin and optional product-chain RPC
```

This path is used for focused diagnosis without SSO. It uses the same Rust
signing, account, entropy, Statement Store, Bulletin, and product runtime logic
as the paired path.

### 2.3 Ownership boundaries

`truapi-server` owns:

- protocol dispatch and SCALE encoding;
- product and role semantics;
- product-account derivation;
- signing and transaction construction;
- product-scoped entropy derivation;
- Ring-VRF aliases and proofs;
- SSO request/response encoding and transport;
- Statement Store proof, submission, and allowance logic;
- Bulletin preimage submission and resource allocation;
- subscriptions and runtime errors; and
- typed unavailable behavior for unsupported services.

`truapi-host-cli` owns:

- argument and slash-command parsing;
- the single supported network preset;
- local signer selection and onboarding;
- local persistence and account-store locking;
- approvals and `--auto-accept`;
- the terminal UI and plain output;
- product-frame WebSocket listening;
- session and product switching;
- child editor and Bun processes; and
- CLI-specific diagnostics.

Product scripts own:

- the TrUAPI calls under test;
- assertions over typed results;
- test output; and
- success or failure through normal completion or a thrown error.

## 3. Build, installation, and runtime dependencies

### 3.1 Build

From the repository root:

```sh
make headless
```

This builds:

- the Rust `truapi-host` binary; and
- the generated `@parity/truapi` TypeScript client used by the script runner.

The direct Cargo equivalent for the binary is:

```sh
cargo build -p truapi-host-cli
```

### 3.2 Installation

```sh
make headless install
```

The `install` target depends on `headless` and runs:

```sh
cargo install \
  --path rust/crates/truapi-host-cli \
  --bin truapi-host \
  --locked \
  --force
```

### 3.3 Runtime dependencies

Host-only commands need the installed Rust binary. Product scripts additionally
need:

- `bun` on `PATH`;
- `js/runner.ts`; and
- the repository's generated `@parity/truapi` TypeScript sources and their
  dependencies.

By default, the runner path is compiled from `CARGO_MANIFEST_DIR` and therefore
points into the source checkout. `TRUAPI_HOST_RUNNER` can select another
`runner.ts`. The v0.1 install is not a self-contained relocatable script
runtime: deleting or moving the checkout without supplying a replacement
runner breaks `/script` and `--script`.

The binary has `--help` but no `--version` option.

## 4. Top-level command line

```text
truapi-host [--log-level <level>] <command>
```

Commands:

| Command | Purpose |
| --- | --- |
| `pairing-host` | Run the seedless product-facing host. |
| `signing-host` | Run the wallet-local signing host. |
| `identity-check` | Probe People-chain identity records for a mnemonic. |
| `alloc-check` | Inspect or submit Statement Store allowance registration. |

### 4.1 Global logging option

`--log-level` accepts:

- `error`
- `warn`
- `info`
- `debug`
- `trace`

The default is `info`. `TRUAPI_HOST_LOG` supplies an environment default. The
option is global and is accepted before or after a subcommand.

If `RUST_LOG` contains a valid tracing filter, it takes precedence at startup.
The interactive `/log` command later replaces the active filter with the
selected CLI level.

## 5. `pairing-host`

```text
truapi-host pairing-host [options]
```

| Option | Default | Behavior |
| --- | --- | --- |
| `--script <path>` | none | Run one JS/TS product script and exit with its status. |
| `--product-id <id>` | `headless-playground.dot` | Initial product scope. |
| `--frame-listen <socket>` | `127.0.0.1:9955` | Product WebSocket listener. Port `0` selects an available port. |
| `--base-path <path>` | section 12.1 | Root for network, identity, core, script, and product state. |
| `--network <preset>` | `paseo-next-v2` | Select the complete endpoint/genesis preset. |
| `--auto-accept` | off | Approve platform confirmations automatically. |

Without `--script`, both stdin and stdout must be terminals. The command enters
the full-screen terminal UI and remains active until `/quit`, idle Ctrl-C, or
terminal input ends.

With `--script`, the command:

1. builds the pairing runtime;
2. binds and reports the product-frame listener;
3. starts the frame accept loop;
4. starts Bun with inherited stdio;
5. keeps the host alive until Bun exits;
6. stops the accept loop; and
7. exits with the child status.

The script must call `truapi.account.requestLogin()` or the operator must use
`/login` in interactive mode to initiate pairing.

There is no pairing-host `exec` subcommand.

## 6. `signing-host`

```text
truapi-host signing-host [options] [exec '<slash-command>']
```

| Option | Default | Behavior |
| --- | --- | --- |
| `--script <path>` | none | Run one direct product script and exit with its status. |
| `--product-id <id>` | `headless-playground.dot` | Initial product scope. |
| `--deeplink <url>` | none | Answer a pairing deeplink after initialization. |
| `--mnemonic <phrase>` | none | Use raw BIP-39 entropy as an ephemeral local signer. |
| `--account <name>` | none | Use one named account from the default account store. |
| `--session <name>` | remembered session | Restore or create a managed session. |
| `--lite-username-prefix <prefix>` | session-derived | Prefix for newly generated Lite username bases. |
| `--base-path <path>` | section 12.1 | Root for account, session, core, script, and product state. |
| `--network <preset>` | `paseo-next-v2` | Select the complete endpoint/genesis preset. |
| `--frame-listen <socket>` | `127.0.0.1:9956` | Direct product WebSocket listener. Port `0` is allowed. |
| `--auto-accept` | off | Approve platform confirmations automatically. |

`HOST_CLI_SIGNER_MNEMONIC` supplies `--mnemonic` when the option is omitted.

### 6.1 Argument conflicts

The CLI rejects these combinations with invocation status `2` before runtime
startup:

- `--script` with `exec`;
- `--mnemonic` with `--account`;
- `--mnemonic` with `--session`;
- `--mnemonic` with `--lite-username-prefix`;
- `--account` with `--session`; and
- `--account` with `--lite-username-prefix`.

The same conflicts apply when the mnemonic came from
`HOST_CLI_SIGNER_MNEMONIC`.

An explicit session name is validated before startup. Empty strings are treated
as absent after trimming.

### 6.2 Interactive mode

When neither `--script` nor `exec` is present, stdin and stdout must be
terminals. The signing host:

1. resolves the selected session and any locally cached signer;
2. creates the signing runtime;
3. activates a cached signer without a network onboarding round trip;
4. binds and reports the product-frame listener;
5. starts `--deeplink`, when supplied, as an initial `/pair` operation; and
6. enters the command loop.

Signer provisioning is otherwise lazy. Merely starting the UI, using `/help`,
using `/product`, or inspecting sessions does not create a new account.

### 6.3 One-shot `--script`

The host binds its product listener, optionally starts a background responder
for `--deeplink`, ensures and activates a signer, runs Bun with inherited stdio,
aborts the responder after the script, and exits with the child status.

### 6.4 `signing-host exec`

```sh
truapi-host signing-host [parent options] exec '<slash-command>'
```

`exec`:

- parses exactly one slash command;
- starts the same signing runtime and product-frame listener;
- does not enter raw mode or the alternate screen;
- writes human output to normal stdout/stderr;
- optionally runs `--deeplink` in the background for the command lifetime;
- aborts that responder when the command completes; and
- exits after the command.

Parent options must appear before `exec`.

`exec '/script'` needs a TTY because it opens an editor. In non-TTY execution,
use `exec '/script <path>'` instead. `/copy` is unavailable. `/clear` and
`/quit` are successful no-ops in one-shot mode.

## 7. Product identifiers and switching

Accepted product identifiers are:

- a name ending in `.dot`;
- `localhost`; or
- a string beginning with `localhost:`.

Identifiers are trimmed, Unicode-NFC normalized, and lowercased. For example,
`" Dotli.DOT "` becomes `dotli.dot`.

Other identifiers, including an ordinary `example.com`, are rejected.

The product id scopes:

- product accounts and signatures;
- Ring-VRF context and cross-product policy;
- derived entropy;
- local product storage;
- permissions and authority checks;
- product-frame runtime context; and
- `host.productId` and `host.productAccount()` in scripts.

`/product` prints the current normalized id.

`/product <id>` changes only the process-local product selection. It does not
change:

- the network;
- the active signer or paired user;
- the signing session;
- the SSO relationship; or
- another product's stored data.

Changing the product invalidates all active product WebSockets. Clients must
reconnect; new connections receive the new product context. Selecting the
already-current normalized id does not reset connections.

Product-scoped entropy is intentionally different for different product ids
even when the account and caller context bytes are identical.

## 8. Slash commands

Commands start with `/`. There are no `q`, `quit`, `exit`, or non-slash aliases.

| Command | Pairing host | Signing host | Behavior |
| --- | :---: | :---: | --- |
| `/script` | yes | yes | Edit and run the remembered script, creating a scratch script when needed. |
| `/script <path>` | yes | yes | Remember and run an existing JS/TS script. |
| `/login` | yes | no | Start or join pairing for the current product and copy the new link. |
| `/logout` | yes | no | Disconnect and clear the old pairing identity/history. |
| `/pair <url>` | no | yes | Validate and answer a `polkadotapp://pair?...` link. |
| `/product` | yes | yes | Print the current product id. |
| `/product <id>` | yes | yes | Switch product and reset product connections. |
| `/session` | no | yes | Show current session, user, and path. |
| `/session <name>` | no | yes | Switch to or create and provision a session. |
| `/session --list` | no | yes | List network-scoped user sessions and mark the active one. |
| `/log <level>` | yes | yes | Replace the runtime log filter. |
| `/help` | yes | yes | Show role-specific commands and key bindings. |
| `/clear` | yes | yes | Clear the retained visible transcript. |
| `/copy` | yes | yes | Copy the retained, redacted transcript. TUI only. |
| `/quit` | yes | yes | Leave the command loop. |

The shared parser recognizes every command, then the active role rejects
commands it cannot execute. `/pair` performs a fast prefix check; the Rust core
then fully decodes and validates the V2 handshake.

Unknown commands, missing required arguments, invalid log levels, invalid
products, invalid session names, and arguments passed to no-argument commands
produce explicit errors.

## 9. Terminal UI

### 9.1 Layout

Both roles use the same full-screen ratatui/crossterm surface:

```text
scrollable transcript

command completion list, when open
› command input or idle placeholder
TrUAPI <role> host · 👤 <state-or-name> · 🌐 <network> · 📦 <product>
```

The role label is omitted at narrow widths so user, network, and product remain
visible. Values are ellipsized to fit, with the product consuming the remaining
space after the user and network. Session and log level are not shown. Idle
command guidance appears as a dim placeholder inside the empty prompt instead
of consuming status-bar space. Operational hints temporarily use the right side
of the status line while a command, approval, completion menu, or scroll is
active.

The composer:

- has one column of horizontal padding when width permits;
- adds vertical padding on terminals at least seven rows high;
- blends a subtle surface color from `COLORFGBG`;
- uses true color when `COLORTERM` is `truecolor` or `24bit`;
- falls back to an ANSI-256 approximation; and
- becomes unstyled when `NO_COLOR` exists.

### 9.2 Transcript

The transcript contains:

- host lifecycle events;
- submitted commands;
- script stdout and stderr;
- approvals;
- SSO request/response summaries; and
- tracing output allowed by the active log filter.

Submitted commands become bold, full-width divider titles. `/pair` arguments
are rendered as `/pair <pairing link>`.

Status symbols are:

| Symbol | Meaning |
| --- | --- |
| `•` | informational |
| `◌` | running |
| `✓` | success |
| `!` | warning |
| `×` | failure |
| `–` | cancelled |

Running activities are keyed and updated in place. For example, `Script
running` becomes `Script finished` or `Script failed`, and pairing progresses
from link generation through authentication to its final state.

### 9.3 Input and completion

- Typing `/` opens role-specific completion.
- Up/Down cycles completion while it is visible.
- Up/Down navigates process-local command history when completion is closed.
- Tab accepts the selected completion.
- Enter first accepts a differing selected completion; a later Enter submits.
- `/script` followed by a space completes filesystem entries.
- `/session` followed by a space completes known signing sessions and `--list`.
- Left/Right, Home/End, Backspace, and Delete edit by Unicode character.
- Long input scrolls horizontally and retains a native terminal cursor.
- Bracketed paste is enabled; pasted control characters are discarded.
- At most eight completion rows are visible.

Command history is in memory only and disappears when the process exits.

### 9.4 Scrolling and cancellation

- Ctrl-U scrolls up by half the transcript viewport.
- Ctrl-D scrolls down by half the viewport.
- End moves the input cursor to the end and resumes latest-output view.
- Esc dismisses completion.
- Ctrl-C clears non-empty input.
- Idle Ctrl-C exits the command loop when input is empty.
- Busy Ctrl-C drops the active operation future.

Only one operator command runs at once. Input may be prepared while a command
runs, but Enter reports that another command is active. Host events and
approval input continue to be processed during the operation.

Captured script children use `kill_on_drop`, so cancelling an interactive
script terminates Bun. Pairing-host `/login` additionally calls the core's
pairing cancellation method.

### 9.5 Approvals in the TUI

An approval temporarily saves and clears the command draft. The operator can:

- press `y` or `Y` with an empty input to approve;
- press `n`, `N`, or Esc to reject; or
- type `yes`/`no` and press Enter.

Invalid typed answers show `Answer yes or no`. The saved draft is restored
after the decision. Approval requests are serialized by the platform prompt
lock.

### 9.6 Clipboard and redaction

`/copy` lazily opens the system clipboard and copies plain transcript text
without the full-screen UI. Complete pairing links are replaced by
`<pairing link>`.

Operator `/login` copies the first generated pairing link automatically. A
clipboard failure is reported as a warning and does not cancel pairing.
Product-driven `requestLogin()` does not automatically copy its link.

### 9.7 Output safety and bounds

Captured script and log text is sanitized before rendering:

- CSI escape sequences are removed;
- OSC escape sequences are removed;
- other control characters are removed except newline and tab; and
- individual child-output lines are truncated at 16 KiB.

Consequently Chalk color, bold, and other ANSI styling are not rendered inside
the TUI. One-shot `--script` inherits stdout and can render ANSI normally.

The retained transcript is pruned from the oldest item when any limit is
exceeded:

- 10,000 feed items;
- 10,000 logical lines; or
- 1 MiB of retained plain text.

Adjacent output is chunked at 256 lines or 64 KiB.

## 10. Product scripts

### 10.1 Execution contract

A product script is a JavaScript or TypeScript ES module executed by Bun.
Before importing it, the runner:

1. reads its required environment;
2. opens the product-frame WebSocket, with a 15-second connection timeout;
3. creates the public `@parity/truapi` client;
4. injects the script globals; and
5. imports the absolute script URL.

Top-level module code is awaited. If the module's default export is a function,
the runner calls and awaits it with the host context.

The provider is disposed on success or failure.

### 10.2 Injected globals

```ts
declare const truapi: TrUApiClient;

declare const host: {
  productId: string;
  productAccount(index?: number): ProductAccountId;
};

declare function assert(
  condition: unknown,
  ...message: unknown[]
): asserts condition;
```

`host.productAccount()` defaults to derivation index `0` and uses the exact
active product id.

`assert` joins string arguments directly and formats other values with
`node:util.inspect` without color. A false condition throws either the joined
message or `assertion failed`.

### 10.3 Internal child environment

The Rust parent sets:

| Variable | Meaning |
| --- | --- |
| `TRUAPI_FRAME_URL` | Bound product-frame WebSocket URL. |
| `TRUAPI_PRODUCT_ID` | Normalized active product id. |
| `TRUAPI_SCRIPT` | Canonical absolute script path. |
| `TRUAPI_CLI_HOST_ROLE` | `pairing-host` or `signing-host`. |

These variables are runner internals, not CLI configuration inputs.

### 10.4 Script status

- Successful completion exits `0`.
- A thrown error or rejected promise is printed as `[script error] ...` and
  exits `1`.
- Failure to open the product socket within 15 seconds exits `2`.
- Failure to locate the runner, canonicalize the script, or spawn Bun is a CLI
  error.

The CLI emits `Script running` before Bun starts and `Script finished` or
`Script failed` afterward.

One-shot `--script` preserves the child's normal numeric status. Interactive
script failures are displayed and the TUI remains active. `exec '/script
<path>'` reports the child code but returns the CLI's general error status when
the child failed.

### 10.5 Remembered scripts and editor behavior

`/script <path>` resolves a relative path against the CLI process's current
working directory, remembers the resulting absolute path in the current host
session, and runs it.

A later bare `/script`:

1. reuses the remembered file when it still exists;
2. otherwise creates a unique `script-<time>-<pid>-<sequence>.ts` under the
   current state directory's `scripts/`;
3. stores that selection;
4. leaves the TUI;
5. opens the file in the configured editor;
6. restores the TUI; and
7. runs the script when the editor exits successfully.

Editor selection order:

1. non-empty `VISUAL`;
2. non-empty `EDITOR`;
3. `notepad` on Windows; or
4. `vi` elsewhere.

The editor specification is parsed with shell-like quoting but is launched
directly, without a shell. Values such as `EDITOR='code --wait'` work.

An editor failure retains the script and does not run it.

Scratch scripts store only their filename in `session.json`, so they remain
valid if a session directory is promoted. Explicit scripts outside the session
store their absolute path. A missing remembered file is ignored and replaced
by a new scratch file.

Mnemonic-backed ephemeral signing sessions remember a path only for the
current process and create scratch files under the system temporary
`truapi-host/scripts` directory.

The top-level `--script` option does not update remembered `/script` state.

### 10.6 Shipped scripts

`rust/crates/truapi-host-cli/js/scripts/` contains:

| Script | Purpose |
| --- | --- |
| `battery.ts` | Run every generated Playground example and write the role-specific compatibility report. |
| `whoami.ts` | Print the primary username. |
| `signing-smoke.ts` | Focused product-account signing test. |
| `ring-vrf-smoke.ts` | Verify alias/proof behavior for the Paseo Next v2 LitePeople ring. |
| `preimage-smoke.ts` | Exercise Bulletin preimage submission and lookup. |

`battery.ts` writes to `explorer/diagnosis-reports/<role>-cli.md` unless
`TRUAPI_BATTERY_REPORT_PATH` overrides the destination. Its custom reporter
uses terminal color when stdout is a TTY or `FORCE_COLOR` is nonzero, unless
`NO_COLOR` exists.

## 11. Pairing lifecycle

### 11.1 Login

Login may start from:

- a product calling `truapi.account.requestLogin()`;
- operator `/login`; or
- an already-running product script.

The pairing host generates or reuses its current pairing device identity,
publishes the handshake proposal through the People-chain Statement Store, and
emits a `polkadotapp://pair?...` link.

The observable states are:

```text
disconnected
  -> pairing link ready
  -> authenticating
  -> paired with <user>
```

Failures become `Pairing failed`. A rejected login returns `Rejected`; a
connected session can return `AlreadyConnected`.

`/login` uses the product selected at the moment the command starts. Ctrl-C
cancels that login attempt.

### 11.2 Signing-host response

Before a signing host answers a link, it:

1. ensures a signer;
2. decodes the V2 handshake;
3. derives its `//wallet//sso` account;
4. reads the pairing device Statement Store account from the proposal;
5. finds the signer's LitePeople ring, scanning back from the current ring;
6. grants or reuses Statement Store allowance for `wallet-sso`;
7. grants or reuses allowance for the pairing device; and
8. starts the real SSO responder.

`/pair` replaces an existing background responder after preparation succeeds.
The old task is aborted. The responder reports its final protocol outcome or
failure.

In `exec`, `/pair` waits for the responder to finish. With `--deeplink` plus
another `exec` command or a script, the responder runs only for that command's
lifetime and is then aborted.

### 11.3 Logout and re-pairing

Pairing-host `/logout`:

- invalidates an in-flight login;
- disconnects the active account-authority session;
- clears the persisted auth session;
- tries to publish the disconnected SSO message;
- deletes `PairingDeviceIdentity`; and
- deletes `LastProcessedPairingStatement`.

The next login generates a fresh pairing keypair/topic and can pair with a
different signing host.

Logout does not clear:

- product storage;
- non-auth core state;
- scripts;
- the selected product; or
- another user's identity-scoped directory.

## 12. Signing identities, accounts, and sessions

### 12.1 Base path

Host commands choose their base path in this order:

1. explicit `--base-path`;
2. `TRUAPI_HOST_BASE_PATH`;
3. `$XDG_STATE_HOME/truapi-host`;
4. `$HOME/.local/state/truapi-host`; or
5. `.truapi-host`.

The signing session catalog converts a relative base path to an absolute path
at startup. Pairing storage uses the supplied/default path directly.

### 12.2 Signer selection

Signing-host signer selection order is:

1. explicit `--mnemonic` or `HOST_CLI_SIGNER_MNEMONIC`;
2. explicit `--account`;
3. the first attested, non-exhausted auto account for the network and current
   Statement Store period;
4. the first pending auto account for the network; or
5. a newly generated auto account.

Explicit mnemonic mode:

- parses a 12/15/18/21/24-word BIP-39 phrase;
- uses the raw BIP-39 entropy, not the PBKDF2 seed;
- does not read or write an account record;
- has no cached username;
- reports the session as `ephemeral`; and
- disables `/session <name>`.

Explicit account mode looks up a named record in the default account store and
ensures its on-chain identity and ring readiness. It is not considered
auto-managed for slot rotation.

### 12.3 Auto-account onboarding

A new auto account:

1. acquires `accounts.json.lock`;
2. generates a 12-word mnemonic;
3. derives the `//wallet//sso` sr25519 account;
4. chooses `auto-<n>` as its local name;
5. tries up to eight available Lite username bases;
6. saves a pending account record;
7. builds and submits identity-backend registration proofs;
8. polls `Resources.Consumers` for the final `name.discriminator`;
9. waits for inclusion in a LitePeople ring; and
10. marks and saves the account as attested.

Identity and ring polling each allow 10 attempts with four seconds between
attempts. Identity-backend HTTP clients use a 30-second timeout.

The default Lite username prefix is `headless`. For a non-default session, the
prefix is its lowercase letters with digits and separators removed; a name
with no letters becomes `session`. `--lite-username-prefix` overrides this and
must contain lowercase ASCII letters only.

The generated base is at least 12 characters and, for prefixes of six or more
characters, appends six pseudo-random lowercase letters.

### 12.4 Cached startup

An attested account with a resolved Lite username can be loaded and activated
from `accounts.json` without contacting the identity backend or checking ring
membership on every restart. The current Statement Store period is still used
to skip locally marked exhausted accounts.

### 12.5 Statement Store slot rotation

Before pairing, allowance is registered for both the signing wallet and device
accounts. If registration reports no free slot and the signer is auto-managed,
the CLI:

1. records the current Statement Store period in that account;
2. selects or creates another account;
3. activates the new signer; and
4. retries pairing preparation.

At most eight rotations are attempted. Explicit mnemonic and explicit account
modes return the slot error instead of changing identity.

### 12.6 Session identity and naming

Managed session names must:

- contain 1 to 64 ASCII characters;
- start with a lowercase letter or digit;
- contain only lowercase letters, digits, `.`, `_`, and `-`; and
- not be `.` or `..`.

At startup the initial session is:

1. `ephemeral` for explicit mnemonic mode;
2. explicit `--session`;
3. `default` for explicit `--account`; or
4. the network's remembered `current-session`.

`default` is a compatibility/bootstrap session. Once an auto-managed signer is
known, the public and durable session name becomes its Lite username and its
directory becomes `<username>_signing_host`. The bootstrap name is not
user-selectable and is omitted from session completion and listing.

### 12.7 Session inspection and switching

`/session` reports:

- `ephemeral` or the profile name;
- `<not provisioned>` or the known Lite username; and
- `<none>` or the filesystem path.

When a managed session has no connected user, startup and bare `/session` add
an actionable transcript notice directing the user to `/session <name>`.

`/session --list` includes:

- legacy directories under `signing-host/sessions/`; and
- network directories ending in `_signing_host`.

The active session is marked with `*`.

`/session <name>` provisions the target before replacing the current runtime:

1. validate and create its provisional profile;
2. resolve or create its signer;
3. promote it to the resolved username directory;
4. load its remembered script and storage;
5. build and activate the replacement runtime;
6. persist `current-session`;
7. stop any pairing responder;
8. swap the runtime;
9. disconnect product WebSockets using the old runtime; and
10. update status and completion.

If activation fails, the previous `current-session` pointer is restored and the
old in-memory runtime remains active. Files created while preparing the target
may remain.

## 13. Persistence

### 13.1 Current layout

The layout may contain compatibility paths as well as identity-owned paths:

```text
<base-path>/
  accounts.json
  accounts.json.lock

  <network>/
    signing-host/
      current-session
      session.json                    # default/bootstrap metadata, when used
      core-storage.json               # default/bootstrap core state
      scripts/
      storage/
        default/
          <product-file>.json
      sessions/                       # accepted legacy session layout
        <legacy-name>/

    pairing-host/
      current-user
      session.json                    # bootstrap script metadata, when used
      core-storage.json               # bootstrap auth/core state
      scripts/
      storage/                        # or legacy storage/default/

    <username>_signing_host/
      accounts.json
      accounts.json.lock
      session.json
      core-storage.json
      scripts/
      storage/
        <product-file>.json

    <username>_pairing_host/
      session.json
      core-storage.json
      scripts/
      storage/
        <product-file>.json
```

An explicit mnemonic has no account profile, but its signing runtime uses the
default/bootstrap signing storage path for core and product state.

### 13.2 Pairing-user storage switching

Before the first resolved user, pairing state uses
`<network>/pairing-host`. On connection:

- storage identity prefers the Lite username and falls back to the full
  username;
- display identity prefers the full username and falls back to the Lite
  username;
- the target is `<username>_pairing_host`; and
- `pairing-host/current-user` is updated atomically.

For the first bootstrap migration, in-memory bootstrap core and product values
move into the resolved user's directory.

When switching from one resolved user to another, only transient authentication
keys move:

- `AuthSession`;
- `PairingDeviceIdentity`; and
- `LastProcessedPairingStatement`.

The previous user's product KV and other core state remain in that user's
directory. The new user's existing product/core state is loaded.

Remembered pairing scripts are scoped to whichever bootstrap/user directory is
active. Resolving another user loads that directory's `session.json`.

### 13.3 Session metadata

`session.json` is version `1` and may contain:

```json
{
  "version": 1,
  "user_id": "alice.dot",
  "last_script": "script-....ts"
}
```

Scratch scripts use a single portable filename and must resolve inside the
session's `scripts/` directory. Explicit external scripts use an absolute path.
Invalid multi-component relative values are rejected. Missing scripts are
treated as not remembered.

### 13.4 Product storage

Each normalized product has one file:

```text
storage/<slug>--<sha256(product-id)>.json
```

The slug:

- retains ASCII letters, digits, `.`, and `-`;
- replaces other characters with `-`;
- is limited to 48 characters;
- trims leading/trailing `.` and `-`; and
- falls back to `product`.

The full SHA-256 digest prevents slug collisions.

The version `1` JSON document is:

```json
{
  "version": 1,
  "productId": "example.dot",
  "values": {
    "raw-product-key": "hex-encoded-value"
  }
}
```

The core has already removed its product namespace before the CLI stores the
raw key. Identity and host role are isolated by the parent directory.

Legacy combined `product-storage.json` keys are decoded with
`ProductStorageKey` and split into per-product files. A fully safe migration is
retained as `product-storage.v1.json.migrated`. An undecodable legacy key or
document prevents the backup rename.

Noncanonical product filenames, unsupported versions, invalid ids, and invalid
hex values are ignored with warnings.

### 13.5 Core storage

`core-storage.json` is a versionless JSON object whose keys and values are hex:

```json
{
  "values": {
    "<SCALE-encoded CoreStorageKey hex>": "<value hex>"
  }
}
```

Core state includes auth sessions, pairing bootstrap material, permission
state, and other role-owned runtime data.

### 13.6 Account store

`accounts.json` is versioned and stores records containing:

- local name;
- network id;
- plaintext BIP-39 mnemonic;
- final Lite username;
- `//wallet//sso` public key and address;
- creation timestamp;
- attested state; and
- exhausted Statement Store periods.

Account mutations hold an exclusive `accounts.json.lock`. Secret-file writes
use a temporary file, flush, atomic rename, and `0600` permissions on Unix.
The lock file can be created during a read-only cached-signer lookup.

### 13.7 Write and corruption behavior

Product and core storage writes:

- create parent directories;
- write a process/id-specific temporary file;
- flush file data;
- atomically rename;
- and sync the parent directory on Unix.

Session metadata and current-user/session pointers use temporary-file rename
but do not apply the account file's explicit secret permissions.

Malformed account or session JSON is a startup/command error. Malformed core
JSON is warned about and loaded as empty. Malformed product files are warned
about and skipped.

There is no session-wide process lock. The account store is locked, but
simultaneous processes can still race on session, core, product, and current
selection files.

## 14. Network and transport

### 14.1 Network preset

v0.1 supports only `paseo-next-v2`.

| Purpose | Value |
| --- | --- |
| Identity backend | `https://identity-backend-next.parity-testnet.parity.io/api/v1` |
| People RPC | `wss://paseo-people-next-system-rpc.polkadot.io` |
| People genesis | `0xc5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5` |
| Bulletin RPC | `wss://paseo-bulletin-next-rpc.polkadot.io` |
| Bulletin genesis | `0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22` |
| Asset Hub RPC | `wss://paseo-asset-hub-next-rpc.polkadot.io` |
| Asset Hub genesis | `0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f` |

There are no public endpoint override flags.

People and Bulletin routes are always enabled because host internals require
them. Asset Hub routing is enabled only when `E2E_LIVE_CHAIN=1`.

The all-zero SSO sentinel and every genesis hash not present in the active
route map fall back to the People RPC.

A rustls ring crypto provider is installed at process startup for `wss://`
connections.

### 14.2 Product-frame WebSocket

The listener uses plain `ws://`. Defaults are loopback, but any
`SocketAddr` accepted by the OS can be supplied. v0.1 has no authentication,
TLS, origin check, or non-loopback warning.

Each accepted WebSocket:

- snapshots the current normalized product;
- creates one `ProductRuntime`;
- forwards each incoming binary message as one protocol frame;
- also forwards a text message's UTF-8 bytes as a protocol frame;
- sends each emitted runtime frame as one binary message;
- disposes the runtime on close/error/reset; and
- closes on product selection or signing-runtime replacement.

Product WebSocket connections do not survive `/product` or signing-session
switches.

The frame writer uses an unbounded channel. The accept loop retries listener
accept errors. Each connection runs independently on the Tokio worker pool;
the shared service-trait contract requires dispatch futures to be `Send`.

### 14.3 Chain JSON-RPC

Every chain connection opens a fresh WebSocket. Outgoing requests use an
unbounded channel. Incoming text frames and UTF-8 binary frames enter a
1,024-message broadcast buffer.

The first response receiver is created before the reader task, preventing a
fast initial RPC response from being lost. A lagged subscriber drops missed
responses and emits a warning. `close()` prevents further sends; background
socket tasks end when their channels or sockets close.

## 15. TrUAPI capability surface

The direct and paired compatibility reports currently expose the same method
surface.

| Service | Implemented behavior |
| --- | --- |
| Account | Connection status, product accounts, aliases, proofs, empty legacy-account list, user id, and login. |
| Chain | chainHead-v1 follow/header/body/storage/call/unpin/continue/stop, chain spec queries, transaction broadcast/stop. Asset Hub needs `E2E_LIVE_CHAIN=1`. |
| Entropy | Product-scoped deterministic entropy from the active account/session. |
| Local Storage | Persistent product-scoped read, write, and clear. |
| Notifications | In-process immediate/scheduled delivery and cancellation with transcript events. |
| Permissions | Device and remote permission approval through the CLI policy. |
| Preimage | Real Bulletin submission/lookup path plus bounded in-core read-after-write cache. |
| Resource Allocation | Real host-managed allocation, including Bulletin long-term storage over SSO. |
| Signing | Product and legacy transaction construction, raw signing, and payload signing. |
| Statement Store | Real subscribe, proof, authorized proof, and submit over People. |
| System | Handshake, feature query, and no-op navigation. |
| Theme | One `Dark` subscription value. |
| Chat | Typed unavailable/empty-subscription behavior. |
| Coin Payment | Typed unavailable/interrupted-subscription behavior. |
| Payment | Typed unsupported/interrupted-subscription behavior. |

### 15.1 Exact reported methods

Implemented-success methods in the checked-in direct and paired battery
reports:

- `Account/connection_status_subscribe`
- `Account/get_account`
- `Account/get_account_alias`
- `Account/create_account_proof`
- `Account/get_legacy_accounts`
- `Account/get_user_id`
- `Account/request_login`
- `Chain/follow_head_subscribe`
- `Chain/get_head_header`
- `Chain/get_head_body`
- `Chain/get_head_storage`
- `Chain/call_head`
- `Chain/unpin_head`
- `Chain/continue_head`
- `Chain/stop_head_operation`
- `Chain/get_spec_genesis_hash`
- `Chain/get_spec_chain_name`
- `Chain/get_spec_properties`
- `Chain/broadcast_transaction`
- `Chain/stop_transaction`
- `Entropy/derive`
- `Local Storage/read`
- `Local Storage/write`
- `Local Storage/clear`
- `Notifications/send_push_notification`
- `Notifications/cancel_push_notification`
- `Permissions/request_device_permission`
- `Permissions/request_remote_permission`
- `Preimage/lookup_subscribe`
- `Preimage/submit`
- `Resource Allocation/request`
- `Signing/create_transaction`
- `Signing/create_transaction_with_legacy_account`
- `Signing/sign_raw_with_legacy_account`
- `Signing/sign_payload_with_legacy_account`
- `Signing/sign_raw`
- `Signing/sign_payload`
- `Statement Store/subscribe`
- `Statement Store/create_proof`
- `Statement Store/submit`
- `Statement Store/create_proof_authorized`
- `System/handshake`
- `System/feature_supported`
- `System/navigate_to`
- `Theme/subscribe`

Deliberately unavailable methods:

- all six generated Chat methods;
- all nine generated Coin Payment methods; and
- all four generated Payment methods.

A successful `System/feature_supported` call returns `supported: false` for
every queried feature in the CLI platform. Success means the method is wired,
not that every feature is present.

### 15.2 Platform-specific semantics

Navigation logs the requested URL at debug level and returns success; it does
not open a browser.

Notification ids start at `1` per process. A future `scheduledAt` value is
treated as Unix milliseconds and delivered by a Tokio timer. At most 64
notifications may be pending. Cancelling an unknown/already-delivered id is a
successful no-op.

The platform-level preimage lookup map is in memory. The core also owns the
real Bulletin client and a separate 16 MiB insertion-ordered preimage bridge
for read-after-write behavior.

Statement Store keeps up to 64 accepted statements in an insertion-ordered
read-after-write bridge until the remote subscription reports them.

## 16. Entropy, accounts, and authority policy

The signing host uses raw BIP-39 entropy. Product entropy applies three
BLAKE2b-256 layers:

1. keyed root source using domain `product-entropy-derivation`;
2. a product layer keyed by the hash of the normalized product id; and
3. a caller-context layer keyed by 1 to 32 context bytes.

During pairing, only the pre-hashed root entropy source is shared with the
pairing host. Therefore paired and direct hosts derive the same value only
when all three are the same:

- signing account/root entropy;
- normalized product id; and
- context bytes.

Product accounts, entropy, and Ring-VRF contexts reject cross-product use
unless the core policy and any required user confirmation allow it.

Same-product Ring-VRF alias/proof requests follow the signing-runtime policy
and do not add a CLI-only prompt.

## 17. Approvals and security behavior

### 17.1 Prompt policy

Without `--auto-accept`, platform approval is deny-by-default.

In the TUI it uses the approval card described in section 9. In plain mode:

- stdin must be a TTY;
- the prompt is `Approve? [y/N]`;
- only `y` or `yes` approves; and
- EOF, invalid input, or non-TTY stdin rejects.

Approval summaries exist for:

- SCALE payload signing;
- raw-data signing;
- transaction construction;
- account alias derivation;
- account proof creation;
- identity disclosure;
- resource allocation;
- preimage submission;
- cross-product account access;
- device permission; and
- remote permission.

Raw signing payloads are hidden from approval summaries. Proof summaries show
only product and message length.

### 17.2 Auto-accept

`--auto-accept` returns `true` for each platform prompt and emits:

```text
✓ Approved <action> automatically
  <redacted summary>
```

It does not bypass product-id validation or core authorization rules.

### 17.3 Sensitive state and output

The dedicated pairing-link event necessarily prints the complete deeplink in
plain mode and shows it in the live TUI. Treat captured output as sensitive.
Transcript copies and submitted-command dividers redact it.

Mnemonics are never intentionally printed, but auto-managed mnemonics are
stored in plaintext `accounts.json`. That file is local test secret material,
not production custody.

`debug` and especially `trace` can include decoded product payloads and
transport metadata. Do not publish trace logs from sensitive test accounts
without review.

## 18. Events, output, and logging

### 18.1 Human output

v0.1 has one human output format. There is no JSON or JSONL mode.

Outside the TUI:

- lifecycle and command results use stdout;
- tracing and many diagnostics use stderr through the tracing writer;
- Clap and explicit invocation errors use stderr; and
- script stdio is inherited in top-level `--script` mode.

Representative output:

```text
• Listening for product frames
  ws://127.0.0.1:<port>
✓ Paired
✓ Signing host ready
◌ Script running
✓ Script finished
```

The same `SystemEvent` presentation code supplies plain and TUI wording.

### 18.2 Lifecycle events

The CLI exposes events for:

- frame listener readiness;
- signing-host readiness;
- exhausted signer-account rotation;
- responder start/stop/failure;
- product connection reset after session/profile replacement;
- LitePeople ring discovery;
- wallet and device allowance preparation/results;
- notification scheduling/delivery/cancellation;
- pairing link/authentication/connection/disconnection/failure;
- script start/exit;
- session status/create/switch;
- log-level change; and
- transcript copy.

### 18.3 SSO transcript

SSO summaries use a dedicated tracing layer and remain visible at every log
level. Request and response events with the same Statement Store request id
update one transcript row.

When available, rows contain:

- humanized request name;
- statement request id;
- remote/response message id;
- protocol outcome;
- elapsed milliseconds; and
- encoded error reason.

Fallback SSO summary text is still shown when structured fields are absent.

### 18.4 Log filtering

Without `RUST_LOG`, the selected CLI level applies to:

- `truapi`
- `truapi_host`
- `truapi_platform`
- `truapi_server`

Other targets remain at `warn`.

The following noisy targets are always hidden from the ordinary CLI log layer:

- `truapi_server::sso_transcript` (handled by its dedicated layer);
- `rustls` and its children; and
- `tungstenite::protocol` and its children.

Logging ANSI is disabled before messages enter the transcript.

## 19. Diagnostic commands

### 19.1 `identity-check`

```text
truapi-host identity-check \
  --mnemonic <BIP-39> \
  [--network paseo-next-v2]
```

The command derives and queries three accounts:

- root;
- `//wallet`; and
- `//wallet//sso`.

For each it prints one of:

```text
IDENTITY_FOUND path=<path> account=<ss58> username=<name>
IDENTITY_NONE path=<path> account=<ss58>
IDENTITY_ERROR path=<path> account=<ss58> error=<reason>
```

Per-path RPC errors are printed and do not make the command itself fail.
Mnemonic parsing failures do fail the command. The mnemonic is not persisted.

### 19.2 `alloc-check`

```text
truapi-host alloc-check \
  --mnemonic <BIP-39> \
  [--network paseo-next-v2] \
  [--target <32-byte-hex>] \
  [--lookback 8] \
  [--submit]
```

It prints:

- runtime spec version;
- transaction version;
- genesis hash;
- derived bandersnatch member key;
- current ring index;
- matching ring details or onboarding-pending status;
- current allowance period;
- target account;
- free/already-allocated slot or scan error; and
- submission result when requested.

Without `--target`, the target is all zeroes and the command is scan-only.
`--submit` requires an explicit 32-byte target. `0x` is optional on target
hex.

Submission uses the shared metadata-driven
`set_statement_store_account` implementation and reuses an existing allocation
when present.

## 20. Exit status and shutdown

| Status | Meaning |
| --- | --- |
| `0` | Successful command, diagnostic, or script. |
| `1` | General runtime/state/network error, invalid product at runtime construction, or failed `exec` script. |
| `2` | Clap/explicit invocation error, non-TTY interactive use, malformed slash command passed to `exec`, or runner connection timeout in top-level script mode. |
| child status | Top-level pairing/signing `--script` preserves a normal Bun exit status. |

Interactive command errors do not terminate the host. They finalize running
activities, display the error, and return to the command bar.

Dropping a `SigningHostSession` aborts its background responder. Leaving the
TUI restores the cursor, bracketed-paste mode, alternate screen, and raw mode.
The frame accept task is aborted when its owning command body completes.

The CLI has no explicit SIGTERM/SIGINT signal orchestration. Interactive
Ctrl-C is handled as a terminal key; external process termination follows
normal operating-system behavior.

Top-level `--script` uses `std::process::exit` after the frame-server scope has
ended. This preserves the child status but bypasses later Rust destructors.

## 21. Environment variable reference

| Variable | Scope |
| --- | --- |
| `TRUAPI_HOST_LOG` | Default `--log-level`. |
| `RUST_LOG` | Full startup tracing filter. |
| `TRUAPI_HOST_BASE_PATH` | Default `--base-path`. |
| `HOST_CLI_SIGNER_MNEMONIC` | Signing, identity, and allowance mnemonic input. |
| `XDG_STATE_HOME` | Preferred default state parent. |
| `HOME` | Fallback default state parent. |
| `VISUAL` | Preferred script editor. |
| `EDITOR` | Fallback script editor. |
| `TRUAPI_HOST_RUNNER` | Override `js/runner.ts`. |
| `E2E_LIVE_CHAIN` | Value `1` enables optional Asset Hub routing. |
| `NO_COLOR` | Disable CLI semantic colors and battery reporter color. |
| `COLORFGBG` | Infer TUI background color. |
| `COLORTERM` | Select true-color TUI rendering. |
| `FORCE_COLOR` | Force battery reporter color in non-TTY output. |
| `TRUAPI_BATTERY_REPORT_PATH` | Override battery report destination. |

## 22. Current v0.1 operational constraints

These are part of the as-built specification:

- only `paseo-next-v2` is selectable;
- product scripts require Bun and, by default, the source checkout;
- there is no structured/JSON output mode;
- there is no `--version`;
- there is no script timeout option;
- there is no global signal-aware graceful-shutdown controller;
- onboarding can wait for the fixed identity/ring polling windows;
- session/core/product state has no inter-process mutation lock;
- corrupt core storage is treated as empty after a warning;
- non-loopback product listeners have no authentication or warning;
- product text WebSocket frames are accepted as protocol bytes;
- product-frame and chain outbound queues are unbounded;
- unknown chain genesis hashes fall back to People;
- interactive child ANSI styling is stripped rather than parsed; and
- pairing and signing state are local plaintext test state.

## 23. Verification contract

The implementation is covered by:

- CLI unit tests for parsing, completion, TUI rendering, storage, accounts,
  products, sessions, approvals, and platform behavior;
- process-boundary tests for help, non-TTY rejection, product reporting,
  session restore, cached signer activation, and bare-script safety;
- `truapi-server` runtime, protocol, cryptographic vector, and integration
  tests;
- script-runner/Bun diagnosis tests;
- paired and direct `battery.ts` runs; and
- checked-in compatibility reports:
  - `explorer/diagnosis-reports/pairing-host-cli.md`
  - `explorer/diagnosis-reports/signing-host-cli.md`

The reports currently have identical method results apart from their title:

- 45 implemented-success methods;
- 6 unavailable Chat methods;
- 9 unavailable Coin Payment methods; and
- 4 unavailable Payment methods.

Recommended local verification after CLI changes:

```sh
cargo fmt --all -- --check
cargo clippy -p truapi-host-cli --all-targets -- -D warnings
cargo test -p truapi-host-cli
git diff --check
```
