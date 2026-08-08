---
title: "Credential-endpoint remote permission"
owner: "@BigTava"
---

# RFC-0025: Credential-endpoint remote permission

|                 |                                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------------------------- |
| **RFC Number**  | 25                                                                                                                |
| **Start Date**  | 2026-08-03                                                                                                        |
| **Authors**     | Tiago Tavares                                                                                                     |
| **Description** | Add a `RemotePermission::Credential` variant granting one method and path, with a personhood proof the host attaches |

## Summary

Add one variant to `RemotePermission` ([RFC-0002](0002-permission-model.md)), `Credential`, granting outbound access to a single `(domain, path, method)` rather than a whole domain. Every request the grant covers carries a ring VRF proof of people-set membership and its contextual alias, both attached by the host.

Products keep issuing their own requests. The host sandbox already mediates every one of them, which is how `Remote` is enforced, so no new call is needed to attach the headers.

A backend behind a `Credential` grant MUST verify the proof. Personhood is not optional and there is no weaker mode to select.

## Definitions

- **Credential grant**. A `RemotePermission::Credential` decision covering one method and path on one domain, distinct from a `Remote` decision covering a domain set.
- **Ring VRF proof**. The anonymous bandersnatch proof of people-set membership produced by `create_account_proof` ([RFC-0004](0004-ringlocation-redesign.md)). Proves membership without revealing which member.
- **Contextual alias**. The identifier `create_account_proof` derives from the member key and a `ProductProofContext`. The same member key under different contexts yields different, unlinkable aliases ([RFC-0004](0004-ringlocation-redesign.md)).

## Motivation

A funding product wants a meld.io API key for fiat onramp. A game product wants TURN credentials for a relay. Neither can hold one, because a product runs in a host on the user's machine and anything given to it is readable by that user. [Meld](https://docs.meld.io/docs/meld-api/getting-started) says so directly: "Always call Meld from your backend. Direct calls from a browser or mobile app expose your API key."

The shape of the answer needs nothing new: the deployer runs a backend holding the credential, and the product calls it for a derived token, a relay ticket or a widget URL scoped to one buyer. `RemotePermission::Remote` already permits that call and `create_account_proof` already proves a real person is behind it. Two properties of the grant are wrong for this use. It is domain-wide, so a user approving "access to onramp.example.com" cannot distinguish a product that creates one payment session from one that reads every endpoint the deployer runs. And it carries no personhood: a product may send the alias anywhere, the user approves producing a proof but never its recipient, and the product supplies the `ProductProofContext` itself, so nothing stops it presenting a different alias on each visit and defeating any per-person limit the backend applies.

Requirements for a solution:

1. **The credential never reaches the product.** It stays on the deployer's backend, and what crosses to the product is a token derived from it.
2. **The user can tell what they approved.** A grant names one operation rather than a set the user has to reason about.
3. **Personhood disclosure names its recipient.** The user learns which endpoint receives their alias at the moment they approve it, not afterwards.
4. **A backend can rate limit one person.** The alias it receives is stable across visits and not something the product can vary.
5. **Nothing depends on the host platform.** The desktop and web hosts have no equivalent of Apple App Attest or Google Play Integrity and must not be second-class.
6. **Decisions already made survive.** Permissions persist indefinitely under [RFC-0002](0002-permission-model.md), and nothing here may invalidate one a user has already given.

## Detailed Design

### The variant

Added to `RemotePermission`:

```rust
/// Outbound access to one method and path on one domain, carrying a
/// personhood proof the host attaches (RFC 0025).
Credential {
    /// Domain the grant covers. Covered requests must be `https`.
    domain: String,
    /// Exact path the grant covers. No wildcards.
    path: String,
    /// HTTP method the grant covers.
    method: String,
}
```

It MUST be appended last and existing variants MUST NOT be reordered. `RemotePermission` is SCALE-encoded into `CoreStorageKey::PermissionAuthorization`, so amending `Remote` would change the key every stored decision was written under, and every lookup would miss and re-prompt. [RFC-0002](0002-permission-model.md) requires decisions to persist indefinitely.

`Credential` narrows `Remote`, it does not replace it. A product needing many operations still requests a domain and receives one prompt. A grant covers one triple, and a second triple is a second request, which is a second prompt: [RFC-0002](0002-permission-model.md) settled that when it rolled back batching.

Hosts canonicalise `domain` to lower case and `method` to upper case before keying a stored decision, as they already do for the domain set in `Remote`. `path` is case-sensitive and is keyed verbatim.

### Authorization

Same shape as any remote permission, with rules 2 and 3 specific to this variant:

1. **No session.** Denied ([RFC-0009](0009-unauthenticated-product-access.md)), and the host does not auto-prompt login.
2. **The requests the grant would cover are not `https`.** Denied. `domain` carries no scheme, so the host applies this to the scheme it would use for covered requests. The proof would otherwise travel in plaintext.
3. **The user is not a people-set member.** Denied. The host cannot produce the proof the grant requires, and `create_account_proof` returns `NotMember` for the same reason.
4. **Otherwise.** A prompt naming the method, the domain, the path, and the fact that this endpoint will receive a personhood alias. Granted on approval, denied on decline.

`RemotePermissionResponse` carries only `granted: bool`, so all four collapse to one answer. A product cannot distinguish "the user declined" from "the user is not verified" and route to onboarding. See Unresolved Questions.

### Proof attachment

A grant is prompted once and then persists ([RFC-0002](0002-permission-model.md)), so a product issues as many covered requests as it likes without further consent. Each one is proved separately.

On every request a `Credential` grant covers, the host attaches:

```text
X-Polkadot-Proof      Ring VRF proof, made over the message below.
X-Polkadot-Ring       Ring revision the proof was made against.
X-Polkadot-Timestamp  Unix seconds.
X-Polkadot-Nonce      Random per request.
```

The message passed to `create_account_proof` is

```text
blake2b256(
  "truapi/credential-request/v1"
  ++ len(method) ++ method
  ++ len(domain) ++ domain
  ++ len(path)   ++ path
  ++ len(query)  ++ query
  ++ timestamp_be64
  ++ nonce
  ++ blake2b256(body)
)
```

with each length a big-endian `u32` byte count. The length prefixes matter: plain concatenation makes `("GET", "a.com", "/b")` and `("GET", "a.com/", "b")` the same bytes. The label separates this digest from any other use of the same key. Query and body are covered because a proof over method, domain and path alone would authorise any content sent with them, which is the replay the nonce exists to bound.

The alias is not sent. It is the VRF output, so a verifier obtains it from the proof, and a separate header would only give a backend something to trust in its place. `X-Polkadot-Ring` is sent because a verifier cannot otherwise tell which ring snapshot to check against, the ring advancing as members are onboarded.

The host derives the `ProductProofContext` from the granted `(domain, path, method)` and never from anything the product supplies. That is what makes the alias stable for one person at one endpoint, and what stops a product presenting as several people or directing an alias to an endpoint the user did not approve.

The host MUST strip any caller-supplied header in the `X-Polkadot-` namespace before attaching its own.

A `Credential` grant authorises the host to produce these proofs for covered requests without a further confirmation. That is a departure from `create_account_proof`, which otherwise requires a per-call confirmation unless `AutoSigning` ([RFC-0010](0010-allowance.md)) covers the account. Keeping that rule here would mean a dialog on every HTTP request, which is unusable. The grant is the consent, and it is why the prompt must name the endpoint as the recipient of an alias rather than only as a network destination.

### Consuming-backend contract

A backend behind a `Credential` grant MUST:

> Verify the ring VRF proof against the People chain under the context derived from the endpoint being called and the digest of the request as received. Derive the per-person key it rate limits from the alias that verification returns, never from any other field of the request.

Passing the context and message as verification inputs is what makes those bindings enforced rather than checked. A proof obtained against a different endpoint, or over an earlier request, does not verify under the ones the backend supplies. Rejecting a timestamp outside an accepted window, and a nonce already seen inside it, is what bounds how long a captured proof stays useful.

## Implementation notes

Verification is one call. `verifiablejs`, a WASM binding of `paritytech/verifiable` published for both Node and bundlers, exposes `validate(proof, members, context, message)` returning the alias to key on. `web3-citizenship-web` already uses it for the ring-VRF flows against People. Note that the caller supplies `context`, so a proof made under any other context fails to verify rather than needing a separate check.

What a backend must supply is `members`. The ring is rebuilt from People chain storage and committed, which is too expensive to redo per request and wants caching keyed on the ring revision the request carries. That, rather than the cryptography, is the integration cost.

**Recommended backend implementation.**

- **Pin the context derivation with test vectors.** A backend computes the expected `ProductProofContext` independently, so the mapping from `(domain, path, method)` has to be reproducible rather than described.
- **Exchange one proved request for a session token.** A bandersnatch proof is large relative to an HTTP header and slow relative to a fetch, and a grant places no bound on how many requests follow it.
- **A conformance test.** The same user calling the same endpoint from two sessions yields the same alias, and a different endpoint yields an unrelated one.

## Drawbacks

- **Non-members cannot hold a `Credential` grant.** Personhood is mandatory, so anyone still verifying is refused. [RFC-0023](0023-account-sign-vrf.md) exists because that population needs a different path, and this design has no equivalent.
- **Backends need a chain connection, not just a verifier.** Verifying a proof is one library call, but obtaining and refreshing the ring member set is not, so a backend needs a People chain connection and a cache.
- **More prompts for chatty products.** One grant per operation is what buys a legible prompt. `Remote` remains for products that prefer one broad grant.
- **No bound on use after the grant.** [RFC-0002](0002-permission-model.md) persists a decision indefinitely and defines no revocation protocol, so a `Credential` grant authorises an unlimited number of proved requests until the user clears it in host settings.
- **The alias is per product.** A backend reached by several products sees a different alias per product for the same person, because [RFC-0004](0004-ringlocation-redesign.md) scopes contexts by product deliberately.

## Alternatives

- **Amending `Remote` rather than adding a variant.** One way to ask instead of two, but `RemotePermission` is SCALE-encoded into the persisted permission key, so it would invalidate every stored grant.
- **A `secrets.request` method proxying the call through the host.** An earlier draft. It moves outbound HTTP into the protocol, which [RFC-0002](0002-permission-model.md) assigned to the sandbox, then needs SSRF rules, size bounds, redirect handling, and header stripping to contain what that creates. On the web host, itself a browser page, CORS still applies, so the same call behaves differently per platform.
- **Leave the product to attach its own proof.** It can today, and a backend that pins the context can force the binding. What it cannot do is tell the user, at grant time, that this endpoint receives their alias.
- **Declaring the endpoint in a dotNS text record.** The deployer publishes the backend address and the host resolves it, so rotating it needs no product redeploy and a shared service is declared once rather than copied into every caller. This was the centre of an earlier draft. Set aside because a frontend carrying its own API address is ordinary, and a lookup on the path of every grant adds a failure mode for an ergonomic gain.
- **A trusted execution environment operated by network nodes.** The credential is encrypted to an attested enclave rather than held by a deployer, removing the need for deployer infrastructure entirely. Parachains built for confidential compute, such as Integritee and Phala, exist for this. Out of scope because it replaces the backend rather than the grant, and attestation moves trust to a hardware vendor rather than removing it. Worth revisiting if requiring every deployer to run a service blocks adoption.
- **Reputation or governance approval instead of personhood.** A backend could admit callers with an accrued track record, or governance could curate which endpoints products may reach. Both gate on standing that must be earned or granted, so a new user and a new deployer are excluded either way, and neither has a substrate a backend can check without coordination. The People chain gives verification with none.
- **Path prefixes in the grant.** Fewer prompts, but a prompt naming a prefix asks the user to reason about a set, which is what domain grants already do badly.

## Prior Art and References

- **[RFC-0002](0002-permission-model.md)**, permission model. The enum extended here, and the source of the one-grant-per-prompt rule after batching was rolled back.
- **[RFC-0004](0004-ringlocation-redesign.md)**, `create_account_proof`. The ring proof and the `ProductProofContext` whose derivation the host takes over here.
- **[RFC-0009](0009-unauthenticated-product-access.md)**, the no-session gate and the no-auto-login rule.
- **[RFC-0010](0010-allowance.md)**, `AutoSigning`. The confirmation rule this RFC departs from for covered requests.
- **[RFC-0023](0023-account-sign-vrf.md)**, `sign_vrf`. The sr25519 path for participants not yet in the people set, which this does not use.
- [Meld API getting started](https://docs.meld.io/docs/meld-api/getting-started), for the backend-only constraint.

## Unresolved Questions

- **Should the response distinguish denial from ineligibility?** `granted: bool` cannot express "not a people-set member", so a product cannot route the user to onboarding. Widening it touches every remote permission, not only this variant.
- **How is the context derived from `(domain, path, method)`?** It must be reproducible by an independently written verifier, and pinned with test vectors.
