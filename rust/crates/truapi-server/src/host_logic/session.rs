//! Active-session state held in core. The host pushes session info via
//! platform-specific entrypoints whenever the user pairs/unpairs.
//! Account-management methods then read from this state instead of
//! round-tripping a callback to the host on every product call.

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use std::sync::{Arc, Mutex};

use truapi::v01::HostAccountConnectionStatusSubscribeItem;
use truapi::versioned::account::HostAccountConnectionStatusSubscribeItem as VersionedItem;

/// Session info pushed by the host. The 32-byte sr25519 public key plus
/// optional usernames sourced from the People-Chain identity record.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 32-byte sr25519 root public key of the paired session.
    pub public_key: [u8; 32],
    /// Short username (e.g. `alice`).
    pub lite_username: Option<String>,
    /// Fully qualified username (e.g. `Alice Smith`).
    pub full_username: Option<String>,
}

/// Holds the currently-active session and broadcasts connection-status
/// transitions to subscribers. Cheap to clone via `Arc`.
#[derive(Default)]
pub struct SessionState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    current: Option<SessionInfo>,
    subscribers: Vec<mpsc::UnboundedSender<VersionedItem>>,
}

impl SessionState {
    /// Construct a fresh session holder, starting in the `Disconnected` state.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Replace the active session with `info`. Emits a `Connected` event to
    /// every live subscriber if this is a transition from no-session.
    pub fn set_session(&self, info: SessionInfo) {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        let was_present = inner.current.is_some();
        inner.current = Some(info);
        if !was_present {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Connected,
            );
        }
    }

    /// Drop the active session. Emits a `Disconnected` event to every live
    /// subscriber if there was a session to clear.
    pub fn clear_session(&self) {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        if inner.current.take().is_some() {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Disconnected,
            );
        }
    }

    /// Snapshot of the current session, or `None` when nothing is paired.
    pub fn current(&self) -> Option<SessionInfo> {
        self.inner
            .lock()
            .expect("session-state mutex poisoned")
            .current
            .clone()
    }

    /// Stream of connection-status events. The first item emitted is the
    /// current state (so subscribers don't have to read it separately);
    /// subsequent items reflect every `set_session` / `clear_session`
    /// transition.
    pub fn subscribe(&self) -> BoxStream<'static, VersionedItem> {
        let (tx, rx) = mpsc::unbounded();
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        let initial = match inner.current {
            Some(_) => HostAccountConnectionStatusSubscribeItem::Connected,
            None => HostAccountConnectionStatusSubscribeItem::Disconnected,
        };
        inner.subscribers.push(tx);
        let initial_item = VersionedItem::V1(initial);
        Box::pin(stream::once(async move { initial_item }).chain(rx))
    }
}

fn broadcast(
    subscribers: &mut Vec<mpsc::UnboundedSender<VersionedItem>>,
    status: HostAccountConnectionStatusSubscribeItem,
) {
    let item = VersionedItem::V1(status);
    // `retain` drops senders whose receiver has been dropped, so the
    // subscriber list self-prunes on the next broadcast after a reader
    // unsubscribes.
    subscribers.retain(|tx| tx.unbounded_send(item.clone()).is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::{FutureExt, StreamExt};

    fn info(pubkey_byte: u8) -> SessionInfo {
        SessionInfo {
            public_key: [pubkey_byte; 32],
            lite_username: Some("alice".to_string()),
            full_username: None,
        }
    }

    #[test]
    fn current_starts_empty() {
        let state = SessionState::new();
        assert!(state.current().is_none());
    }

    #[test]
    fn set_then_current_returns_session() {
        let state = SessionState::new();
        state.set_session(info(0x42));
        let got = state.current().expect("session should be present");
        assert_eq!(got.public_key, [0x42; 32]);
        assert_eq!(got.lite_username.as_deref(), Some("alice"));
    }

    #[test]
    fn clear_returns_to_empty() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        state.clear_session();
        assert!(state.current().is_none());
    }

    #[test]
    fn subscribe_emits_current_state_first() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let first = block_on(stream.next()).expect("expected initial item");
        assert_eq!(
            first,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
    }

    #[test]
    fn subscribe_emits_disconnected_when_no_session() {
        let state = SessionState::new();
        let mut stream = state.subscribe();
        let first = block_on(stream.next()).expect("expected initial item");
        assert_eq!(
            first,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Disconnected)
        );
    }

    #[test]
    fn set_session_broadcasts_connected_to_existing_subscribers() {
        let state = SessionState::new();
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x01));
        let next = block_on(stream.next()).expect("expected Connected event");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
    }

    #[test]
    fn clear_session_broadcasts_disconnected_to_existing_subscribers() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.clear_session();
        let next = block_on(stream.next()).expect("expected Disconnected event");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Disconnected)
        );
    }

    #[test]
    fn set_session_twice_does_not_re_emit_connected() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x02));

        let pending = stream.next().now_or_never();
        assert!(
            pending.is_none(),
            "no transition event expected on session replace"
        );
    }

    #[test]
    fn multi_subscriber_broadcast() {
        let state = SessionState::new();
        let mut a = state.subscribe();
        let mut b = state.subscribe();
        // Drain initial Disconnected from both.
        let _ = block_on(a.next());
        let _ = block_on(b.next());

        state.set_session(info(0x77));
        let a_next = block_on(a.next()).expect("a should receive Connected");
        let b_next = block_on(b.next()).expect("b should receive Connected");
        assert_eq!(
            a_next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
        assert_eq!(
            b_next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
    }
}
