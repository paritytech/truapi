# RFC-00XX: Contacts \- Host-Mediated Invitations and Product-Separated Key Agreement

| RFC Number | 00XX |
| :---- | :---- |
| **Start Date** | 2026-07-06 |
| **Description** | Reachable-by-default, end-to-end-encrypted contact requests between users, plus product-separated symmetric key agreement on host-managed exchange keys |
| **Authors** |  |
| **Status** | Draft |

## Summary

This RFC adds a `Contacts` service to TrUAPI with four methods: `resolve`, `derive_shared_key`, `send`, and `subscribe`. Together they let a product deliver an end-to-end-encrypted payload (an invitation, a share, a contact request) to any user identified by their DotNS username or identity key \- **including users who have never opened the product**. The host derives a per-identity X25519 *exchange key* from the root account entropy, publishes its public half on the Statement Store once per identity, watches the identity's contact inbox on the user's behalf, and delivers decrypted payloads to the addressed product when it runs. Product isolation comes from domain-separating the KDF from the product identifier rather than from per-product keypairs, so one published key makes the user reachable across every product. The user agent \- not the product \- decides how incoming contact attempts are surfaced.

## Motivation

Products need to deliver secret-bearing or trust-establishing payloads to users identified by their identity, end-to-end encrypted. link3 (shared-profiles) is the concrete case; chat contact requests, HackM3, and Mark3t have the same shape.

Because the host exposes only signing over identity keys, link3 must build this itself: derive a per-recipient keypair via `host_derive_entropy` and ship its secret (`skR`) to the recipient; have every user mint an "inbox" keypair and publish it on the Statement Store before anyone can reach them; and seal invitations to that published key. The publication happens inside the product, so it requires the recipient to have opened the product, granted `StatementSubmit`, and obtained a `StatementStoreAllowance` \- in practice "A deploys, B deploys, A deploys again" before an invitation lands. Product-side fixes softened this, but the hard floor remains: **you cannot invite someone into a product they have never opened.** Users should instead learn about a product *because* a friend contacted them about it. The approach also costs each product several hundred lines of generic, delicate crypto and statement-store plumbing.

Requirements, distilled from the design discussion between the link3 and TrUAPI teams:

1. **E2E encryption on host-managed exchange keys, domain-separated per product.** Products get authenticated encryption to a peer without holding the exchange secret; a ciphertext or derived key for product P is useless to product Q.  
2. **A generalized incoming-contact mechanism in the host.** Chat requests and product invitations are the same thing: an attempt by one user to reach another through a product. The host receives them for all products and hands each payload to its product when the product runs.  
3. **Reachable by default.** Reachability must be a property of *having an identity*, not of having opened a product. The user agent decides how to *report* a contact attempt, but contact must not be impossible by default.  
4. **A simple product-facing API.** The common case \- "invite this username, deliver this payload" \- must be one call on each side.

## Detailed Design

### Overview

```
 One-time, per identity (no product involved):
   Host(Bob)   ── publishes ──▶  Statement Store: ExchangeKeyRecord @ exchangeKeyTopic(bob)

 Alice invites Bob to product P (Bob has never opened P):
   P@Alice     ── send(bob, payload) ──▶ Host(Alice)
   Host(Alice) ── resolves bob's exchange key, seals envelope ──▶ Statement Store @ contactInboxTopic(bob)
   Host(Bob)   ── watching bob's inbox ── decrypts, verifies, persists, applies UA policy
               ── (e.g.) notifies: "alice invited you on P"
   Bob opens P for the first time:
   P@Bob       ── subscribe() ──▶ Host(Bob) ──▶ { sender: alice, payload }
```

Three ideas carry the design:

1. **One exchange key per identity, published by the host.** Every identity is reachable for every product, past and future, with zero product involvement \- this removes the bootstrap.  
2. **Product separation in the KDF, not in the keys.** All products seal to the same exchange key, but every derived symmetric key mixes the product identifier into the KDF.  
3. **The host is the recipient.** The host holds the exchange secret and watches the inbox continuously, so it can receive and triage contacts for products that are not running \- or never ran. Products receive plaintext payloads; they perform no envelope cryptography.

### The exchange key

*Identity* means the account owning the user's primary DotNS username (RFC-0015); its 32-byte public key is denoted `identity`. The host derives the exchange keypair from `rootEntropySource` \- the root material RFC-0007 shares with every conforming host at SSO \- under a dedicated domain separator:

```rust
let exchangeSeed: [u8; 32] = blake2b256_keyed(
    message: rootEntropySource,                       // the SSO-shared root, as in RFC-0007
    key: b"truapi-exchange-key-v1",
);
let exchangeSecret = x25519_clamp(exchangeSeed);
let exchangePublic = x25519_basepoint_mul(exchangeSecret);
```

The derivation is deterministic, so the key is identical on every device and every conforming host \- no key storage, no sync. No product can compute it: a product only ever sees `blake2b256_keyed(perProductEntropy, key)`, and the 22-byte exchange separator can never equal a product's 32-byte `blake2b256(productId)` key, so no product's derivation tree contains it.

#### Publication

| Field | Value |
| :---- | :---- |
| `topics` | `[ blake2b256(b"truapi:exchange-key:v1:" ++ identity) ]` |
| `channel` | `blake2b256(b"truapi:exchange-key-channel:v1")` |
| `data` | SCALE-encoded `ExchangeKeyRecord` |
| `proof` | `Sr25519`, signed by the **identity account itself** |

```rust
struct ExchangeKeyRecord {
    version: u8,               // = 1
    exchange_public: [u8; 32], // X25519 public key
    issued_at: u64,            // unix seconds
}
```

The statement proof is the authenticity binding: verifiers MUST check that the proof is valid and that `proof.signer` equals the `identity` the topic derives from, and MUST ignore records failing either check. (App-level schemes publish inbox keys with no such binding and are open to key substitution by anyone with store quota.)

Hosts MUST publish the record when an identity becomes available to them (login/session establishment) unless the store already holds the current record on the LWW channel; the check-then-publish is idempotent. The record's identity signature is produced once per identity \- approved at first login and cached, since the record is deterministic \- so publication is a single one-time approval per identity, never a per-product prompt, and never contingent on any product being open. A user agent MAY offer an unreachability opt-out (publishing nothing); unreachable is never the default.

#### Relationship to the chat `identifier_key`

The chat stack already publishes a per-user P-256 encryption key on the People chain (`Resources.Consumers.identifier_key`), and existing product designs wrap per-recipient keys against it. The exchange key is the protocol-level successor of that pattern for cross-product use: host-held rather than product-computed, X25519 (matching the envelope construction), and device-portable without key sync. The two coexist during migration \- hosts that implement chat keep publishing `identifier_key`; new invitation and sharing flows SHOULD address peers through this RFC's surface, and a later chat migration onto this rail (see Future Directions) would allow `identifier_key` to be retired.

### The contact envelope

`send` seals the payload into a `ContactEnvelope` addressed to the recipient's published exchange key. All hashes are BLAKE2b-256, keyed/unkeyed exactly as in RFC-0007:

```rust
struct ContactEnvelope {
    version: u8,               // = 1
    ephemeral_public: [u8; 32],
    ciphertext: Vec<u8>,       // ChaCha20-Poly1305
}
```

```rust
let (ephemeral_secret, ephemeral_public) = x25519_generate();     // fresh per envelope
let ss = x25519(ephemeral_secret, recipient_exchange_public);
let k_env = blake2b256_keyed(
    message: ss ++ ephemeral_public ++ recipient_exchange_public,
    key: b"truapi-contact-envelope-v1",
);
ciphertext = chacha20poly1305_encrypt(
    key: k_env,
    nonce: [0u8; 12],                    // fresh key per envelope; nonce reuse impossible
    aad: version_byte ++ ephemeral_public,
    plaintext: scale_encode(ContactMessage { ... }),
);
```

```rust
struct ContactMessage {
    product_id: String,        // sending product's normalized DotNS identifier
    sender_identity: [u8; 32],
    sent_at: u64,              // unix seconds
    payload: Vec<u8>,          // opaque product bytes
    signature: [u8; 64],       // sr25519 by the sender identity account
}
```

The signature covers, in order:

```
blake2b256(
    b"truapi-contact-sig-v1"
    ++ recipient_identity
    ++ blake2b256(product_id)
    ++ ephemeral_public
    ++ blake2b256(payload)
    ++ scale_encode(sent_at)
)
```

Binding the recipient prevents re-sealing a captured message to another recipient; binding `ephemeral_public` ties the signature to this envelope; binding `product_id` stops cross-product impersonation. The receiving host derives the exchange secret, MUST check the decrypted `recipient_identity` is its own, and MUST verify the signature against `sender_identity`, discarding anything that fails to decrypt or verify.

Receipts, accept/decline, and request/response flows are ordinary contacts addressed back to the sender's identity (delivered to the recipient's product as `sender_identity`/`sender_username`); the host emits no automatic acknowledgment (see Privacy).

#### The statement carrying an envelope

| Field | Value |
| :---- | :---- |
| `topics` | `[ blake2b256(b"truapi:contact-inbox:v1:" ++ recipient_identity) ]` |
| `channel` | `blake2b256(b"truapi:contact-channel:v1:" ++ blake2b256(product_id) ++ recipient_identity)` |
| `data` | SCALE-encoded `ContactEnvelope` |
| `proof` | any account controlled by the sending host with statement admission |

Channels are last-write-wins per signing account, so re-sending to the same recipient for the same product *replaces* the previous contact \- invitation refresh for free, bounded sender footprint. At most one contact per `(sender, recipient, product)` is outstanding in the store at a time.

The proof exists solely for store admission; verifiers MUST NOT infer the sender from it \- the authenticated sender is `ContactMessage.sender_identity`. Hosts SHOULD sign contact submissions with a dedicated product-scoped account covered by the product's `StatementStoreAllowance` (RFC-0010), keeping the identity and primary product accounts out of observable metadata. Because replacement correlates by `(channel, submitting account)`, that account MUST be stable per `(recipient, product)`; scoping it per recipient keeps it stable without letting a store observer link a sender's contacts across recipients. Hosts SHOULD set expiry using the established ceiling-expiry practice (footprint is bounded by the LWW channel; a genuine TTL loses admission races when the account's slots are full).

### Protocol surface

A new `Contacts` service trait is added to the `TrUApi` super-trait, following the standard pattern: `v01` wire types, single-`V1` `versioned_type!` envelopes, fresh append-only wire ids (the highest allocated today is 163):

```rust
/// Contact requests and invitations between users, mediated by the host (RFC-00XX).
pub trait Contacts: Send + Sync {
    /// Resolve a peer to their identity key and published exchange key.
    #[wire(request_id = 164)]
    async fn resolve(&self, _cx: &CallContext, _request: HostContactsResolveRequest)
        -> Result<HostContactsResolveResponse, CallError<HostContactsResolveError>>;

    /// Derive a symmetric key shared with a peer, scoped to the calling
    /// product and a caller-chosen context.
    #[wire(request_id = 166)]
    async fn derive_shared_key(&self, _cx: &CallContext, _request: HostContactsDeriveSharedKeyRequest)
        -> Result<HostContactsDeriveSharedKeyResponse, CallError<HostContactsDeriveSharedKeyError>>;

    /// Seal a payload to a recipient and submit it to their contact inbox.
    #[wire(request_id = 168)]
    async fn send(&self, _cx: &CallContext, _request: HostContactsSendRequest)
        -> Result<(), CallError<HostContactsSendError>>;

    /// Receive contact payloads addressed to the calling product.
    #[wire(start_id = 170)]
    async fn subscribe(&self, _cx: &CallContext)
        -> Result<Subscription<HostContactsSubscribeItem>, CallError<HostContactsSubscribeError>>;
}
```

```rust
/// A peer addressed by DotNS username or identity public key.
pub enum ContactPeer {
    Username(String),
    Identity([u8; 32]),
}

pub struct HostContactsResolveRequest { pub peer: ContactPeer }
pub struct HostContactsResolveResponse {
    pub identity: [u8; 32],
    /// `None`: the identity exists but has no published exchange key
    /// (e.g. its host predates this RFC); the peer is not yet reachable.
    pub exchange_key: Option<[u8; 32]>,
}
pub enum HostContactsResolveError {
    NotFound,
    Unknown { reason: String },
}

pub struct HostContactsDeriveSharedKeyRequest {
    pub peer: ContactPeer,
    /// Domain-separation context, at most 32 bytes (as in RFC-0007,
    /// callers hash longer contexts down).
    pub context: Vec<u8>,
}
pub struct HostContactsDeriveSharedKeyResponse {
    pub key: [u8; 32],
}
pub enum HostContactsDeriveSharedKeyError {
    NotFound,
    NotReachable,
    ContextTooLong,
    NotConnected,
    Unknown { reason: String },
}

pub struct HostContactsSendRequest {
    pub recipient: ContactPeer,
    pub payload: Vec<u8>,
}
pub enum HostContactsSendError {
    NotFound,
    NotReachable,
    PayloadTooLarge,
    PermissionDenied,
    NotConnected,
    Unknown { reason: String },
}

pub struct HostContactsSubscribeItem {
    pub contacts: Vec<ContactDelivery>,
    /// `false` while replaying contacts received before this subscription,
    /// `true` once caught up and live (as in RFC-0008).
    pub is_complete: bool,
}
pub struct ContactDelivery {
    pub sender_identity: [u8; 32],
    pub sender_username: Option<String>,
    pub payload: Vec<u8>,
    pub sent_at: u64,
}
pub enum HostContactsSubscribeError {
    NotConnected,
    Unknown { reason: String },
}
```

(`response_id`s follow the `request_id + 1` convention; the subscription occupies 170–173. Ids to be confirmed against the wire table at merge time.)

#### `resolve`

Resolves a username to its identity key (People-chain username owner) and looks up the published record (verifying the proof binding). No prompt: everything returned is public.

#### `derive_shared_key`

Returns a 32-byte symmetric key shared between the calling user and the peer, unique to the (user pair, product, context) tuple. Derivation mirrors the three-layer structure of RFC-0007:

```rust
let ss = x25519(own_exchange_secret, peer_exchange_public);
// lo/hi: the two exchange public keys in byte-wise lexicographic order,
// so both sides compute identical input
let pairSecret: [u8; 32] = blake2b256_keyed(
    message: ss ++ lo ++ hi,
    key: b"truapi-contact-shared-v1",
);
let perProductSecret: [u8; 32] = blake2b256_keyed(
    message: pairSecret,
    key: blake2b256(productId),          // normalized as in RFC-0007
);
let sharedKey: [u8; 32] = blake2b256_keyed(
    message: perProductSecret,
    key: context,
);
```

Core invariant: for users A and B and product P, `derive_shared_key` called by A with peer B and by B with peer A, with the same `context`, MUST return the same key on every conforming host (X25519 is symmetric in the pair; the remaining inputs are order-normalized). Different products obtain unrelated keys; `pairSecret` and `perProductSecret` never leave the host.

This primitive replaces per-recipient `skR` shipping entirely: both sides *derive* the pairwise key, so invitations carry no key material (see the worked example). It is also the intended root secret for richer product protocols (sessions, ratchets). A peer with no published exchange key yields `NotReachable`; a `context` longer than 32 bytes is rejected with `ContextTooLong`. No prompt: the result is product-scoped key material, equivalent in sensitivity to `host_derive_entropy` output.

#### `send`

Resolves the recipient, builds and signs the `ContactMessage`, seals the envelope, and submits the statement \- one call. Failure modes: `NotFound` (no such identity), `NotReachable` (no exchange key published), `PayloadTooLarge` (statement would exceed the store's size limit; payloads SHOULD stay small and carry references \- CIDs, names \- rather than bulk data).

A send to the sender's own identity MUST be delivered locally without a store round-trip. Because channels key on identity, a product that will later re-send SHOULD pin the identity `resolve` returned rather than re-passing a `Username`, which can rebind to a new owner.

`send` is gated by a new remote permission, `RemotePermission::ContactSend` (variant appended \- wire-compatible), requested implicitly on first use per the RFC-0002 lifecycle: one prompt, decision persisted. Admission economics (allowances, quotas) are handled by the host internally.

#### `subscribe`

Delivers `ContactDelivery` items whose envelope named the calling product (`ContactMessage.product_id` equals the caller's normalized product identifier \- the host enforces this; a product can never observe another product's contacts). The host first replays persisted contacts in pages with `is_complete = false`, then a page with `is_complete = true`, then live deliveries \- the RFC-0008 pattern. A later delivery from the same `sender_identity` supersedes the earlier one (invitation refresh). Receiving requires no prompt: the UA policy layer has already decided what reaches the product.

### Worked example: link3

Today, after link3's own best-effort fixes:

```
Alice: open link3, deploy, whitelist bob      → invite queued "pending" (bob unreachable)
Bob:   open link3 (must know about it!), grant allowance + StatementSubmit,
       click "Let others invite you"          → inbox key published
Alice: reopen link3                            → pending invite retried, sealed, submitted
Bob:   reopen link3                            → invite received, skR stored, links decrypt
```

With this RFC (Bob has never opened link3):

```
Alice (in link3):
    contentKey = derive_shared_key(bob, blake2b256("link3:content:v1"))
    // encrypt bob's private-links slot under contentKey
    send(bob, { profileName, profileCid })

Bob's host: receives, verifies, notifies "alice invited you on link3"
Bob (opens link3 for the first time):
    subscribe() → { sender: alice, payload: {...} }
    contentKey = derive_shared_key(alice, "link3:content:v1")
    // fetch envelope by profileCid, decrypt private links
    // optionally: send(alice, accepted)   // reaches alice, reachable by default
```

No inbox keypair, no key publication, no pending-invite queue, no sealed-box code \- and because both sides derive `contentKey`, no key material in the invitation at all: an intercepted invitation leaks a CID, not a decryption key. Multi-recipient products wrap their content key under each pairwise `sharedKey` (or use it as the per-recipient slot-key seed) \- the same structure as today, minus the shipping.

### Codegen impact

A standard additive change: `v01/contacts.rs` types, `versioned/contacts.rs` single-`V1` envelopes, `api/contacts.rs` trait, `Contacts` in the `TrUApi` super-trait, then `./scripts/codegen.sh` regenerates the TS client, Rust dispatcher/wire table, Dart host interfaces, and playground/explorer metadata.

## Relationship to RFC-0014 (Contacts API)

RFC-0014 exposes the host-managed address book to products: *who the user knows*, as local display names plus per-context alias/account entries, gated by `DevicePermission::Contacts` and `ContactsCrossContext`. This RFC is the other half \- *how a user reaches a peer* and establishes keys. The two remain separate services and compose:

- **Permissions.** Reading the address book is a device permission (RFC-0014); sending on the user's behalf is a remote permission (`ContactSend`). Neither implies the other.  
- **Ingestion.** RFC-0014 leaves open how contacts enter the address book. Accepted contact requests are the natural path: when the user acts on a delivered contact, the host SHOULD offer to add the sender to the address book.  
- **Privacy-preserving sends.** RFC-0014's products see peers only as context-scoped aliases, while `send` takes a global identity. A future `ContactPeer` variant carrying an opaque address-book reference would let the host resolve alias → identity internally, so a product can "invite the friends who also use this app" without learning anyone's global identity. Deferred until RFC-0014's handle format settles; `ContactPeer` appends compatibly.  
- **Wire naming.** This RFC's methods generate `contacts_*` wire names (including `contacts_subscribe`); RFC-0014's methods should take non-colliding names (e.g. `contacts_list` / `contacts_list_subscribe`) when it lands.

## Drawbacks

- **No forward secrecy or rotation.** The exchange key is static; root-entropy compromise exposes everything (as in RFC-0007), and a leaked exchange secret cannot be replaced without re-deriving the identity's root entropy. Products needing forward secrecy should treat `derive_shared_key` as a session-bootstrap secret, not a long-term message key.  
- **No withdrawal or threading.** A submitted contact cannot be recalled: a later send to the same peer replaces it under LWW, but a contact already delivered is beyond recall. At most one contact per `(sender, recipient, product)` is outstanding, so coexisting invitations and reply-correlation are left to the product payload.  
- **Identity requirement.** Users without a registered primary username are unaddressable.

## Security Considerations

- **Trust model** is the RFC-0007 triangle: the host already holds root entropy, so the exchange secret adds no new class of secret. Products never see the exchange secret, `pairSecret`, or `perProductSecret` \- only leaf keys scoped to themselves.  
- **Exchange-key authenticity** rests on the proof-signer-equals-topic-identity check.  
- **Sender authenticity and replay.** The in-envelope sr25519 signature binds sender, recipient, product, ephemeral key, payload, and timestamp; replay to another recipient fails the recipient binding, replay to the same recipient is absorbed by dedup. A malicious product can only send *as itself*, and only after `ContactSend` is granted.  
- **Product isolation** is enforced twice: cryptographically (the product-keyed KDF layer) and at delivery (routing by the authenticated `product_id`).  
- **Spam and DoS.** Store admission costs sender-side quota/allowance; the LWW channel bounds per-sender-per-product footprint to one statement per recipient; UA policy bounds attention. The residual risk \- a funded adversary spamming a topic \- costs the recipient's host verification work only.

### Privacy

- A store observer sees that *some account* submitted to *this identity's* inbox topic, its timing, and size. Sender, product, and payload are inside the AEAD; the proof account has no protocol-level link to the sender's identity. This improves on today's practice, where product-specific topics reveal which product an invitation belongs to. Hosts SHOULD pad envelopes to coarse-size buckets.  
- The exchange key is public and identity-bound \- the same linkability class as the username; per-product unlinkability of accounts (RFC-0010) is untouched.  
- Traffic analysis of a recipient topic reveals contact *frequency*; mitigations (cover traffic, private retrieval) are out of scope.  
- Hosts MUST NOT emit automatic acknowledgments; replying is a product/user action.  
- The recipient's host necessarily learns the incoming contact graph \- the cost of host mediation, consistent with the trust model.

## Unresolved Questions

1. **Attestation registry.** Which source(s) feed the "notify eagerly" set \- on-chain attestations, host-curated lists, both? A shared registry format may deserve its own RFC.  
2. **Delivery status.** Is an opt-in host-level delivery signal ("recipient's host has seen it") wanted \- so a sender can tell a delivered contact from one evicted before a dormant recipient ever came online \- or is product-level reply the only acknowledgement this protocol should have?  
3. **Withdrawal.** Is LWW-replacement enough, or do products need best-effort retraction (tombstones) to recall an undelivered contact?  
4. **Payload ceiling.** The maximum payload size should be stated normatively once pinned against the store's chain constants.  
5. **Multiple identities.** `subscribe`/`send` bind to the active session identity (RFC-0009); is per-call identity selection needed?  
6. **HPKE.** Whether to adopt HPKE verbatim for the envelope's KEM/KDF step instead of the BLAKE2b construction.
