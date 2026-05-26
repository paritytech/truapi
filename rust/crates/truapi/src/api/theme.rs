//! Unified [`Theme`] trait.

use crate::versioned::theme::HostThemeSubscribeItem;
use crate::wire;
use crate::{CallContext, Subscription};

/// Host theme subscription.
pub trait Theme: Send + Sync {
    /// Subscribe to host theme changes.
    ///
    /// ```ts
    /// import { from, take } from "rxjs";
    ///
    /// from(truapi.theme.subscribe())
    ///   .pipe(take(3))
    ///   .subscribe({
    ///     next: (theme) => console.log(theme),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// ```
    #[wire(start_id = 104)]
    async fn subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        Subscription::empty()
    }
}
