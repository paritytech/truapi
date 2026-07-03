# truapi-server

_Runtime core for TrUAPI: dispatcher, protocol frames, SCALE-coded wire envelope._

## What this crate is for

`truapi-server` is the runtime that turns trait implementations of the
`truapi` API into a working host. It owns:

- the [`ProtocolMessage`] wire envelope and SCALE codec
- the [`Dispatcher`] that routes incoming frames to per-method handlers
- the subscription lifecycle (start/receive/stop/interrupt)
- the [`Transport`] trait that platform-specific IPC backends implement
- the auto-generated dispatcher/wire-table tables shipped under
  [`crate::generated`]
- the host embedding surface: one long-lived role handle
  (`PairingHostRuntime` or `SigningHostRuntime`) per host application, exposing
  shared [`RuntimeServices`] plus one [`ProductRuntime`] per product connection

## Architecture

Two ownership bands. A **per-product-connection** band (byte frames →
dispatcher → role-neutral product runtime) is minted once per host↔product
connection by the role handle and lives for that product's whole session; a
**shared per-host** band owns role-neutral infrastructure (`RuntimeServices`)
and the role object (`PairingHost` or `SigningHost`), which is itself the
`ProductAuthority`. Pure `host_logic` is a no-I/O library both bands call, not a
stage in the frame path; the host's `Platform` impl is the syscall floor.

```text
   ┌───────────────────────────────────────────────────────┐
   │ product      sandboxed iframe · native WebView        │
   └───────────────────────────────────────────────────────┘
                              │  ▲
          SCALE frames        │  │  MessageChannel · loopback
          both directions     ▼  │  WS
   ┌───────────────────────────────────────────────────────┐
   │ binding layer :  wasm.rs  ( browser / node )          │
   │ thin byte bridge  ·  no protocol logic                │
   └───────────────────────────────────────────────────────┘

 ══ per host→product connection ( one per connected product ) ══
   ┌───────────────────────────────────────────────────────┐
   │ ProductRuntime           frame endpoint               │
   │ decode each SCALE frame → dispatch one typed call     │
   └───────────────────────────────────────────────────────┘
                              │  typed method call
                              ▼
   ┌───────────────────────────────────────────────────────┐
   │ ProductRuntimeHost       role-neutral                 │
   │ validate · permission-gate · confirm                  │
   └───────────────────────────────────────────────────────┘
                              │  wallet-authority tail :
                              │  sign · alias · entropy · alloc
                              │  via  Arc<dyn ProductAuthority>
                              ▼

 ══ shared per host app ( one per host, all connections ) ══════
     the PairingHostRuntime | SigningHostRuntime handle owns both:
   ┌─────────────────────────────┐   ┌────────────────────────┐
   │ role  =  ProductAuthority   │   │ RuntimeServices        │
   │ PairingHost | SigningHost   │   │ platform · chain · RPC │
   └─────────────────────────────┘   └────────────────────────┘
              │
              │  PairingHost only : encrypted SSO channel
              ▼
       ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
       ╎ remote signing host   ( external wallet ) ╎
       └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘

   both bands call host_logic for pure work, never traverse it :
   ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
   ╎ host_logic       pure library ( no I/O )              ╎
   ╎ crypto · codecs · derivation · policy                 ╎
   └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘

 ══ host-owned floor · where every I/O above bottoms out ═══════
   ┌───────────────────────────────────────────────────────┐
   │ Platform impl  ( TS / Swift / Kotlin )                │
   │ storage · prompts · chain RPC · navigation            │
   └───────────────────────────────────────────────────────┘
```

`ProductRuntimeHost` handles everything role-neutral (id normalization,
permission gating, confirmation, soft product-key derivation), then delegates
the wallet-authority tail (`sign_*`, `create_transaction`, `account_alias`,
`allocate_resources`, `derive_entropy`) through an `Arc<dyn ProductAuthority>`
handle with an `AuthoritySession` snapshot the role revalidates before touching
key material.

The embedder builds a role handle, `PairingHostRuntime::new(...)` or
`SigningHostRuntime::new(...)`, then calls `product_runtime(product, sink)` for
each product connection. Role-specific operations live only on the matching handle:
`cancel_pairing` and `notify_session_store_changed` on the pairing handle,
`activate_local_session` on the signing handle. Calling the wrong operation is
a compile error, not a runtime `Unavailable`.

### The two roles

Both implement the role-neutral **`ProductAuthority`** trait; each owns its
role-specific lifecycle, so no method exists on a role that can't mean it:

- **`PairingHost`** (seedless): the user's keys live in an external wallet, so
  signing/aliases/entropy relay over an encrypted SSO channel (statement store
  on the People chain; the channel lives in `pairing_host/sso_channel.rs`). It
  owns pairing/login state, persisted auth-session reload, and remote
  signing-host liveness monitoring.
- **`SigningHost`** (wallet-local): signs on device from local BIP-39 entropy,
  no pairing flow. `signing_host/local_activation.rs` establishes a session
  from host-held secret material. Extrinsic signing / transaction construction /
  ring-VRF aliases / resource allocation currently return `Unavailable` pending
  chain-metadata and on-chain support.

`host_logic` stays pure: the orchestrators above call into it for codecs,
session/SSO crypto, key derivation, and permission policy, while all I/O
(statement-store RPC, storage, prompts, chain RPC) stays in the layers above.

## Wire envelope

Every frame on the wire is encoded as:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant identifies a method + frame kind via the auto-generated
[`crate::generated::wire_table::WIRE_TABLE`]. Each method's ids are exposed
as a named const (`PREIMAGE_SUBMIT`, ...); both `WIRE_TABLE` and the generated
dispatcher reference those consts. Method ordering is part of the wire
protocol; only ever append.

The payload bytes are the SCALE-encoded inner value, inlined without a
length prefix. The discriminant is carried directly as `Payload::id`, and the
dispatcher routes on that numeric id via id-keyed tables.
