use parity_scale_codec::{Decode, Encode};

use super::coin_payment::{CoinPaymentPurseId, CoinPaymentReceivable};

/// Identifies one funding session.
///
/// Durable across host restart, so a product that reloads reattaches with
/// [`crate::api::Funding::status_subscribe`] rather than starting over.
pub type FundingIntentId = String;

/// Amount in the funded asset's own units.
///
/// Distinct from [`super::payment::Balance`], which is denominated in the
/// host's single fixed payment asset. A funding session names its asset, so its
/// units are not fixed.
pub type FundingAmount = u128;

/// Which way value crosses the boundary between the user's Polkadot balance
/// and everything outside it.
///
/// The two directions do not have equal guarantees. See
/// [`HostFundingStatusSubscribeItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum FundingDirection {
    /// Value moves in. The host confirms arrival by observing the chain.
    In,
    /// Value moves out. The host confirms only that funds left under the
    /// user's authorization; the off-chain leg is the provider's obligation.
    Out,
}

/// A user-visible asset a session moves.
///
/// Opaque to products: an identifier the host resolves against its own asset
/// registry. Products do not construct one.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FundingAsset {
    /// Host-assigned stable asset identifier.
    pub id: [u8; 32],
}

/// The external side of a session, and the primary key of the list the user
/// picks from.
///
/// The host labels the same variant "from" or "to" depending on
/// [`FundingDirection`], which is why this names a rail rather than a source.
/// Keying on the rail rather than on the provider means replacing a provider
/// leaves the user-visible list untouched.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FundingRail {
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

/// An on-chain target a session moves value to.
///
/// The variant names the mechanism, because paying an ordinary account when a
/// receivable was meant loses the funds.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FundingDelivery {
    /// Deposit a cheque against this receivable (RFC 0017).
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

/// Where a provider stands with respect to the on-chain leg.
///
/// The direction decides who names the target: inbound, the host mints one and
/// hands it over; outbound, the provider names where the host should send.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FundingServeTarget {
    /// Inbound. Deliver here. Freshly minted for this session.
    DeliverTo(FundingDelivery),
    /// Outbound. Reply with [`HostFundingReportRequest::SettlementTarget`]
    /// naming where the host should send.
    AwaitingTarget,
}

/// Why a route is not offered, or why a session ended.
///
/// Carries an outcome, never a rule, so a client renders and classifies these
/// without holding any part of the operator's compliance logic.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FundingFailure {
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
    /// Present so an operator introduces an outcome without a wire change.
    /// Clients render the message and treat the code as opaque.
    Other {
        /// Stable machine-readable code.
        code: String,
        /// Human-readable text, already localized by the host.
        message: String,
    },
}

/// Request to open the funding modality.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingRequest {
    /// Which way value moves.
    pub direction: FundingDirection,
    /// Asset to move. `None` lets the host resolve it from `purse`, or asks
    /// the user. Products generally pass `None`.
    pub asset: Option<FundingAsset>,
    /// Amount sought. `None` lets the user choose.
    pub amount: Option<FundingAmount>,
    /// Purse credited on an inbound session, debited on an outbound one.
    /// `None` means `MAIN_PURSE`.
    pub purse: Option<CoinPaymentPurseId>,
    /// Opaque context returned verbatim when the session settles.
    ///
    /// Bounded to 4 KiB, stored under the calling product's identity, returned
    /// only to that product, and discarded with the session. The host attaches
    /// no authority to these bytes.
    pub resume: Option<Vec<u8>>,
}

/// Accepted intent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingResponse {
    /// Identifier for the session.
    pub intent: FundingIntentId,
}

/// Error from [`crate::api::Funding::request`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingError {
    /// User is not logged in.
    NotConnected,
    /// The user dismissed the sheet without starting a session.
    Cancelled,
    /// No route serves this direction, asset, and user.
    NoRoute {
        /// Why nothing was available.
        reason: FundingFailure,
    },
    /// The asset is not known to the host.
    UnknownAsset,
    /// The supplied `resume` context exceeds the 4 KiB bound.
    ResumeTooLarge,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to watch a funding session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingStatusSubscribeRequest {
    /// Session to watch.
    pub intent: FundingIntentId,
}

/// Progress of a funding session, ending in exactly one terminal item.
///
/// The success terminal differs by direction, because the guarantees do.
/// `Delivered` means the host saw the funds arrive on-chain. `Released` means
/// the host saw them leave under the user's authorization, and says nothing
/// about the off-chain leg — no host can verify that cash reached a hand or a
/// bank account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingStatusSubscribeItem {
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
    /// Inbound terminal success. Funds observed on-chain by the host.
    Delivered {
        /// Amount credited, which may differ from the amount requested.
        credited: FundingAmount,
        /// Resume context supplied in the intent.
        resume: Option<Vec<u8>>,
    },
    /// Outbound terminal success. Funds left under the user's authorization.
    ///
    /// The provider now owes the off-chain leg. That obligation is outside
    /// what the host can observe or enforce.
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

/// Error from [`crate::api::Funding::status_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingSessionError {
    /// No such session, or it does not belong to the caller.
    NotFound,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// An intent handed to a provider.
///
/// Carries no jurisdiction, verification state, user identity, or resume
/// context.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingServeSubscribeItem {
    /// Session this intent belongs to.
    pub intent: FundingIntentId,
    /// Which way value moves.
    pub direction: FundingDirection,
    /// External side the user picked.
    pub rail: FundingRail,
    /// Asset being moved.
    pub asset: FundingAsset,
    /// Amount sought.
    pub amount: FundingAmount,
    /// The on-chain leg: a target to deliver to, or a request for one.
    pub target: FundingServeTarget,
}

/// Error from [`crate::api::Funding::serve_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingServeError {
    /// The caller's manifest does not declare the funding modality.
    NotAProvider,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// A provider's report on a session it is serving.
///
/// Reports move a session forward but never settle it. On an inbound session
/// `Sent` tells the host when to look for arrival; the host still decides
/// `Delivered` from its own observation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingReportRequest {
    /// Outbound: where the host should send. Answers
    /// [`FundingServeTarget::AwaitingTarget`].
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

/// Error from [`crate::api::Funding::report`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingServingError {
    /// No such session, or it is not assigned to this provider.
    NotAssigned,
    /// The session has already reached a terminal state.
    AlreadySettled,
    /// The report does not apply to this session's direction or current state.
    OutOfOrder,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
