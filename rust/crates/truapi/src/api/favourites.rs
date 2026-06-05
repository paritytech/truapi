//! Unified [`Favourites`] trait.

use crate::versioned::favourites::{
    HostFavouritesAddError, HostFavouritesAddRequest, HostFavouritesAddResponse,
    HostFavouritesForgetError, HostFavouritesForgetRequest, HostFavouritesSubscribeError,
    HostFavouritesSubscribeItem,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Bookmarked-product catalogue methods.
pub trait Favourites: Send + Sync {
    /// Subscribe to the user's bookmarked products.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const favourites = await firstValueFrom(
    ///   from(truapi.favourites.subscribe()),
    /// );
    /// console.log("favourites:", favourites);
    /// ```
    #[wire(start_id = 164)]
    async fn subscribe(
        &self,
        _cx: &CallContext,
    ) -> Result<
        Subscription<HostFavouritesSubscribeItem>,
        CallError<HostFavouritesSubscribeError>,
    > {
        Err(CallError::unavailable())
    }

    /// Add a product to the favourites catalogue.
    ///
    /// ```ts
    /// const result = await truapi.favourites.add({
    ///   productId: "some-product.dot",
    /// });
    /// result.match(
    ///   (value) => console.log("added:", value),
    ///   (error) => console.error("add failed:", error),
    /// );
    /// ```
    #[wire(request_id = 168)]
    async fn add(
        &self,
        _cx: &CallContext,
        _request: HostFavouritesAddRequest,
    ) -> Result<HostFavouritesAddResponse, CallError<HostFavouritesAddError>> {
        Err(CallError::unavailable())
    }

    /// Remove a product from the favourites catalogue.
    ///
    /// ```ts
    /// const result = await truapi.favourites.forget({
    ///   productId: "some-product.dot",
    /// });
    /// result.match(
    ///   () => console.log("forgotten"),
    ///   (error) => console.error("forget failed:", error),
    /// );
    /// ```
    #[wire(request_id = 170)]
    async fn forget(
        &self,
        _cx: &CallContext,
        _request: HostFavouritesForgetRequest,
    ) -> Result<(), CallError<HostFavouritesForgetError>> {
        Err(CallError::unavailable())
    }
}
