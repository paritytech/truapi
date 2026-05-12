//! Unified [`HostTheme`] trait.

use crate::versioned::theme::HostThemeSubscribeItem;
use crate::wire;
use crate::{CallContext, Subscription};

/// Host UI theme subscription.
///
/// The default body returns an empty stream; hosts override to push theme
/// updates.
#[async_trait::async_trait]
pub trait HostTheme: Send + Sync {
    /// Subscribe to host theme changes (light/dark).
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostThemeSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchTheme(truapi: Client): Subscription {
    ///   return truapi.hostTheme.themeSubscribe().subscribe({
    ///     next: (theme: HostThemeSubscribeItem) => console.log(theme),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 104)]
    async fn host_theme_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostThemeSubscribeItem> {
        Subscription::empty()
    }
}
