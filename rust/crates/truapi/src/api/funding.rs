//! Unified [`Funding`] trait.

use crate::versioned::funding::{
    HostFundingError, HostFundingReportRequest, HostFundingReportResponse, HostFundingRequest,
    HostFundingResponse, HostFundingServeError, HostFundingServeSubscribeItem,
    HostFundingServingError, HostFundingSessionError, HostFundingStatusSubscribeItem,
    HostFundingStatusSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Funding modality: moving value across the boundary between the user's
/// Polkadot balance and everything outside it, in either direction.
///
/// A consumer declares an intent and watches it to completion. A provider whose
/// manifest declares the funding modality receives intents addressed to it and
/// reports progress. Reports never settle a session: the host decides
/// `Delivered` from its own on-chain observation, and `Released` from the user's
/// authorization to send.
#[crate::async_trait]
pub trait Funding: Send + Sync {
    /// Open the funding modality for a direction, asset, and amount.
    ///
    /// A product that ran out of balance mid-action passes the purse it was
    /// spending from and leaves `asset` unset; the host resolves it. `resume`
    /// is returned verbatim when the session settles.
    ///
    /// ```ts
    /// const result = await truapi.funding.request({
    ///   direction: "In",
    ///   asset: undefined,
    ///   amount: 1000n,
    ///   purse: undefined,
    ///   resume: undefined,
    /// });
    /// assert(result.isOk(), "funding.request failed:", result);
    /// console.log("funding intent:", result.value.intent);
    /// ```
    #[wire(request_id = 168)]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostFundingRequest,
    ) -> Result<HostFundingResponse, CallError<HostFundingError>> {
        Err(CallError::unavailable())
    }

    /// Watch a funding session to completion.
    ///
    /// Emits the stamp-bar states, then exactly one terminal item carrying the
    /// amount moved and the resume context. The success terminal is `Delivered`
    /// inbound and `Released` outbound, because the guarantees differ.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const requested = await truapi.funding.request({
    ///   direction: "In",
    ///   asset: undefined,
    ///   amount: 1000n,
    ///   purse: undefined,
    ///   resume: undefined,
    /// });
    /// assert(requested.isOk(), "funding.request failed:", requested);
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.funding.statusSubscribe({
    ///       request: { intent: requested.value.intent },
    ///     }),
    ///   ),
    /// );
    /// console.log("funding status received:", status);
    /// ```
    #[wire(start_id = 170)]
    async fn status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostFundingStatusSubscribeRequest,
    ) -> Result<Subscription<HostFundingStatusSubscribeItem>, CallError<HostFundingSessionError>>
    {
        Err(CallError::unavailable())
    }

    /// Receive intents addressed to this provider.
    ///
    /// Available only to a product whose manifest declares the funding
    /// modality. Each item carries the direction, the rail, and either a
    /// host-minted delivery target or a request for one — and nothing about
    /// the user.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const intent = await firstValueFrom(
    ///   from(truapi.funding.serveSubscribe()),
    /// );
    /// console.log("intent to serve:", intent);
    /// ```
    #[wire(start_id = 174)]
    async fn serve_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Result<Subscription<HostFundingServeSubscribeItem>, CallError<HostFundingServeError>> {
        Err(CallError::unavailable())
    }

    /// Report progress on a session this product is serving.
    ///
    /// `Sent` tells the host when to look for arrival; it does not itself
    /// settle the session. On an outbound session the provider answers with
    /// `SettlementTarget` before anything else can happen.
    ///
    /// ```ts
    /// const result = await truapi.funding.report({
    ///   tag: "Sent",
    ///   value: { intent: "funding-session-1" },
    /// });
    /// assert(result.isOk(), "funding.report failed:", result);
    /// console.log("progress reported");
    /// ```
    #[wire(request_id = 178)]
    async fn report(
        &self,
        _cx: &CallContext,
        _request: HostFundingReportRequest,
    ) -> Result<HostFundingReportResponse, CallError<HostFundingServingError>> {
        Err(CallError::unavailable())
    }
}
