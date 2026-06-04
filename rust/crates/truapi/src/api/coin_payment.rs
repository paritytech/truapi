//! Unified [`CoinPayment`] trait (RFC 0017).

use crate::versioned::coin_payment::{
    HostCoinPaymentCreateChequeError, HostCoinPaymentCreateChequeRequest,
    HostCoinPaymentCreateChequeResponse, HostCoinPaymentCreatePurseError,
    HostCoinPaymentCreatePurseRequest, HostCoinPaymentCreatePurseResponse,
    HostCoinPaymentCreateReceivableError, HostCoinPaymentCreateReceivableRequest,
    HostCoinPaymentCreateReceivableResponse, HostCoinPaymentDeletePurseError,
    HostCoinPaymentDeletePurseItem, HostCoinPaymentDeletePurseRequest, HostCoinPaymentDepositError,
    HostCoinPaymentDepositItem, HostCoinPaymentDepositRequest, HostCoinPaymentListenForError,
    HostCoinPaymentListenForItem, HostCoinPaymentListenForRequest, HostCoinPaymentQueryPurseError,
    HostCoinPaymentQueryPurseRequest, HostCoinPaymentQueryPurseResponse,
    HostCoinPaymentRebalancePurseError, HostCoinPaymentRebalancePurseItem,
    HostCoinPaymentRebalancePurseRequest, HostCoinPaymentRefundError, HostCoinPaymentRefundItem,
    HostCoinPaymentRefundRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// CoinPayment operations.
///
/// RFC 0017 describes `Resolvable<T>` values for long-running operations.
/// TrUAPI represents those as subscriptions whose items are the RFC status
/// updates.
pub trait CoinPayment: Send + Sync {
    /// Create a new firewalled CoinPayment purse.
    ///
    /// ```ts
    /// const result = await truapi.coinPayment.createPurse({
    ///   name: "Terminal purse",
    /// });
    /// assert(result.isOk(), "createPurse failed:", result);
    /// console.log("purse created:", result.value.purse);
    /// ```
    #[wire(request_id = 136)]
    async fn create_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreatePurseRequest,
    ) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>>
    {
        Err(CallError::unavailable())
    }

    /// Query product-visible purse metadata and balance.
    ///
    /// ```ts
    /// const result = await truapi.coinPayment.queryPurse({ purse: 1 });
    /// assert(result.isOk(), "queryPurse failed:", result);
    /// console.log("purse info:", result.value.info);
    /// ```
    #[wire(request_id = 138)]
    async fn query_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentQueryPurseRequest,
    ) -> Result<HostCoinPaymentQueryPurseResponse, CallError<HostCoinPaymentQueryPurseError>> {
        Err(CallError::unavailable())
    }

    /// Transfer balance between local purses.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.coinPayment.rebalancePurse({
    ///       request: { from: 1, to: 2, amount: 1000 },
    ///     }),
    ///   ),
    /// );
    /// console.log("rebalance status:", status);
    /// ```
    #[wire(start_id = 140)]
    async fn rebalance_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentRebalancePurseRequest,
    ) -> Result<
        Subscription<HostCoinPaymentRebalancePurseItem>,
        CallError<HostCoinPaymentRebalancePurseError>,
    > {
        Err(CallError::unavailable())
    }

    /// Delete a purse after draining its balance into another local purse.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.coinPayment.deletePurse({
    ///       request: { target: 2, drainInto: 1 },
    ///     }),
    ///   ),
    /// );
    /// console.log("delete status:", status);
    /// ```
    #[wire(start_id = 144)]
    async fn delete_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentDeletePurseRequest,
    ) -> Result<
        Subscription<HostCoinPaymentDeletePurseItem>,
        CallError<HostCoinPaymentDeletePurseError>,
    > {
        Err(CallError::unavailable())
    }

    /// Create a receivable public key for depositing into a purse.
    ///
    /// ```ts
    /// const result = await truapi.coinPayment.createReceivable({ into: 1 });
    /// assert(result.isOk(), "createReceivable failed:", result);
    /// console.log("receivable created:", result.value.receivable);
    /// ```
    #[wire(request_id = 148)]
    async fn create_receivable(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreateReceivableRequest,
    ) -> Result<
        HostCoinPaymentCreateReceivableResponse,
        CallError<HostCoinPaymentCreateReceivableError>,
    > {
        Err(CallError::unavailable())
    }

    /// Create a cheque paying from a local purse to a receivable.
    ///
    /// ```ts
    /// const result = await truapi.coinPayment.createCheque({
    ///   from: 1,
    ///   to: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///   amount: 1000,
    /// });
    /// assert(result.isOk(), "createCheque failed:", result);
    /// console.log("cheque created:", result.value.cheque);
    /// ```
    #[wire(request_id = 150)]
    async fn create_cheque(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreateChequeRequest,
    ) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>>
    {
        Err(CallError::unavailable())
    }

    /// Claim coins from a cheque into the receivable's purse.
    ///
    /// ```ts
    /// import { type CoinPaymentCheque } from "@parity/truapi";
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const cheque: CoinPaymentCheque = {
    ///   id: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///   amount: 1000,
    ///   encryptedSecrets: "0x",
    /// };
    ///
    /// const status = await firstValueFrom(
    ///   from(truapi.coinPayment.deposit({ request: { cheque } })),
    /// );
    /// console.log("deposit status:", status);
    /// ```
    #[wire(start_id = 152)]
    async fn deposit(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentDepositRequest,
    ) -> Result<Subscription<HostCoinPaymentDepositItem>, CallError<HostCoinPaymentDepositError>>
    {
        Err(CallError::unavailable())
    }

    /// Attempt to return coins associated with a receivable.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.coinPayment.refund({
    ///       request: {
    ///         receivable:
    ///           "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///       },
    ///     }),
    ///   ),
    /// );
    /// console.log("refund status:", status);
    /// ```
    #[wire(start_id = 156)]
    async fn refund(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentRefundRequest,
    ) -> Result<Subscription<HostCoinPaymentRefundItem>, CallError<HostCoinPaymentRefundError>>
    {
        Err(CallError::unavailable())
    }

    /// Listen for a cheque delivered through a standard transmission channel.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const item = await firstValueFrom(
    ///   from(
    ///     truapi.coinPayment.listenForPayment({
    ///       request: {
    ///         receivable:
    ///           "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///       },
    ///     }),
    ///   ),
    /// );
    /// console.log("payment received:", item);
    /// ```
    #[wire(start_id = 160)]
    async fn listen_for_payment(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentListenForRequest,
    ) -> Result<Subscription<HostCoinPaymentListenForItem>, CallError<HostCoinPaymentListenForError>>
    {
        Err(CallError::unavailable())
    }
}
