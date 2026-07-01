//! Core-side invalidation signal for host-global session storage.
//!
//! The host owns persistence; the core owns decoding and projecting the
//! current blob into `SessionState` and `AuthState`. This notifier is just the
//! "the backing store may have changed" signal that drives a re-read.

use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};

#[derive(Default)]
pub struct SessionStoreChangeNotifier {
    subscribers: Mutex<Vec<mpsc::UnboundedSender<()>>>,
}

impl SessionStoreChangeNotifier {
    /// Create a notifier with no subscribers.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Broadcast a storage-change tick to current subscribers.
    pub fn notify(&self) {
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("session-store notifier mutex poisoned");
        subscribers.retain(|tx| tx.unbounded_send(()).is_ok());
    }

    /// Subscribe to storage-change ticks, including one initial tick.
    pub fn subscribe(&self) -> BoxStream<'static, ()> {
        let (tx, rx) = mpsc::unbounded();
        self.subscribers
            .lock()
            .expect("session-store notifier mutex poisoned")
            .push(tx);
        Box::pin(stream::once(async {}).chain(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::{FutureExt, StreamExt};

    #[test]
    fn subscribe_emits_initial_tick() {
        let notifier = SessionStoreChangeNotifier::new();
        let mut ticks = notifier.subscribe();

        assert!(block_on(ticks.next()).is_some());
    }

    #[test]
    fn notify_broadcasts_to_subscribers() {
        let notifier = SessionStoreChangeNotifier::new();
        let mut first = notifier.subscribe();
        let mut second = notifier.subscribe();
        let _ = block_on(first.next());
        let _ = block_on(second.next());

        notifier.notify();

        assert!(block_on(first.next()).is_some());
        assert!(block_on(second.next()).is_some());
    }

    #[test]
    fn dropped_subscriber_is_pruned_on_next_notify() {
        let notifier = SessionStoreChangeNotifier::new();
        let dropped = notifier.subscribe();
        drop(dropped);

        notifier.notify();

        assert_eq!(
            notifier
                .subscribers
                .lock()
                .expect("session-store notifier mutex poisoned")
                .len(),
            0
        );
    }

    #[test]
    fn no_tick_without_notify_after_initial() {
        let notifier = SessionStoreChangeNotifier::new();
        let mut ticks = notifier.subscribe();
        let _ = block_on(ticks.next());

        assert!(ticks.next().now_or_never().is_none());
    }
}
