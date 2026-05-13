//! Unified [`Theme`] trait.

use crate::versioned::theme::HostThemeSubscribeItem;
use crate::wire;
use crate::{CallContext, Subscription};

/// Host theme subscription.
#[async_trait::async_trait]
pub trait Theme: Send + Sync {
    /// Subscribe to host theme changes.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostThemeSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchTheme(truapi: Client): Subscription {
    ///   return truapi.theme.subscribe().subscribe({
    ///     next: (theme: HostThemeSubscribeItem) => console.log(theme),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 104)]
    async fn subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        Subscription::empty()
    }
}
