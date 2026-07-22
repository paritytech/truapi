# Headless Host CLI specification

Status: implementation contract for the `headless-host` worktree  
Target: `truapi-host` v0.1  
Reference implementation: `rust/crates/truapi-host-cli/`  
Protocol source of truth: `truapi` and `truapi-server`

This document defines the product and engineering contract for completing the
native headless TrUAPI host CLI. The crate README is the user guide; this file
is the implementation and acceptance specification. If code, help text, tests,
and this document disagree, an agent must resolve the disagreement explicitly
rather than silently preserving both behaviors.

The key words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative.

## 1. Product definition

`truapi-host` is a native developer tool for running real TrUAPI host roles
without a browser UI or phone automation service. It is intended for local
development, protocol diagnosis, and end-to-end product tests.

The CLI runs the real `truapi-server` dispatcher, services, signing logic, SSO
logic, and protocol frames. It replaces only the operating-system seam with a
CLI platform and adds process orchestration around it.

The primary success case is:

1. A seedless pairing host serves a product and publishes a Polkadot Mobile
   pairing deeplink.
2. A wallet-local signing host answers that deeplink over the real SSO
   transport.
3. A product script calls the public `@parity/truapi` client exactly as a real
   product would.
4. The script exit status is the test result.

The CLI is experimental test infrastructure. It is not a production wallet,
general-purpose key manager, mock host, or replacement for dot.li.

## 2. Goals and non-goals

### 2.1 Goals

The v0.1 CLI MUST:

- exercise the real Rust core and SCALE protocol frame path;
- support both paired and direct signing-host product execution;
- pair over the selected network's real Statement Store;
- manage a reusable local signer without requiring an external signing bot;
- expose safe interactive approvals and explicit unattended auto-approval;
- provide persistent, isolated signing-host sessions;
- run JS or TS product scripts and faithfully return their status;
- provide enough stable machine output for shell and CI orchestration;
- terminate predictably under errors, timeouts, cancellation, and signals;
- be installable and runnable without retaining the repository checkout; and
- have deterministic local tests plus a separately identified live-network
  acceptance suite.

### 2.2 Non-goals

The v0.1 CLI MUST NOT:

- redefine protocol payloads, wire ids, signing rules, SSO messages, allowance
  proofs, or Bulletin submission logic;
- add CLI-only cryptographic implementations when the behavior belongs in
  `truapi-server`;
- present itself as secure custody for production funds;
- make Chat, Coin Payment, or Payment services work without their real host
  backends;
- accept arbitrary RPC URLs as a normal user-facing configuration surface;
- emulate push delivery or navigation beyond reporting the host action; or
- make live-network tests part of the deterministic unit-test contract.

### 2.3 Implementation baseline

This status snapshot is based on worktree commit `c19c22ab`. Agents must update
it when a work package lands.

Implemented and expected to be preserved:

- [x] four top-level commands and the current flag-conflict validation;
- [x] real paired and direct signing-host runtimes;
- [x] real People-chain SSO, Statement Store allowance, and Bulletin flows;
- [x] auto-managed signer creation, onboarding, caching, and slot rotation;
- [x] signing-host TUI, slash commands, serialized approvals, and plain `exec`;
- [x] persistent named sessions and runtime replacement on session switch;
- [x] durable scratch scripts opened through the configured editor;
- [x] product scripts with injected `truapi` and `host` globals; and
- [x] SSO request/response outcome reporting.

Known completion work:

- [ ] define the documented Make build/install targets;
- [ ] package a relocatable script runner and client dependencies;
- [ ] add named deadlines, setup-time cancellation, and signal-driven cleanup;
- [ ] add session-level locking and atomic restrictive storage writes;
- [ ] bound frame/chain queues and enforce binary product frames;
- [ ] add structured automation output and centralized redaction;
- [ ] expand deterministic process and transport tests;
- [ ] harden the live E2E harness and CI artifact handling; and
- [ ] reconcile stale README/help statements, including removed `/whoami`
      references.

## 3. Ownership boundaries

### 3.1 `truapi-server` owns

- protocol dispatch and frame encoding;
- product, pairing, signing, and authorization semantics;
- SSO request and response encoding;
- product-account derivation and signing;
- ring-VRF aliases and proofs;
- Statement Store and Bulletin allowance proof/extrinsic logic;
- Bulletin preimage construction, submission, and lookup;
- subscription lifecycle rules; and
- reusable runtime errors.

Logic in this list MUST be fixed or extended in `truapi-server`, then consumed
by the CLI. It MUST NOT be duplicated in `truapi-host-cli`.

### 3.2 `truapi-host-cli` owns

- command-line parsing and validation;
- network presets;
- process, task, timeout, cancellation, and signal orchestration;
- terminal UI and plain `exec` output;
- approval prompts and `--auto-accept` policy;
- local state paths, locking, persistence, and file permissions;
- auto-managed test signer selection and onboarding orchestration;
- the product-frame WebSocket listener;
- product-script packaging and execution; and
- CLI-specific diagnostics and exit codes.

### 3.3 Product scripts own

- the sequence of public TrUAPI calls under test;
- assertions over typed protocol results;
- test-specific output; and
- success or failure through normal process completion or a thrown error.

A script MUST NOT reach into the Rust runtime, session files, or private CLI
state to make an assertion pass.

## 4. Runtime topologies

### 4.1 Paired topology

```text
product script
    │ binary SCALE frames over loopback WebSocket
    ▼
pairing-host ── People-chain Statement Store / SSO ── signing-host
    │                                                    │
    └── seedless product/core state                      └── signer + wallet state
```

The pairing host MUST own the product-facing session. The signing host MUST own
wallet entropy and answer SSO requests. Neither process may bypass the real SSO
request/response path for paired tests.

### 4.2 Direct signing-host topology

```text
product script
    │ binary SCALE frames over loopback WebSocket
    ▼
signing-host ── selected network RPCs
```

This topology is for focused signing-host diagnosis. It MUST use the same
signing runtime services as the paired topology. Results that differ between
the direct and paired paths require an explicit test or documented capability
difference.

## 5. Command surface

One binary named `truapi-host` MUST expose these commands:

| Command | Contract |
| --- | --- |
| `pairing-host` | Run the seedless, product-facing host. |
| `signing-host` | Run the wallet-local host and optional product endpoint. |
| `identity-check` | Diagnose the registered identity derived from a mnemonic. |
| `alloc-check` | Diagnose or submit Statement Store allowance registration. |

Global options MUST be accepted before or after the subcommand.

### 5.1 Global options

| Option | Default | Requirement |
| --- | --- | --- |
| `--log-level <level>` | `info` | Accept `error`, `warn`, `info`, `debug`, `trace`. `TRUAPI_HOST_LOG` supplies the default. |
| `--output <mode>` | `human` | Target completion item: accept `human` or `jsonl`; see section 13. Existing human output remains the default. |

`RUST_LOG` MAY provide module-specific startup filtering. CLI filters MUST
always suppress protocol-noise targets known to flood ordinary operation,
including `rustls` and `tungstenite::protocol`. `/log` MAY replace the runtime
filter after startup.

### 5.2 `pairing-host`

| Option | Default | Requirement |
| --- | --- | --- |
| `--script <path>` | none | Run one product script and exit with its status. |
| `--product-id <id>` | `headless-playground.dot` | Scope frames, storage, permissions, and product accounts. Validate at startup. |
| `--frame-listen <addr>` | `127.0.0.1:9955` | Product-frame listener. Port `0` MUST select an available port. |
| `--base-path <path>` | platform state dir | Root of CLI-managed state. `TRUAPI_HOST_BASE_PATH` supplies the default. |
| `--network <preset>` | `paseo-next-v2` | Select a complete network preset. |
| `--auto-accept` | off | Approve confirmations without prompting and report each decision. |
| `--script-timeout <duration>` | none | Target completion item: bound a one-shot script without changing interactive mode. |

With `--script`, the command MUST start the frame listener before starting the
runner, keep the listener alive for the script lifetime, and return the script
status. Without `--script`, it MUST remain available for repeated script runs
until EOF, `quit`, `exit`, `q`, or a termination signal.

The pairing-host interactive surface MAY remain a plain line REPL in v0.1. It
MUST support `script <path>` and MUST NOT claim to provide the signing-host TUI.

### 5.3 `signing-host`

| Option | Default | Requirement |
| --- | --- | --- |
| `--script <path>` | none | Run one direct product script and return its status. |
| `--product-id <id>` | `headless-playground.dot` | Product scope for direct scripts. Validate at startup. |
| `--deeplink <url>` | none | Validate and answer one pairing deeplink after initialization. |
| `--mnemonic <phrase>` | environment or none | Use an explicit ephemeral signer. `HOST_CLI_SIGNER_MNEMONIC` supplies it. |
| `--account <name>` | none | Use a named account in the default-session account store. |
| `--session <name>` | remembered session | Select or create a persistent session. |
| `--lite-username-prefix <prefix>` | session-derived | Prefix new auto-managed Lite usernames. |
| `--base-path <path>` | platform state dir | Root of accounts, sessions, scripts, and runtime state. |
| `--network <preset>` | `paseo-next-v2` | Select identity, People, Bulletin, and chain configuration together. |
| `--frame-listen <addr>` | `127.0.0.1:9956` | Direct product-frame listener. Port `0` MUST work. |
| `--auto-accept` | off | Auto-approve and report each decision. |
| `--script-timeout <duration>` | none | Target completion item: bound a one-shot script without changing interactive mode. |

Invocation validation MUST reject ambiguous ownership combinations before
starting network or filesystem onboarding:

- `--script` with `exec`;
- `--mnemonic` with `--account`, `--session`, or
  `--lite-username-prefix`;
- `--account` with `--session` or `--lite-username-prefix`; and
- invalid session names.

When neither `--script` nor `exec` is present, stdin and stdout MUST both be
TTYs. Otherwise the command MUST exit with invocation status `2` and explain
how to use `exec` or `--script`.

### 5.4 `signing-host exec`

`truapi-host signing-host [OPTIONS] exec '<slash-command>'` MUST:

- avoid raw mode, alternate screens, cursor control, and ANSI output;
- write command results to stdout and diagnostics to stderr;
- execute exactly one slash command;
- never wait indefinitely for an approval on non-TTY stdin;
- exit when that command and its owned cleanup complete; and
- use the same parser and business logic as the interactive TUI.

### 5.5 `identity-check`

`identity-check` MUST validate the BIP-39 mnemonic locally, derive the same
identity paths used by the signing runtime, query the selected People chain,
and print enough information to identify the derivation carrying a username.
It MUST NOT persist the supplied mnemonic.

### 5.6 `alloc-check`

`alloc-check` MUST report network runtime versions, genesis, ring membership,
the selected slot, and the registration outcome. `--submit` MUST require a
32-byte `--target`; the all-zero default is scan-only and MUST never be
submitted. Failures from the shared allowance implementation MUST retain their
specific reason.

## 6. Signing-host terminal UI

The signing-host TUI MUST provide a scrollable transcript and one command bar.
Background logs, SSO events, script output, commands, and approval requests
MUST enter the transcript rather than overwrite user input.

Required slash commands:

| Command | Behavior |
| --- | --- |
| `/deeplink <url>` | Validate and answer a Polkadot Mobile pairing URL. |
| `/script` | Create, edit, save, then run a session-local TS scratch script. |
| `/script <path>` | Run an existing JS/TS product script. |
| `/log <level>` | Change runtime tracing level. |
| `/session` | Show session name, path, and user id without provisioning it. |
| `/session <name>` | Prepare and atomically switch to a persistent session. |
| `/session --list` | List sessions for the active network and mark the current one. |
| `/help` | Show commands and keyboard controls. |
| `/clear` | Clear the visible transcript, not retained process state. |
| `/copy` | Copy the retained transcript. TUI only. |
| `/quit` | Shut down cleanly. |

Required interaction rules:

- Typing `/` opens completion.
- Up/Down selects completion; with the menu closed it navigates process-local
  history.
- Tab accepts completion.
- `/script` completes filesystem paths; `/session` completes known sessions.
- Ctrl-U/Ctrl-D scroll by half a viewport; End resumes auto-follow.
- Esc closes completion or rejects the active approval.
- Ctrl-C clears input, cancels the active command, or exits when idle.
- Pairing deeplinks MUST NOT be retained in cross-process history.
- Only one operator command runs at once, but SSO traffic and approvals MUST
  continue while it runs.
- Concurrent approvals MUST be serialized.
- Suspending the TUI for `$VISUAL`, `$EDITOR`, or the platform editor MUST
  restore terminal state even when the editor fails.

The bare `/script` template MUST use only the public `truapi` and `host` script
API. Scratch files are durable under the active session and MUST be retained
after editor or script failure.

## 7. Product script contract

The runner MUST execute a JS or TS ES module with these globals:

```ts
declare const truapi: TrUApiClient;
declare const host: {
  productId: string;
  productAccount(index?: number): ProductAccountId;
};
```

`truapi` MUST be the public generated client connected through the product
frame endpoint and scoped to `--product-id`. `host.productAccount()` MUST use
the same product id and default to derivation index `0`.

The runner MUST:

- connect before importing the user module;
- execute top-level module code;
- await a default export when it is a function;
- close its provider on success and failure;
- preserve script stdout and stderr;
- return `0` on success and nonzero on a thrown/rejected error; and
- kill the child when its owning CLI operation is cancelled.

The public CLI artifact MUST contain a bundled runner and its TrUAPI client
dependencies. The default runner MUST NOT import files through
`CARGO_MANIFEST_DIR` or require the source repository to remain at its build
path. `TRUAPI_HOST_RUNNER` MAY remain as an explicit development override.
Using the packaged runner may require a documented Bun runtime, but all missing
runtime/asset errors MUST be detected before starting a live host flow.

## 8. Pairing and signing lifecycle

### 8.1 Pairing host states

```text
starting -> frames-ready -> disconnected
                         -> pairing -> authenticating -> connected
                                                   \-> failed -> disconnected
```

Every state transition MUST be observable. A new login MAY replace a failed or
disconnected attempt. Disposal MUST stop subscriptions and release transport
resources.

### 8.2 Signing host states

```text
starting -> session-loaded -> signer-ready -> allowance-ready
                                      \-> pairing-response-running -> completed
```

Reading or listing a session MUST stop at `session-loaded`; it MUST NOT trigger
identity registration or ring onboarding. A command that needs a signer MUST
resolve and activate it lazily.

Before answering a deeplink, the signing host MUST register Statement Store
allowance for both submitting accounts:

- the signing host's `//wallet//sso` account; and
- the pairing host's per-pairing device key.

Pairing MUST NOT continue if allowance registration fails. If an auto-managed
account has no free slot for the active period, the CLI MUST mark that account
exhausted for the period, choose or create another account, activate it, and
retry within a bounded attempt count. Explicit mnemonic and named-account
modes MUST surface exhaustion without silently changing identity.

Bulletin long-term storage allocation MUST use the real SSO/runtime path. It
MUST NOT be faked through CLI sentinel storage.

### 8.3 Deadlines and cancellation

All external waits MUST have named, testable deadlines and cancellation:

- frame listener startup;
- script-runner connection;
- identity-backend requests;
- identity record visibility;
- ring-membership visibility;
- RPC connection and request;
- subscription acknowledgement;
- extrinsic submit acknowledgement and finality;
- allowance visibility;
- pairing response; and
- child script completion when a script timeout is configured.

Cancellation MUST propagate through setup as well as steady-state operation.
Ctrl-C, `/quit`, process termination, session switching, or owner-task failure
MUST stop child processes, responders, subscriptions, frame connections, and
poll loops owned by that operation. Cancellation MUST NOT become effective
only after setup finishes.

Timeout errors MUST identify the operation, endpoint or network, and elapsed
deadline without printing secrets.

## 9. Sessions, accounts, and persistence

### 9.1 Base path

Default state root:

1. `$TRUAPI_HOST_BASE_PATH` or `--base-path`;
2. `$XDG_STATE_HOME/truapi-host`;
3. `$HOME/.local/state/truapi-host`; or
4. `.truapi-host` when neither platform location is available.

Relative explicit paths MUST be resolved to absolute paths once at startup.

### 9.2 Layout

The v0.1 layout is:

```text
<base-path>/
  accounts.json                         # default-session account pool
  accounts.json.lock
  <network>/
    pairing-host/
      product-storage.json
      core-storage.json
    signing-host/                       # default session runtime state
      current-session
      session.json
      product-storage.json
      core-storage.json
      scripts/
      sessions/
        <name>/
          accounts.json
          accounts.json.lock
          session.json
          product-storage.json
          core-storage.json
          scripts/
```

The `default` session preserves the root account pool and signing-host runtime
paths. A mnemonic-backed host is named `ephemeral`, MUST NOT allow session
switching, and MUST NOT write the mnemonic into a managed session.

### 9.3 Session names and switching

A persistent session name MUST:

- contain 1 to 64 ASCII characters;
- begin with a lowercase letter or digit;
- contain only lowercase letters, digits, `.`, `_`, or `-`; and
- not be `.` or `..`.

Switching MUST be transactional from the operator's perspective:

1. validate and prepare the target path;
2. load or build the target runtime without disturbing the active one;
3. stop the old pairing responder;
4. atomically replace the runtime;
5. disconnect product sockets bound to the old runtime so clients reconnect;
6. persist `current-session`; and
7. update TUI status and completion data.

A failed preparation MUST leave the prior session active.

Only one process MAY mutate a named session at a time. Completion work MUST add
a session-scoped process lock and a clear contention error. Read-only
diagnostics MAY operate without the mutation lock when safe.

### 9.4 Account resolution

Signer selection order is normative:

1. explicit mnemonic;
2. explicit named account;
3. an attested, non-exhausted account in the active session; or
4. a pending or newly generated auto-managed account.

Auto-managed accounts MUST be network- and session-scoped, registered through
the configured identity backend, and verified for People-chain ring membership
before use. Newly generated Lite username prefixes come from the lowercase
letters in the session name, fall back to `session`, and use `headless` for the
default session unless explicitly overridden.

### 9.5 Persistence safety

- Files containing mnemonics or runtime authentication state MUST be `0600` on
  Unix and use the closest platform equivalent elsewhere.
- Directories containing them SHOULD be user-only.
- Writes MUST use a temporary file, flush, atomic rename, and parent-directory
  sync where supported.
- Account-store mutation MUST hold an inter-process lock.
- Product/core storage mutation MUST not lose updates through concurrent
  writes.
- Corrupt state MUST produce a path-specific error; it MUST NOT be silently
  replaced with empty authentication state.
- Mnemonics, signing payloads, and private transport material MUST be redacted
  from normal logs and JSONL events. A full pairing deeplink may appear only in
  its dedicated lifecycle marker/event because the companion process needs it.

Plaintext mnemonics remain acceptable only because this is explicitly local
test state. The README and first account creation MUST warn about that boundary.

## 10. Network configuration

A network preset MUST define one coherent set of:

- stable network id;
- identity-backend base URL;
- People RPC URL and genesis hash;
- Bulletin RPC URL and genesis hash; and
- allowed live-chain genesis-to-RPC routes.

`paseo-next-v2` is the required v0.1 preset. Adding another preset MUST include
unit tests for literal genesis values, state namespace isolation, and endpoint
routing.

There MUST be no ordinary `--statement-store` URL override. This prevents a
mixed configuration where SSO, identity, allowance, and product chain calls
refer to different networks. Test-only endpoint injection belongs in internal
configuration or test fixtures.

Live product Chain routing SHOULD become an explicit discoverable CLI option.
Until then, `E2E_LIVE_CHAIN=1` is a test-runner switch. Disabled live routing
MUST return a clear unavailable/unsupported result; it MUST NOT silently route
an unknown genesis to the People endpoint.

## 11. Product-frame transport

The frame server MUST:

- bind before advertising readiness;
- default to loopback;
- create one `ProductRuntime` per WebSocket connection;
- associate every connection with the validated configured product id;
- carry exactly one SCALE `ProtocolMessage` in each binary WebSocket message;
- reject text frames rather than treating UTF-8 text as protocol bytes;
- dispose the product runtime when the socket closes;
- disconnect old runtimes during session replacement;
- bound outbound buffering or apply backpressure; and
- stop accepting connections when the owner is cancelled.

Binding a non-loopback address exposes an unauthenticated host capability and
MUST require an explicit unsafe opt-in plus a prominent warning. This behavior
is a completion item before non-loopback listening is documented.

## 12. Capability behavior

The diagnosis reports demonstrate protocol reachability, but a green row may
mean a well-typed unavailable result rather than a native implementation. The
v0.1 behavior contract is:

| Capability | Headless-host behavior |
| --- | --- |
| Account/session | Real core state, SSO login, product accounts, username resolution, aliases, and proofs. |
| Signing | Real host-owned signing and transaction assembly, including supported legacy calls. |
| Statement Store | Real selected-network subscribe, proof, authorized proof, submit, and allowance. |
| Preimage/Bulletin | Real core construction, Bulletin submit/lookup, and SSO resource allocation. |
| Chain | Real preset RPC routing only when live routing is enabled; otherwise explicit unavailable behavior. |
| Entropy | Real signing-host derivation policy. |
| Product/core storage | Persistent, session-scoped local state. |
| Permissions/confirmation | Interactive deny-by-default prompt or reported `--auto-accept`. |
| Theme | A stable CLI theme value; dark is the v0.1 default. |
| Navigation | Report the requested URL; do not launch it implicitly. |
| Notifications | Return a typed unavailable result; do not pretend delivery occurred. |
| Feature support | Report actual CLI capabilities rather than blanket success. |
| Chat, Coin Payment, Payment | Typed unsupported/unavailable until a real backend exists. |

The checked-in pairing and signing diagnosis reports MUST remain free of
unexpected failures. Skipped methods MUST correspond to deliberate capability
gaps, not crashes, hangs, undecodable frames, or missing host wiring.

## 13. Output, events, and logging

### 13.1 Human mode

Human mode is the default. Outside the TUI:

- command results and sentence-case lifecycle events go to stdout;
- diagnostics, warnings, approvals, and tracing go to stderr;
- sensitive values are redacted unless the value is the explicit operator
  input or capability required for the workflow; and
- `exec` emits no ANSI control sequences.

Streaming and TUI modes MUST share the same human wording and status symbols.
For example:

```text
• Listening for product frames
  <ws-url>
• Pairing link
  <url>
◌ Authenticating pairing
✓ Paired with <user>
✓ Signing host ready
✓ Script finished
```

Events MUST be emitted once per corresponding transition and MUST not be
assembled by scraping tracing prose. Automation MAY extract the explicit
`polkadotapp://pair?...` URL, but MUST NOT depend on a second machine-only copy
of the lifecycle text.

Inside the interactive TUI, those events use the same copy with richer layout.
Machine markers and repeated source labels such as `HOST ·` and `SCRIPT ·`
MUST NOT appear. Submitted commands MUST be visually grouped with
their output, script stdout MUST use the terminal's default foreground, and
pending pairing/onboarding work MUST update a keyed activity rather than append
one transcript row per poll. The command prompt MUST use the native terminal
cursor after the insertion point, including for wide Unicode and horizontally
scrolled input. Color MUST be supplemental, honor `NO_COLOR`, and remain
understandable through symbols and wording alone.

### 13.2 JSONL mode

Before v0.1 is treated as a stable CI tool, `--output jsonl` SHOULD emit one
versioned object per lifecycle event:

```json
{"version":1,"event":"frames_listening","role":"pairing-host","url":"ws://127.0.0.1:9955"}
```

Every object MUST contain `version` and `event`; applicable objects SHOULD
contain `role`, `network`, `session`, `operation_id`, and `elapsed_ms`. Script
stdout/stderr MUST use explicit `script_stdout` and `script_stderr` events or a
documented passthrough mode. Mnemonics, signing payloads, and private transport
material MUST never appear in JSONL. The full deeplink is allowed only in the
dedicated `pairing_deeplink` event and consumers MUST treat captured output as
sensitive.

### 13.3 SSO transcript

Decoded inbound request names and response outcomes MUST remain visible at
normal verbosity. Stable response entries SHOULD include request name,
statement id, remote message id, outcome, and elapsed time. Encoded protocol
errors SHOULD include their reason. Full payload and transport metadata belong
only at `trace` and still require secret redaction.

## 14. Approvals and security

Without `--auto-accept`, sensitive actions MUST require an explicit affirmative
`y` or `yes`; EOF, invalid input, non-TTY input, Esc, and cancellation reject.
The prompt MUST identify the action and enough review detail to make the
decision meaningful.

`--auto-accept` MUST:

- be opt-in on every invocation;
- approve only actions that already pass core permission and scope checks;
- print or emit one decision record per approval; and
- never weaken product-id, session, or cross-product authorization.

Same-product Ring-VRF behavior MUST match the signing runtime policy.
Cross-product requests MUST retain the appropriate confirmation path.

The CLI MUST NOT print mnemonics. Tracing and general transcript entries MUST
NOT print complete pairing deeplinks or raw signing payloads. The one dedicated
pairing-deeplink marker/event is required for handoff and MUST be documented as
sensitive output. Help and docs MUST label explicit mnemonic use and
auto-managed account files as local test functionality.

## 15. Exit codes and shutdown

| Code | Meaning |
| --- | --- |
| `0` | Command or script completed successfully. |
| `1` | Runtime, network, state, approval, or script failure without a more specific child status. |
| `2` | Invalid invocation, missing required runtime/asset, or usage error. |
| child code | One-shot `--script` SHOULD preserve a normal child exit code when representable. |
| `128 + signal` | MAY be used for conventional signal termination on Unix. |

Graceful shutdown MUST stop accepting product connections, cancel owned
operations, terminate child scripts, stop pairing responders, dispose product
runtimes, restore the terminal, flush durable state, and then exit. Shutdown
MUST itself have a short deadline; stuck cleanup may be forcefully aborted
after reporting what did not stop.

`std::process::exit` SHOULD NOT bypass required cleanup or terminal restoration.

## 16. Installation and packaging

The supported developer flow MUST be real and tested:

```sh
make headless
make install-headless
truapi-host --help
```

These target names are normative. README, help, CI, and this document MUST
agree. The existing prose `make headless install` is not an acceptable contract
because it names two unrelated Make targets and neither is defined in the
current worktree.

An installed CLI MUST work after moving or deleting the source checkout. The
packaging test MUST run a product script from a temporary directory using only:

- the installed `truapi-host` binary;
- documented runtime dependencies such as Bun; and
- the user-provided script.

It MUST NOT resolve `js/runner.ts` or `@parity/truapi` through repository-
relative imports. Release artifacts SHOULD include checksums and the version
reported through `--version`.

## 17. Test and acceptance specification

### 17.1 Required deterministic checks

Every implementation slice MUST pass the narrow tests it changes. The final
branch MUST pass:

```sh
cargo +nightly fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build -p truapi-host-cli --release
git diff --check
```

No deterministic test may depend on a public RPC, identity backend, real clock
period boundary, phone, signing bot, or pre-existing user state.

### 17.2 Unit tests

Unit coverage MUST include:

- clap conflicts and defaults;
- slash-command and approval parsing;
- session-name validation and username-prefix derivation;
- session layout, current selection, identity cache, and failed-switch rollback;
- account selection, locking, atomic persistence, exhaustion, and permissions;
- network preset genesis and routing;
- frame transport binary-only behavior, disconnect, and reset;
- output marker/JSONL serialization and redaction;
- timeout and cancellation propagation; and
- editor/runner path handling without shell injection.

### 17.3 Process-boundary tests

Process tests MUST cover:

- non-TTY interactive rejection with exit `2`;
- plain `exec /help`, `/session`, `/session --list`, and invalid commands;
- no ANSI in `exec` stdout or stderr;
- session restoration and isolated state;
- two-process session lock contention;
- child script success, thrown error, timeout, and Ctrl-C cleanup;
- missing Bun/runner failure before network onboarding;
- graceful SIGINT/SIGTERM behavior on supported platforms; and
- installation from a temporary prefix followed by source-checkout removal.

### 17.4 Through-core integration tests

Tests MUST send real encoded product frames through `ProductRuntime`; mocks may
replace only network/platform boundaries. Required cases:

- product storage round trip and session isolation;
- permission accept/reject;
- direct signing-host product account flow;
- pairing state transitions and responder failure;
- runtime replacement disconnects stale product sockets;
- SSO request and response transcript outcomes; and
- cancellation during connection, subscription setup, and submit watch.

### 17.5 Live acceptance tests

Live tests are opt-in and MUST clearly identify the selected preset. They MUST
cover:

1. `battery.ts` in paired mode, covering every generated playground example
   and writing `explorer/diagnosis-reports/cli.md`;
2. focused direct signing-host product-account flows;
3. focused signing, ring-VRF, and preimage scripts;
4. diagnosis for both headless roles;
5. account onboarding and cached restart;
6. Statement Store slot exhaustion and auto-account rotation; and
7. one cancellation/restart recovery scenario.

The target diagnosis baseline is the checked-in intentional capability set:
`44 passed, 0 failed, 20 skipped` for both headless reports. A changed method
count MUST be reviewed against the canonical generated API rather than updating
the expected number blindly.

Live failures MUST preserve logs and reports with secrets redacted. CI MAY mark
a known external network outage separately from a product failure, but it MUST
not convert a protocol assertion failure into a skip.

## 18. Definition of done for v0.1

The CLI is complete when all of the following are true:

- [ ] CLI help, crate README, root README, Make targets, and this spec describe
      the same commands and slash-command surface.
- [ ] Product ids and incompatible flags fail before starting external work.
- [ ] `pairing-host`, `signing-host`, `exec`, and script modes satisfy their
      lifecycle and exit-code contracts.
- [ ] The signing TUI restores the terminal under success, failure,
      cancellation, editor use, and signals.
- [ ] Persistent sessions are isolated, locked, atomically switched, and safe
      from concurrent mutation.
- [ ] Sensitive state uses atomic, restrictive persistence and logs are
      redacted.
- [ ] Every external wait is bounded and cancellable during setup.
- [ ] Product-frame queues are bounded and text frames are rejected.
- [ ] The CLI installs with a bundled runner and operates without the source
      checkout.
- [ ] Deterministic tests and process-boundary tests pass in CI.
- [ ] Paired and direct live acceptance runs have no unexpected diagnosis
      failures.
- [ ] Unsupported capability results are deliberate, typed, and documented.
- [ ] No CLI-only workaround duplicates logic that belongs in `truapi-server`.

## 19. Suggested agent work packages

These packages are designed to minimize overlapping edits. Agents MUST rebase
or coordinate before touching a file owned by another active package.

### A. Distribution and install contract

Primary files: `Makefile`, `Cargo.toml`, `rust/crates/truapi-host-cli/js/`,
`script_runner.rs`, crate/root READMEs.

Deliverables:

- bundle the runner and generated client dependencies;
- add consistent build/install targets and `--version`;
- add the relocation acceptance test; and
- remove source-checkout assumptions from normal execution.

### B. Lifecycle, deadlines, and cancellation

Primary files: `main.rs`, `attestation.rs`, `accounts.rs`, `chain.rs`, and
shared runtime APIs only where cancellation must cross into `truapi-server`.

Deliverables:

- named deadline policy;
- cancellation tokens/ownership for startup and subscriptions;
- signal-aware graceful shutdown;
- child/responder cleanup; and
- deterministic timeout/cancellation tests.

### C. State safety and session locking

Primary files: `sessions.rs`, `accounts.rs`, `platform.rs`.

Deliverables:

- session process lock;
- atomic and restrictive product/core/session persistence;
- corruption errors instead of silent auth reset;
- failed-switch rollback tests; and
- concurrent writer tests.

### D. Product-frame transport hardening

Primary files: `frame_server.rs`, `chain.rs`, transport-focused tests.

Deliverables:

- binary-only frame enforcement;
- bounded queues/backpressure;
- cancellation-aware accept/connect loops;
- safe non-loopback policy; and
- disconnect/reset/lag tests.

### E. Automation output and TUI conformance

Primary files: `terminal_ui.rs`, `signing_shell.rs`, output helpers, process
tests.

Deliverables:

- centralized lifecycle events;
- optional versioned JSONL renderer;
- stable human markers and stream separation;
- redaction tests;
- cancellation and approval interaction tests; and
- removal of stale `/whoami` documentation.

### F. Deterministic and live E2E harness

Primary files: `e2e/`, `tests/`, `.github/workflows/`, diagnosis scripts and
reports.

Deliverables:

- replace unbounded polling and process-kill assumptions in `run.sh`;
- preserve artifacts and classify external outages;
- add direct and paired gates;
- add installation/relocation coverage; and
- document exact local and CI commands.

### G. Capability audit

Primary files: `platform.rs`, network/runtime integration, diagnosis scripts.

Deliverables:

- make unsupported capabilities return deliberate typed results;
- replace blanket feature reporting with actual support;
- expose live chain routing clearly and reject unknown genesis hashes; and
- keep paired/signing diagnosis reports aligned with the generated API.

Package A should land before the final E2E packaging tests in F. Packages B, C,
D, E, and G can proceed independently when they avoid shared `main.rs` edits;
integration should centralize orchestration changes once their local APIs are
settled.
