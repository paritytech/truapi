---
title: "Funding Modality"
owner: "Shawn Coe"
status: draft
---

# RFC — Funding Modality

|                 |                                                                                                          |
| --------------- | -------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-08-04                                                                                               |
| **Description** | A modality that moves value between a user's Polkadot balance and cash, banks, cards, or other chains      |
| **Authors**     | Shawn Coe                                                                                                |

## Summary

Add a `Funding` trait defining the runtime contract for a new **funding modality**: the one surface for moving value across the boundary between a user's Polkadot balance and everything outside it. Both directions, over the rails the product needs — a card or bank transfer, a swap from another chain, and cash traded in person with a peer.

**Consumers** — the Host's own balance card, or any product that hits an insufficient balance mid-flow — declare an intent and watch it to completion. **Providers**, products declaring the modality in their manifest under the [Product Manifest Format RFC][manifest], receive intents addressed to them and report progress. Route declaration is static and lives in the manifest, so the Host builds the rail list from dotNS records without launching any provider.

Three properties constrain every type below:

- **The Host never holds a third-party credential, never custodies in-flight funds, and never owns a fiat or cash leg.** Its job is: declare intent → hand off → confirm the on-chain leg → resume.
- **The Host trusts only what it observes on-chain.** A provider's report says when to look; it is never itself proof.
- **The list is keyed on the rail, not the provider.** "From a bank account" survives a provider being replaced; a provider's brand does not.

The directions are deliberately asymmetric and the types say so. Inbound, the Host proves the outcome and emits `Delivered`. Outbound, it can prove only that funds left under the user's authorization and emits `Released`, because no host can verify that cash reached a hand or a bank account.

## Motivation

**An empty balance is a dead end today.** The only funding path is `PaymentTopUpSource` — `ProductAccount` and `PrivateKey` from [RFC 0006](0006-payments.md), plus `Coins` appended by [RFC 0021](0021-payment-topup-coins.md). All three mean *the product already controls this money, move it into the user's purse*. None answer the question a user with nothing actually asks, which is where money comes from in the first place. A product that reaches `HostPaymentError::InsufficientBalance` has nowhere to send the user.

**Getting value out has no path at all.** `Payment::request` sends to an account, which moves value sideways rather than out. A user holding a balance and wanting cash, and a business wanting collected funds in a bank account, are the same protocol gap in the other direction. Excluding it would be a false economy: the rails are the same three, the providers are the same providers, the eligibility question is the same question, and a contract built for one direction would have to be reopened for the other. One modality, two directions, is smaller than two modalities.

**Every provider integration otherwise accretes in the Host.** Without a contract, each on-ramp arrives as bespoke Host code: its own screens, its own credential handling, its own idea of what "done" means. Three providers in, the Host owns three fiat relationships it should never have touched. A declared contract pushes that mess outward — the provider hosts its own flow, and the Host's surface stays the same size whether there is one route or twelve.

**A provider needing a server-side credential is already served.** Nothing in this contract transports a credential, and it does not need to: the [Secrets RFC][secrets] covers exactly that case, and a fiat-onramp API key is its motivating example. A provider publishes a `secret:<name>` dotNS record naming its own backend, calls `secrets.request`, and the credential stays on infrastructure the deployer runs. The Host attaches a caller proof derived from the user's own credentials and never sees the key.

Funding therefore says nothing about credentials beyond declining to carry them. A route needing one composes the two RFCs rather than widening either.

**The valuable trigger is the one no design covers.** Three entry points exist, and they are not variations of one thing:

| Trigger                       | Character      | Requirement                                    |
| ----------------------------- | -------------- | ---------------------------------------------- |
| Cold start / onboarding       | patient        | teach the model; zero balance, so unavailable routes are likely |
| Explicit top-up from a card   | routine        | destination pre-filled, list pre-filtered      |
| Insufficient balance mid-flow | time-pressured | **resume the interrupted action**, not just notify |

The third produces recurring volume rather than one-time top-ups, and it is unbuildable unless the intent carries enough context to resume. That is why `resume` is in the intent type and not deferred.

**Two routes need no partner at all.** Funding from a friend is a payment request, and `CoinPayment::create_receivable` plus `CoinPayment::listen_for_payment` cover it — the latter emits `Channel` then `Cheque`, which is exactly the friend-to-friend handoff. Funding from an exchange is a deposit address plus a watch on it, and that one is *not* covered: `Chain::follow_head_subscribe` follows chain head, and nothing in the protocol watches a specific address for an inbound transfer. That watch is Host-internal work this RFC assumes rather than specifies.

Both routes need no counterparty agreement, which is the point: the modality can ship, and be exercised end to end, before any partner term sheet exists.

## Stakeholders

- **Host developers** — render the sheet and the in-flight surface, resolve eligibility, mint receiving targets, collect release authorization, confirm the on-chain leg, persist sessions across restart.
- **Provider product developers** — declare the funding modality and their routes, serve intents, report progress.
- **Consumer product developers** — payment terminals and anything else that can run out of balance mid-action; the resume path is for them. A business treasury wanting bank payout is the outbound consumer.
- **Operators of the route configuration** — own the jurisdiction and verification matrix and the ability to enable or disable a route per market without an app release.
- **Coinage / payment component owners** — the on-chain side of every route; the asset taxonomy question below is theirs to settle.

## Detailed Design

### Terminology

- **Rail** — the external side: cash, a bank account or card, a friend, an exchange, another chain. The primary key of the list, labelled "from" or "to" by direction.
- **Direction** — inbound (on-ramp) or outbound (off-ramp).
- **Route** — one provider's declared path over a rail, in a direction, for a set of assets.
- **Intent** — a declared request: direction, asset, amount, and the action to resume.
- **Session** — an intent in flight. Survives minimize and Host restart.
- **Handoff** — transfer of the screen to the provider, inside a surface the Host can reclaim.
- **Arrival** — the Host's own on-chain observation that funds landed. The inbound terminal success.
- **Release** — funds leaving under the user's authorization. The outbound terminal success, and a weaker claim than arrival.

### UI ownership

The modality is mostly Host UI. A provider contributes at one of two levels:

| Level | Provider supplies | Host supplies | Used for |
| ----- | ----------------- | ------------- | -------- |
| **Static** | manifest metadata — display name, icon, latency, whether an account is needed | the entire row, the whole list, all chrome | every source row, before selection |
| **Framed** | its own App executable | the frame, the dismiss affordance, and the right to reclaim | everything after selection |

**Static is the default and covers the whole pre-selection surface.** The Host builds every row from manifests, so nothing provider-authored executes before the user picks a source. This is what the design's *"no UI until picked"* note already asserts, and it is why the sheet renders instantly on an empty wallet.

**Framed is a takeover, not an embed.** The provider's App executable runs in a frame the Host owns; the Host keeps the dismiss affordance and can reclaim the screen at any point. Verification, card entry, and deposit instructions all live here, because they are the provider's own surface and the Host must not reimplement them.

There is deliberately no third level in which a provider supplies a UI fragment the Host renders natively. That would need the `CustomRendererNode` tree to flow product → Host, and it currently flows only Host → product (`Chat::custom_message_render_subscribe` is registered on the Host implementation). Adding the reverse direction is mechanically straightforward and rejected here for a different reason: a native-looking tree the Host draws inside its own chrome, with no constrained node subset, no size bounds, no attribution, and no reserved status area, would let a provider render "Delivered" or "Verified" in Host styling — contradicting the ownership rules below. Specifying a safe renderer profile is real work, and the routes that would benefit are not the ones shipping first. See [Out of Scope](#out-of-scope).

Mapping the funding-modality design's fourteen states to this boundary. The design covers the inbound flow, so the direction-specific rows are marked:

| Design state | Owner | Notes |
| ------------ | ----- | ----- |
| Balance card | Host | Pocket surface; `+` triggers inbound, send-out triggers outbound |
| Sources | Host | rails built from manifests; provider name is a subtitle |
| Why these options | Host | the Host's own eligibility explanation, never a provider's |
| Destination | Host | resolved against the Host asset registry |
| Amount authorization *(outbound)* | Host | the debit consent; a provider must never collect it |
| No route | Host | renders a `FundingFailure`, no provider involvement |
| Amount | Host | validated against the route's declared limits |
| Below minimum | Host | `BelowMinimum` |
| Deposit QR | **Provider, framed** | the address and its window are the provider's, shown in its own frame |
| Confirming | Host | stamp bar from `HostFundingStatusSubscribeItem` |
| Minimized | Host | docked pill and pending balance row |
| Delivered | Host | inbound only, and only on the Host's own chain observation |
| Released *(outbound)* | Host | the Host saw funds leave; the off-chain leg is unconfirmed |
| Callback | Host → consumer | delivers `resume` to the product that declared the intent |
| Expired | Host | `Expired` |
| ID check | **Provider, framed** — today | becomes a Host surface if verification is Host-owned; see [Unresolved Questions](#unresolved-questions) |

Twelve of the design's fourteen states are Host-owned. The two that are not are provider takeovers inside a frame the Host can reclaim, and one of those — verification — has an open question over whose surface it should be at all. That distribution is the point: the Host owns the vocabulary of funding — rails, eligibility, money-in-flight, arrival, release — and a provider owns only the step where it must be the one acting.

Two consequences worth stating normatively, because both are load-bearing for review:

- **A provider never draws the stamp bar, the pill, or a terminal state.** Those come from the Host's own observation, so a provider cannot show "delivered" for something that did not arrive.
- **A provider never draws the rail list or the eligibility explanation.** Otherwise the operator's routing logic would have to be disclosed to the party being routed around.
- **A provider never collects the authorization to release funds.** An outbound session debits the user, so the consent is Host UI. A provider that could ask for it could ask for more than the user agreed to.

### Rails and directions

The product needs three partner-backed route families, and two that need no partner at all. Both directions run over the same set:

| Rail | Inbound | Outbound | Provider today | Regulated boundary |
| ---- | ------- | -------- | -------------- | ------------------ |
| `Friend` | a peer sends value | a peer receives it | none — native request | none |
| `ExternalWallet` | deposit from an exchange | withdraw to one | none — an address | none |
| `ForeignChain` | swap in from another chain | swap out to one | in-house swap app | none, non-custodial |
| `BankOrCard` | card or bank transfer in | payout to a bank | third-party ramp | provider and operator |
| `Cash` | cash to a peer agent | cash from one | peer-to-peer cash network | operator and agent |

`Friend` and `ExternalWallet` need no counterparty agreement, which makes them the two that ship first and the reason the modality is testable end to end before any partner terms exist. The other three are additive: each is one manifest declaration and one provider, with no change to this contract.

Outbound over `Cash` is the case that most exposes the asymmetry below — the user hands over tokens and expects paper money from a stranger. That is exactly why a peer-to-peer cash network puts value in escrow, and why this RFC does not pretend the Host can adjudicate it.

### Declaring the modality

Funding is a modality in the [Product Manifest Format][manifest] sense: a user-facing surface backed by an executable. A provider adds it to its worker's `includes`, alongside `chat` and `pocket`:

```ts
includes: { chat?: boolean; pocket?: boolean; funding?: boolean };
```

`funding: true` alone advertises that the worker serves intents. What it can actually reach is declared statically in the same executable manifest:

```ts
/** Static route declaration. Absent unless `includes.funding` is true. */
funding?: {
  routes: Array<{
    /** External side this route reaches. */
    rail: FundingRail;
    /** Directions this route serves. A route may serve one or both. */
    directions: FundingDirection[];
    /** Assets this route can move. */
    assets: FundingAsset[];
    /** Typical time to settle, in seconds. Shown before the user commits. */
    latency_seconds: number;
    /** Whether the user must hold an account with the provider. */
    requires_account: boolean;
  }>;
};
```

Keeping this in the manifest rather than behind a call is deliberate. The Host builds the entire rail list from dotNS records, so it needs no round-trip to a provider that may not be running, and an empty-wallet user is never shown a spinner. It also means a provider changes its routes by republishing a manifest, not by shipping through the Host.

Per the current modality-consent position, a user who has engaged with a product has implicitly approved its modalities; funding adds no separate prompt at declaration time. Consent still applies to what a route *does* — verification, custody, meeting a stranger — and that consent is collected by the provider inside its own flow.

### Assets, rails, and direction

```rust
/// Identifies one funding session.
///
/// Durable across Host restart, so a product that reloads reattaches with
/// [`Funding::status_subscribe`] rather than starting over.
type FundingIntentId = String;

/// Amount in the funded asset's own units.
///
/// Distinct from `Balance`, which is denominated in the Host's single fixed
/// payment asset. A funding session names its asset, so its units are not
/// fixed.
type FundingAmount = u128;

/// Which way value crosses the boundary.
enum FundingDirection {
    /// Value moves in. The Host confirms arrival by observing the chain.
    In,
    /// Value moves out. The Host confirms only that funds left under the
    /// user's authorization; the off-chain leg is the provider's obligation.
    Out,
}

/// A user-visible asset a session moves.
///
/// Opaque to products: an identifier the Host resolves against its own asset
/// registry. Products do not construct one.
struct FundingAsset {
    /// Host-assigned stable asset identifier.
    id: [u8; 32],
}

/// The external side of a session, and the primary key of the list the user
/// picks from.
enum FundingRail {
    /// Another user. No provider and no regulated boundary.
    Friend,
    /// An exchange or external wallet the user already controls.
    ExternalWallet,
    /// Crypto on another chain, reached by a swap.
    ForeignChain,
    /// A bank account or payment card.
    BankOrCard,
    /// Physical cash, in person.
    Cash,
}
```

`FundingAsset` is opaque on purpose. Whether a privacy-preserving dollar is a distinct asset or a mode of the ordinary one is unsettled (see [Unresolved Questions](#unresolved-questions)), and this RFC does not need the answer: either resolution is a Host-side registry change, not a wire change. A rail the user is routed *through* — a centralized stablecoin used to settle between two venues, an intermediate hop — has no `FundingAsset` and therefore cannot reach the sheet at all.

Opacity raises an obvious question: if a product cannot construct an asset id, how does it name one? In v1 it does not, and does not need to. A product reaches this modality in one of two situations, and neither requires naming an asset:

- **It ran out mid-action.** It was spending from a purse, so it passes `purse: Some(..)` — or `None` for the main purse — and `asset: None`. The Host knows what that purse holds and resolves it.
- **The user asked, from a balance card.** That card is Host UI, so the Host supplies the asset internally and no product is involved.

`asset: Some(..)` therefore exists for the Host's own callers and for a product handed an id by a prior call, not as something a product mints. A product that genuinely needs to name an arbitrary asset cannot today; a descriptor lookup is [Out of Scope](#out-of-scope), and the Host's own balance card needs one regardless to render a symbol and decimals.

`FundingRail` is the list key so that replacing a provider leaves the user-visible surface untouched. New variants append.

### Declaring an intent

```rust
/// Request to open the funding modality.
struct HostFundingRequest {
    /// Which way value moves.
    direction: FundingDirection,
    /// Asset to move. `None` lets the Host resolve it from `purse`, or asks
    /// the user. Products generally pass `None`.
    asset: Option<FundingAsset>,
    /// Amount sought. `None` lets the user choose.
    amount: Option<FundingAmount>,
    /// Purse credited on an inbound session, debited on an outbound one.
    /// `None` means `MAIN_PURSE`.
    purse: Option<CoinPaymentPurseId>,
    /// Opaque context returned verbatim when the intent settles.
    ///
    /// The caller uses this to resume an action the missing balance
    /// interrupted. The Host stores and returns it without interpretation and
    /// discloses it to no provider. Bounded to 4 KiB; see below.
    resume: Option<Vec<u8>>,
}

/// Accepted intent.
struct HostFundingResponse {
    /// Identifier for the session, durable across Host restart.
    intent: FundingIntentId,
}
```

`resume` is the whole mid-payment trigger. The Host cannot know what a product needs in order to pick up where it left off, so it carries the bytes and hands them back. Because the Host does not interpret them and no provider sees them, a product can put a payment id, a cart, or a signed continuation in there without widening anyone's trust surface.

Opaque durable storage of caller-supplied bytes needs limits, or it becomes an unbounded write primitive and a replay vector:

- **Bounded.** At most 4 KiB encoded. Larger is rejected with `ResumeTooLarge` rather than truncated. It is a continuation handle, not a document store.
- **Bound to the caller.** Stored under the calling product's identity and returned only to that product. A different product subscribing to the same session receives status without `resume`.
- **Expiring.** Discarded when the session reaches a terminal state and is delivered, or when the session expires. It does not outlive the intent that carried it.
- **One-shot.** Delivered once per terminal state, on the terminal item. Not replayable by resubscribing.
- **Not a capability.** The Host attaches no authority to these bytes. A product that encodes something authorising an action MUST make it independently verifiable on its own side — nonce, expiry, or signature — because the Host guarantees only that the bytes are the ones that product supplied for that session, not that acting on them is safe.

That last point is the important one. The Host's guarantee is integrity of custody, not integrity of meaning. A product treating `resume` as proof that a payment should now complete has built a replayable authorisation and the protocol cannot detect it.

A returned `intent` outlives the calling product's page. A product that reloads reattaches with `status_subscribe`.

### Watching a session

```rust
/// Progress of a funding session, ending in exactly one terminal item.
///
/// The success terminal differs by direction, because the guarantees do.
enum HostFundingStatusSubscribeItem {
    /// Inbound: awaiting the user's deposit on the external side.
    AwaitingDeposit {
        /// When the provider's window closes, in Unix milliseconds.
        expires_at: Option<u64>,
    },
    /// Outbound: awaiting the user's authorization to release funds.
    AwaitingRelease,
    /// The external-side leg has been seen but is not settled.
    Confirming,
    /// Value is moving between chains or venues.
    Bridging,
    /// Inbound terminal success. Funds observed on-chain by the Host.
    Delivered {
        /// Amount credited, which may differ from the amount requested.
        credited: FundingAmount,
        /// Resume context supplied in the intent.
        resume: Option<Vec<u8>>,
    },
    /// Outbound terminal success. Funds left under the user's authorization.
    ///
    /// The provider now owes the off-chain leg. That obligation is outside
    /// what the Host can observe or enforce.
    Released {
        /// Amount debited.
        debited: FundingAmount,
        /// Resume context supplied in the intent.
        resume: Option<Vec<u8>>,
    },
    /// Terminal failure.
    Failed {
        /// Why it ended.
        reason: FundingFailure,
        /// Amount moved before the failure. May be non-zero.
        moved: FundingAmount,
        /// Resume context supplied in the intent.
        resume: Option<Vec<u8>>,
    },
}
```

The non-terminal variants are the stamp bar — one vocabulary for money in flight, shared with trades and payments, so no feature invents its own synonym for "pending". `AwaitingDeposit` and `AwaitingRelease` are the direction-specific heads of it: inbound the user is being asked to send, outbound to authorize.

Every terminal variant carries an amount because partial movement is a real outcome on most routes, matching `HostPaymentTopUpError::PartialPayment`. `CoinPaymentStatus` reports the same idea as `cleared`; the name differs because that type counts Coinage coins clearing over several blocks, whereas this one counts a single asset amount.

**The two success terminals are not interchangeable, and that is the most important thing in this RFC.**

`Delivered` is a claim the Host can back. It watched the chain and saw the funds arrive. A provider reporting success does not produce it.

`Released` is a weaker claim, and deliberately so. The Host saw funds leave under the user's authorization; it has no way to know whether the bank credited an account or a stranger handed over notes. Every off-ramp ends on someone's word, and the Host is not in a position to check it. Collapsing both into one "done" state would let the UI tell a user their money arrived when the protocol has no idea, so the protocol refuses to offer that state. What follows a `Released` — attestation, escrow, a dispute path, a human — is the provider's obligation, and standardizing it is [Out of Scope](#out-of-scope).

### Reason codes

```rust
/// Why a route is not offered, or why a session ended.
///
/// Carries an outcome, never a rule. A client can render and classify these
/// without holding any part of the operator's compliance logic.
enum FundingFailure {
    /// Not offered in the user's market.
    RegionUnavailable,
    /// The user must complete verification first.
    VerificationRequired,
    /// Verification was attempted and refused.
    VerificationRefused,
    /// Below the route's minimum.
    BelowMinimum,
    /// Above the route's maximum.
    AboveMaximum,
    /// The user's balance does not cover an outbound session.
    InsufficientBalance,
    /// The deposit window closed before funds arrived.
    Expired,
    /// Funds arrived in the wrong asset or on the wrong chain.
    WrongAssetOrChain,
    /// The route was withdrawn while the session was in flight.
    RouteWithdrawn,
    /// The provider stopped responding.
    ProviderTimeout,
    /// The user abandoned the flow, or declined to authorize an outbound
    /// release.
    Cancelled,
    /// A counterparty did not appear, or a dispute is open.
    CounterpartyFailed,
    /// Route-specific outcome not covered above.
    ///
    /// Present so an operator can introduce an outcome without a wire change.
    /// Clients render the message and treat the code as opaque.
    Other {
        /// Stable machine-readable code.
        code: String,
        /// Human-readable text, already localized by the Host.
        message: String,
    },
}
```

SCALE enums are append-only in practice, so a purely closed set would make every new market condition a protocol release. `Other` gives the operator a namespace to grow into while keeping the common cases typed, which is what clients need in order to branch — offering verification, adjusting an amount, or suggesting a different rail.

Nothing here names a jurisdiction, a tier, or a threshold. `RegionUnavailable` tells a client to stop offering the route; it does not tell it which region or why.

### Serving intents

A provider's worker receives intents addressed to it:

```rust
/// An intent handed to a provider.
struct HostFundingServeSubscribeItem {
    /// Session this intent belongs to.
    intent: FundingIntentId,
    /// Which way value moves.
    direction: FundingDirection,
    /// External side the user picked.
    rail: FundingRail,
    /// Asset being moved.
    asset: FundingAsset,
    /// Amount sought.
    amount: FundingAmount,
    /// The on-chain leg: a target to deliver to, or a request for one.
    target: FundingServeTarget,
}

/// Where a provider stands with respect to the on-chain leg.
///
/// The direction decides who names the target: inbound, the Host mints one and
/// hands it over; outbound, the provider names where the Host should send.
enum FundingServeTarget {
    /// Inbound. Deliver here. Freshly minted for this session.
    DeliverTo(FundingDelivery),
    /// Outbound. Reply with [`HostFundingReportRequest::SettlementTarget`]
    /// naming where the Host should send.
    AwaitingTarget,
}

/// An on-chain target a session moves value to.
///
/// The variant names the mechanism, because paying an ordinary account when a
/// receivable was meant loses the funds.
enum FundingDelivery {
    /// Deposit a cheque against this receivable ([RFC 0017](0017-coinage-payment.md)).
    Receivable {
        /// Receivable public key.
        receivable: CoinPaymentReceivable,
    },
    /// Transfer to this account on the chain the asset lives on.
    Account {
        /// Destination account.
        account: [u8; 32],
        /// Genesis hash of the chain to settle on.
        genesis_hash: [u8; 32],
    },
}

/// A provider's report on a session it is serving.
///
/// Reports move a session forward but never settle it.
enum HostFundingReportRequest {
    /// Outbound: where the Host should send. Answers `AwaitingTarget`.
    SettlementTarget {
        /// Session being reported on.
        intent: FundingIntentId,
        /// Where to send.
        target: FundingDelivery,
    },
    /// Inbound: external-side deposit observed by the provider.
    Deposited {
        /// Session being reported on.
        intent: FundingIntentId,
    },
    /// Value is in transit.
    Bridging {
        /// Session being reported on.
        intent: FundingIntentId,
    },
    /// Inbound: delivery believed complete.
    Sent {
        /// Session being reported on.
        intent: FundingIntentId,
    },
    /// The provider cannot complete this session.
    Failed {
        /// Session being reported on.
        intent: FundingIntentId,
        /// Why it cannot complete.
        reason: FundingFailure,
    },
}
```

The direction decides who names the on-chain target, and getting this backwards is the easiest way to build the wrong thing. **Inbound, the Host mints it** — a fresh receivable or account per session, handed over in `DeliverTo`. **Outbound, the provider names it** — the Host has nothing to mint, because it is the one sending, so the provider answers `AwaitingTarget` with `SettlementTarget` and the Host sends there once the user authorizes.

Minting a fresh inbound target per session limits what a provider learns, and preserves a property a provider may already have when run standalone: a swap app using an ephemeral wallet does not link the user's account to the swap, and being registered as a provider should not silently take that away.

A provider receives no jurisdiction, no verification tier, no user identity, and no `resume` bytes. It gets a direction, a rail, an asset, an amount, and one on-chain target.

`Sent` moves the session no further than `Bridging` on its own. The Host watches the chain and emits `Delivered` when it sees arrival — which is what makes a provider's honesty irrelevant to inbound correctness. Outbound has no equivalent, which is why `Released` claims less.

### Method surface

```rust
/// Funding modality: moving value across the boundary between the user's
/// Polkadot balance and everything outside it, in either direction.
trait Funding: Send + Sync {
    /// Open the funding modality for a direction, asset, and amount.
    #[wire(request_id = 168)]
    async fn request(
        &self,
        cx: &CallContext,
        request: HostFundingRequest,
    ) -> Result<HostFundingResponse, CallError<HostFundingError>>;

    /// Watch a funding session to completion.
    #[wire(start_id = 170)]
    async fn status_subscribe(
        &self,
        cx: &CallContext,
        request: HostFundingStatusSubscribeRequest,
    ) -> Result<
        Subscription<HostFundingStatusSubscribeItem>,
        CallError<HostFundingSessionError>,
    >;

    /// Receive intents addressed to this provider.
    ///
    /// Available only to a product whose manifest declares the funding
    /// modality.
    #[wire(start_id = 174)]
    async fn serve_subscribe(
        &self,
        cx: &CallContext,
    ) -> Result<Subscription<HostFundingServeSubscribeItem>, CallError<HostFundingServeError>>;

    /// Report progress on a session this product is serving.
    #[wire(request_id = 178)]
    async fn report(
        &self,
        cx: &CallContext,
        request: HostFundingReportRequest,
    ) -> Result<HostFundingReportResponse, CallError<HostFundingServingError>>;
}
```

The remaining envelopes:

```rust
/// Request to watch a funding session.
struct HostFundingStatusSubscribeRequest {
    /// Session to watch.
    intent: FundingIntentId,
}

/// Error from [`Funding::request`].
enum HostFundingError {
    /// User is not logged in.
    NotConnected,
    /// The user dismissed the sheet without starting a session.
    Cancelled,
    /// No route serves this direction, asset, and user.
    NoRoute {
        /// Why nothing was available.
        reason: FundingFailure,
    },
    /// The asset is not known to the Host.
    UnknownAsset,
    /// The supplied `resume` context exceeds the 4 KiB bound.
    ResumeTooLarge,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`Funding::status_subscribe`].
enum HostFundingSessionError {
    /// No such session, or it does not belong to the caller.
    NotFound,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`Funding::serve_subscribe`].
enum HostFundingServeError {
    /// The caller's manifest does not declare the funding modality.
    NotAProvider,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`Funding::report`].
enum HostFundingServingError {
    /// No such session, or it is not assigned to this provider.
    NotAssigned,
    /// The session has already reached a terminal state.
    AlreadySettled,
    /// The report does not apply to this session's direction or current state.
    OutOfOrder,
    /// Catch-all.
    Unknown { reason: String },
}
```

`HostFundingReportResponse` is a unit response, declared as an empty versioned wrapper exactly like the existing `HostPaymentTopUpResponse { V1 }`:

```rust
truapi_macros::versioned_type! {
    enum HostFundingReportResponse { V1 }
}
```

`HostFundingResponse` is the only response carrying a payload.

Ids continue the append-only sequence: `Account::sign_vrf` holds 164–165 and the [Secrets RFC][secrets] claims 166–167, so this RFC takes 168–169 (`request`), 170–173 (`status_subscribe`), 174–177 (`serve_subscribe`), and 178–179 (`report`) — twelve ids. The block is contiguous and reserved on merge; if another RFC lands first, these shift up rather than being reused.

### Behavioral requirements

1. A session survives minimize, product reload, and Host restart. On cold relaunch mid-flow the user sees the session, not an empty balance.
2. `Delivered` is emitted only on the Host's own on-chain observation. A provider report never produces it.
3. `Released` is emitted only after the user authorized the release and the Host observed funds leave. It asserts nothing about the off-chain leg, and a Host MUST NOT present it as confirmation that the user was paid.
4. An outbound session debits the user, so the Host collects that authorization itself. A provider never collects it, and never receives it.
5. An inbound delivery target is minted by the Host, fresh per session, and never reused. An outbound settlement target is named by the provider and used for that session only.
6. A provider receives no jurisdiction, verification state, user identity, or `resume` bytes.
7. Eligibility is resolved Host-side. Products receive outcomes and reason codes, never rules, thresholds, or matrices.
8. A route withdrawn while a session is in flight does not orphan the session: it runs to a terminal state, and `RouteWithdrawn` is only for the case where it genuinely cannot.
9. Latency and account requirements are shown on the row before the user commits, in both directions.
10. Partial movement is reported on the terminal item, not collapsed into a bare failure.
11. The Host reclaims the screen when the provider's flow ends, and returns the caller to what it was doing.

## Drawbacks

**Outbound success is unverifiable, and the protocol can only admit it.** `Released` is the honest terminal, but it is not the state a user wants: they want to know the money arrived. Until an escrow or attestation layer exists, every off-ramp ends with the Host saying "we sent it" and the provider being the only party who knows the rest. Stating that plainly is better than a false `Delivered`, but it is a real product gap, not a solved problem.

**A second funding path alongside RFC 0006.** `Payment::top_up` remains the right call when a product already holds the money, and `Payment::request` for moving value between accounts. Three ways to move a balance means explaining which is which; the split is that `top_up` moves value the caller controls, `request` moves it sideways to another account, and `Funding` crosses the boundary to or from outside the network.

**Static route declaration lags reality.** Manifest routes describe capability, not live availability, so a declared route can turn out unavailable at intent time. The reason codes absorb this, but the user may see a route they cannot use.

**`Other` weakens the reason-code contract.** An escape hatch invites overuse, and a client cannot branch on a code it does not know. The alternative — a protocol release per market condition — is worse.

**The provider surface is available to anything that declares the modality.** Declaration is a manifest edit, so route quality is a curation problem the protocol does not solve. What a badge on a provider row asserts, and who is liable for asserting it, is unresolved below.

## Alternatives

**A fourth `PaymentTopUpSource` variant.** Rejected on meaning, not on compatibility. Every existing variant says the caller already controls the funds; funding is the opposite case, and `top_up` has no room for a destination asset, a session, a resume context, or a provider. Compatibility is not the obstacle — [RFC 0021](0021-payment-topup-coins.md) appended `Coins` to this very enum. Appending a variant is safe; reordering existing ones is not, and a peer that predates a new variant cannot decode it, so new variants still need version gating where an older peer may receive one.

**The Host asks providers who can serve a route, at sheet-open time.** Rejected. TrUAPI has exactly one initiator — products call, the Host serves — so there is no host-initiated request to build this on. It would also require launching every installed provider before the list could render. Manifest declaration gets the same information with none of that.

**Eligibility rules in the protocol.** Rejected. Encoding a jurisdiction or verification matrix in a SCALE type makes every market change a protocol version bump, which is exactly the constraint the operator needs to not have.

**Provider completion webhooks as settlement.** Rejected. It would make each provider a trusted party for the one fact the Host is uniquely able to check itself.

**Per-provider verification.** Not chosen here, but not foreclosed: the surface treats verification state as Host-held, so a single reusable verification is expressible without a protocol change. Whether it is owned that way is unresolved.

## Out of Scope

Deliberately excluded, with the reason, so a reviewer sees the boundary rather than infers it.

**Confirming the off-chain leg of an outbound session.** `Released` is where this RFC stops, and a user who sold tokens for cash wants to know the cash arrived. Closing it means one of: escrow, where value is held on-chain until both sides confirm — which is what a peer-to-peer cash network already does and which would make the Host an adjudicator; a provider attestation the Host surfaces as a claim rather than a fact; or a dispute path with a human at the end. Each is a different trust model with different liability, so picking one is a decision about who is accountable, not a protocol detail. It is the most consequential thing left open here.

**A Host-rendered provider fragment.** A third UI level in which a provider supplies a `CustomRendererNode` tree the Host draws natively, so showing a deposit address does not require framing a whole web app. It needs a constrained renderer profile first — an allowed node subset, bounded tree and string sizes, mandatory provider attribution, and a reserved status area the provider cannot draw into — because otherwise a provider can render "Delivered" or "Verified" in Host styling and defeat the ownership rules above. It also needs `CustomRendererNode` re-exported through `truapi::latest`, which today reaches it only via `truapi::v01`. Not needed by the first routes, so not specified here.

**An asset descriptor lookup.** A call resolving a `FundingAsset` to a symbol, decimals, and network for display, letting a product name a destination it was not handed. The Host's own balance card needs the descriptor regardless; it is excluded only because no product does yet.

**A deposit-address watch primitive.** Watching a specific address for an inbound transfer is Host-internal today. If more than one route needs it, it deserves a protocol surface of its own rather than being reimplemented per Host.

**A failure and recovery matrix.** Per route, per failure mode: what the user sees, who is accountable, whether funds are recoverable, and whether a human path exists. The reason codes name outcomes; they assign no owner. This belongs with partner terms rather than in a protocol RFC, but it blocks partner conversations rather than the protocol.

## Unresolved Questions

Recorded rather than answered; the types above are neutral to each answer.

- **Who confirms an outbound off-chain leg, and what happens when it does not complete?** The largest question here, and a liability question before a protocol one. See [Out of Scope](#out-of-scope).
- **Is a privacy-preserving dollar a distinct asset or a mode of the ordinary one?** Decides whether the sheet offers one dollar destination or two.
- **Can a user hold the centralized dollar used to settle between venues, or is it always passed through?** Decides whether it is ever a `FundingAsset`.
- **Where does the operator's jurisdiction and amount matrix live, and how is it versioned?** Required before the modality ships; provider-declared routes do not cover it, since the operator does not own a provider's dotNS name.
- **Is verification Host-held and reused across providers, or per-provider?** Decides whether it is a first-run property or a mid-flow surprise. The [Secrets RFC][secrets] `personhood` tier is the mechanism to evaluate first.
- **What does a verified badge on a provider row assert, and who is liable for asserting it?**
- **Who is accountable for a failed transfer or a stuck swap, and is there a human path?** Needs a per-route matrix with named owners before any partner conversation.
- **Does the Host's own balance card need a resolvable asset descriptor**, and if so does `FundingAsset` opacity survive?

## Prior Art and References

- [**RFC 0006 Payment Host API**](0006-payments.md) — balances, `top_up`, account-to-account payments, payment status. Funding sits beside it and settles through it.
- [**RFC 0017 Coinage Payment**](0017-coinage-payment.md) — purses, receivables, cheques, `listen_for_payment`. Supplies the on-chain side of most routes and the fresh-receivable primitive behind an inbound delivery target.
- [**RFC 0021 Coins variant**](0021-payment-topup-coins.md) — direct coin-key crediting, a landing path for a provider holding coin keys with no on-chain hop.
- [**Product Manifest Format**][manifest] — defines modality and executable, and states that per-modality runtime contracts belong in their own RFCs. This is that contract for funding.
- [**Personhood as a Product**][personhood] — precedent for a modality added to `includes`, and for a registry the Host consults where no caller can supply the answer.
- [**Secrets Management**][secrets] — the credential path for any route whose provider needs a server-side key, with fiat onramp as its motivating example. Funding composes with it rather than duplicating it, and its `personhood` caller tier is a candidate mechanism for reusable verification.
- [**RFC 0002 Permission Model**](0002-permission-model.md) — device and remote permissions; funding adds no new permission, relying on modality consent plus provider-collected consent.

[manifest]: https://github.com/paritytech/truapi/pull/206
[personhood]: https://github.com/paritytech/truapi/pull/324
[secrets]: https://github.com/paritytech/truapi/pull/335
