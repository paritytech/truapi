---
title: "sr25519 VRF signing for product accounts"
owner: "@valentunn"
---

# RFC 0023 — sr25519 VRF signing for product accounts

|                 |                                                                                                          |
| --------------- | -------------------------------------------------------------------------------------------------------- |
| **RFC Number**  | 23                                                                                                       |
| **Start Date**  | 2026-07-21                                                                                               |
| **Description** | Add a general-purpose `account_sign_vrf` method producing an sr25519 VRF signature from a product account. |
| **Authors**     | Valentin Sergeev                                                                                         |

## Summary

Add one method to the `Account` trait, `account_sign_vrf`, producing an
**sr25519 (schnorrkel) VRF signature** from a product account. The caller
supplies the signing transcript as a structured recipe — a root label plus an
ordered list of `(label, value)` items — which the host replays into a Merlin
transcript and signs; the response is the schnorrkel `VRFPreOut` and `VRFProof`.

Authorization matches ordinary product-account signing (`sign_raw`): a per-call
confirmation, unless `AutoSigning` (RFC-0010) covers the account, in which case
the host signs locally. The motivating consumer is the People Chain lottery
ticket a participant submits while **not yet a people-set member**; members use
the bandersnatch ring-VRF path instead (`create_account_proof`, RFC-0004).

## Definitions

- **sr25519 VRF** — schnorrkel's VRF over Ristretto25519. Signing consumes a
  Merlin transcript; the output splits into a `VRFPreOut` (32-byte output point)
  and a `VRFProof` (64-byte DLEQ proof), both needed on-chain.
- **Merlin transcript** — the STROBE-128 transcript schnorrkel signs over, built
  from a root domain-separation label and a sequence of labeled message appends.
- **Product account** — an sr25519 account at `//product//{productId}/{suffix}`
  (RFC-0022).
- **`AutoSigning`** — the RFC-0010 capability handing the host a product's
  subtree secret key so it can sign locally without a round trip.
- **Host** / **Account Holder** — the runtime executing products, and the device
  holding the user's root secret (RFC-0010).
- `Bytes` — a variable-length byte array (SCALE `Vec<u8>` on the wire).
- `ProductAccountId` — `{ dot_ns_identifier: String, derivation_suffix: Bytes }`
  (RFC-0022).

## Motivation

The People Chain airdrop flow requires a participant to submit an sr25519 VRF
signature as a lottery ticket, over a transcript the runtime builds like this
(`indiv_pallet_airdrop::vrf::transcript_for_event`):

```rust
fn transcript_for_event(event_id: &[u8], public_key: &[u8]) -> Transcript {
    let mut domain = Vec::with_capacity(VRF_TRANSCRIPT_LABEL.len() + event_id.len());
    domain.extend_from_slice(VRF_TRANSCRIPT_LABEL);
    domain.extend_from_slice(event_id);

    let mut transcript = Transcript::new(b"pop:airdrop");
    transcript.append_message(b"domain", &domain);
    transcript.append_message(b"signer", public_key);
    transcript
}
```

Products hold no private keys, so the signature must come from the host, bound
to the product's account. No existing method fits: `create_account_proof`
(RFC-0004) is a ring-VRF for people-set members; `sign_raw` / `sign_payload`
produce ordinary signatures, not VRF output.

This transcript is one consumer; asset-hub smart contracts can define others.
So the API is **general-purpose** — it replays an arbitrary Merlin transcript
rather than hard-coding one pallet's shape, so new consumers need no host
change.

**Members vs. non-members.** This sr25519 path is for participants **not yet**
people-set members (still verifying); members use the anonymous bandersnatch
ring-VRF (`create_account_proof`, RFC-0004). The two are complementary.

## Detailed Design

### Method

Added to the `Account` trait:

```rust
/// Produce an sr25519 (schnorrkel) VRF signature from a product account.
///
/// The host builds a Merlin transcript from `transcript_label` and `items` and
/// signs it with `account`'s key, returning the VRF pre-output and proof.
/// Authorized like `sign_raw`: local when `AutoSigning` covers the account,
/// otherwise a per-call user confirmation.
#[wire(request_id = 164)]
async fn account_sign_vrf(
    &self,
    _cx: &CallContext,
    _request: HostAccountSignVrfRequest,
) -> Result<HostAccountSignVrfResponse, CallError<HostAccountSignVrfError>> {
    Err(CallError::unavailable())
}
```

### Request, response, and error

```rust
struct HostAccountSignVrfRequest {
    /// Account whose key signs the VRF.
    account: ProductAccountId,
    /// Root domain-separation label: `Transcript::new(transcript_label)`.
    transcript_label: Bytes,
    /// Replayed in order as `transcript.append_message(item.label, item.value)`.
    items: Vec<VrfTranscriptItem>,
}

/// One `append_message` call against the transcript.
struct VrfTranscriptItem {
    label: Bytes,
    value: Bytes,
}

/// `HostAccountSignVrfResponse = VrfSignature`.
struct VrfSignature {
    /// schnorrkel `VRFPreOut` — the VRF output point.
    pre_output: [u8; 32],
    /// schnorrkel `VRFProof` — the DLEQ proof.
    proof: [u8; 64],
}

enum HostAccountSignVrfError {
    /// No authenticated session (RFC-0009). The host must not auto-prompt login.
    NotConnected,
    /// The user declined the signing confirmation.
    Rejected,
    Unknown { reason: String },
}
```

### Transcript replay

The host reconstructs the transcript deterministically and signs it, performing
**no** interpretation of labels or values — a pure replayer, which is what lets
one method serve any consumer's transcript:

1. `let mut t = Transcript::new(transcript_label);`
2. for each `item` in order: `t.append_message(item.label, item.value);`
3. `let (io, proof, _) = keypair.vrf_sign(t);`
4. return `VrfSignature { pre_output: io.to_preout().to_bytes(), proof: proof.to_bytes() }`.

Reproducing the People Chain transcript above:

```rust
HostAccountSignVrfRequest {
    account,
    transcript_label: b"pop:airdrop".to_vec(),
    items: vec![
        VrfTranscriptItem { label: b"domain".to_vec(), value: [VRF_TRANSCRIPT_LABEL, event_id].concat() },
        VrfTranscriptItem { label: b"signer".to_vec(), value: account_public_key.to_vec() },
    ],
}
```

The caller supplies its `account_public_key` for the `signer` item (from
`host_account_get_account`); the host does not inject it. The root label is a
distinct field, not the first `items` entry, because `Transcript::new`'s framing
is not equivalent to a later `append_message` (see
[Implementation notes](#implementation-notes)).

### Authorization

Same rules as ordinary product-account signing (`sign_raw`):

1. **No session** → `NotConnected` (RFC-0009); the host does not auto-prompt
   login.
2. **`AutoSigning` covers the account** (granted for the account's product, and
   the account is in that product's own subtree) → the host signs locally, no
   round trip.
3. **Otherwise** → a per-call confirmation (`UserConfirmationReview::SignVrf`, a
   new review variant); signs on approval, `Rejected` on decline.

Cross-product signing is allowed: an account outside the caller's own product
simply always takes path 3 (a prompt), never a silent local-sign, and there is
no account-ownership error. The prompt-free path is therefore confined to a
product's own subtree, so a product can never *silently* obtain a VRF bound to
another identity — only with the user's explicit per-call consent, exactly as
with `sign_raw`. That, plus per-product hard derivation (RFC-0022) and the
consuming-runtime contract below, keeps the free-form transcript safe.

### Consuming-runtime contract

A runtime that verifies these VRFs MUST:

> Verify the proof against the public key of the account it authorizes, and
> derive any in-transcript `signer` field from that same key — never from
> caller-supplied data.

Then arbitrary transcript content is safe: schnorrkel binds the proof to the
signing key, so a product that writes a foreign `signer` value produces a proof
that verifies only under its own key and a transcript no correct verifier
reconstructs — it simply fails. The People Chain transcript already binds
`signer = public_key`.

Runtimes should also keep the VRF **input narrow** (per schnorrkel's guidance):
the narrower the bound transcript, the less a participant can manipulate the
draw. That is the runtime's responsibility; this method signs whatever it is
given.

### Accounts Protocol companion

For the non-`AutoSigning` path, the Host ↔ Account Holder boundary gains the
mirror request, shaped like RFC-0004's `create_account_proof` companion:

```rust
/// Host → Account Holder.
fn sign_account_vrf(
    calling_product_id: ProductId,
    account: ProductAccountId,
    transcript_label: Bytes,
    items: Vec<VrfTranscriptItem>,
) -> Result<VrfSignature, SignVrfErr>;
```

The Account Holder derives the account, presents the confirmation, and signs.
`SignVrfErr` carries the `Rejected` / `Unknown` cases.

## Implementation notes

Hosts build the transcript with Merlin (schnorrkel's transcript type), whose API
requires every **label** to be `&'static [u8]` (`Transcript::new`,
`append_message`, …); only message *values* accept runtime lifetimes. A
general-purpose host receives its labels (`transcript_label`, each `item.label`)
as **runtime bytes off the wire**, so it cannot pass them to stock Merlin
directly.

This is an implementation obstacle, **not** a security constraint. The
`&'static` bound is misuse-resistance — it nudges a protocol author to hard-code
their labels — while the binding guarantee comes from Merlin's tag-length-value
framing, which encodes runtime-length labels just as unambiguously. `Box::leak`
and `unsafe` casts to `'static` are sanctioned workarounds, so the bytes'
provenance is irrelevant to correctness.

**Merlin author consultation.** Jeff Burdges (Merlin / schnorrkel /
ark-transcript author) was consulted: he called the `&'static` labels an
annoyance and suggested forking Merlin to relax them ("we could fork merlin and
take that out"), said abstracting the transcript this way has been done before
and is reasonable, and noted the structured `(label, value)` shape is harder to
abuse than a raw pre-accumulated transcript. The acceptability of an `unsafe`
cast to `'static` comes from his earlier public Merlin threads, not this
consultation. The one hazard raised in discussion — domain collision from
arbitrary labels — does not apply here, since signatures are bound to per-account
derived keys.

**Recommended host implementation.**

- A **patched/vendored Merlin** relaxing `&'static [u8]` → `&[u8]` on label
  params: same STROBE-128 body, so byte-identical to stock Merlin with no
  `unsafe` in host code. (Stock-Merlin equivalent: an
  `unsafe transmute::<&[u8], &'static [u8]>` per label call — sound because
  Merlin absorbs labels synchronously and never retains the reference.)
- A **conformance test** asserting byte-equality vs stock `merlin::Transcript`,
  plus an end-to-end "verifies under `sp_core` sr25519" test (the analogue of
  the pallet's `transcript_matches_sp_core`).
- **Bound** `items.len()` and total transcript size against a hostile caller.

**Dead ends.** `Transcript::new(b"")` + append is *not* framing-equivalent to
`Transcript::new(label)`, so the root label must go into the constructor. And
`ark-transcript` — though it allows dynamic labels — wraps SHAKE128 with postfix
framing (not STROBE-128) and feeds the arkworks/bandersnatch stack; schnorrkel
cannot consume it and its output would never verify under sr25519.

## Non-goals

For **infrequent, identity-bound** VRFs (lottery ticket / airdrop claim), where
binding the draw to the product account is the point and a per-call confirmation
or one-time `AutoSigning` grant is acceptable.

**Not** for fast-moving in-game randomness: that should use a **device-local
game key** — held on the player's device, able to act only in the game and never
over money — instead of round-tripping every draw through host signing or
widening `AutoSigning` to a game loop.

## Drawbacks

- **Free-form transcript surface.** Hosts must bound transcript size and cannot
  validate transcript *semantics*; correctness of the bound draw rests with the
  consuming runtime.
- **The caller must supply the `signer` public key** (via
  `host_account_get_account`) rather than the host inserting it.

## Alternatives

- **Opaque single blob** (`signing_context(context).bytes(input)`) — only a
  single-append transcript; can't reproduce the People Chain's `Transcript::new`
  plus two appends, so it fails on-chain.
- **Enumerated per-pallet shapes** — a wire enum per consumer; every new
  consumer would need a host release, defeating the general-purpose goal.
- **Host-injected `signer` placeholder** — unnecessary: the product can fetch
  its own pubkey, and a spoofed `signer` can't compromise another identity, so
  injection buys only ergonomics.
- **`ark-transcript`** — incompatible with schnorrkel/Merlin (see
  [Implementation notes](#implementation-notes)).

## Prior Art and References

- **RFC-0004** — `create_account_proof`; the ring-VRF path for people-set
  members, complementary to this sr25519 path for non-members.
- **RFC-0009** — the `NotConnected` gate and no-auto-login rule.
- **RFC-0010** — allowance and `AutoSigning`.
- **RFC-0022** — account key derivations; `ProductAccountId` and the
  `//product//{productId}/{suffix}` scheme this method signs with.
- **Merlin / schnorrkel** — the transcript and VRF primitives; the static-label
  discussion and Jeff Burdges' input are in
  [Implementation notes](#implementation-notes).
