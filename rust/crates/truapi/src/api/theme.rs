//! Unified [`Theme`] trait.

use crate::versioned::theme::HostThemeSubscribeItem;
use crate::wire;
use crate::{CallContext, Subscription};

/// Host theme subscription.
pub trait Theme: Send + Sync {
    /// Subscribe to host theme changes.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const theme = await firstValueFrom(
    ///   from(truapi.theme.subscribe()),
    /// );
    /// console.log(theme);
    /// ```
    #[wire(start_id = 104)]
    async fn subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        Subscription::empty()
    }
}
