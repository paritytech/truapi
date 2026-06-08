//! Active-session state held in core. The host pushes session info via
//! platform-specific entrypoints whenever the user pairs/unpairs.
//! Account-management methods then read from this state instead of
//! round-tripping a callback to the host on every product call.

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use std::sync::{Arc, Mutex};

use truapi::v01::HostAccountConnectionStatusSubscribeItem;
use truapi::versioned::account::HostAccountConnectionStatusSubscribeItem as VersionedItem;

/// Session info pushed by the host. The 32-byte sr25519 public key plus
/// optional usernames sourced from the People-Chain identity record.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SessionInfo {
    /// 32-byte sr25519 root public key of the paired session.
    pub public_key: [u8; 32],
    /// Current dotli entropy source: host-papp session `ssSecret` bytes.
    /// This is optional while transitional bridge code still pushes only
    /// identity/account state.
    pub entropy_secret: Option<Vec<u8>>,
    /// Short username (e.g. `alice`).
    pub lite_username: Option<String>,
    /// Fully qualified username (e.g. `Alice Smith`).
    pub full_username: Option<String>,
}

const PERSISTED_SESSION_VERSION: u8 = 1;

/// Encode the active-session fields the core currently understands into an
/// opaque host-global session blob. Later SSO channel state should bump
/// `PERSISTED_SESSION_VERSION` instead of extending this layout silently.
pub fn encode_persisted_session(info: &SessionInfo) -> Vec<u8> {
    (PERSISTED_SESSION_VERSION, info).encode()
}

/// Decode a core-owned persisted session blob.
pub fn decode_persisted_session(blob: &[u8]) -> Result<SessionInfo, String> {
    let mut input = blob;
    let (version, info): (u8, SessionInfo) =
        Decode::decode(&mut input).map_err(|err| format!("invalid session blob: {err}"))?;
    if version != PERSISTED_SESSION_VERSION {
        return Err(format!("unsupported session blob version {version}"));
    }
    if !input.is_empty() {
        return Err("invalid session blob: trailing bytes".to_string());
    }
    Ok(info)
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

    /// Attach or replace the current session's entropy secret. Returns false
    /// if there is no active session to update.
    pub fn set_entropy_secret(&self, secret: Vec<u8>) -> bool {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        let Some(current) = inner.current.as_mut() else {
            return false;
        };
        current.entropy_secret = Some(secret);
        true
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
            entropy_secret: None,
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
    fn persisted_session_round_trips() {
        let mut session = info(0x42);
        session.entropy_secret = Some(vec![1, 2, 3]);
        session.full_username = Some("Alice Smith".to_string());

        let blob = encode_persisted_session(&session);
        let decoded = decode_persisted_session(&blob).expect("session should decode");

        assert_eq!(decoded, session);
    }

    #[test]
    fn persisted_session_rejects_unknown_version() {
        let mut blob = encode_persisted_session(&info(0x42));
        blob[0] = 0xff;

        let err = decode_persisted_session(&blob).unwrap_err();

        assert_eq!(err, "unsupported session blob version 255");
    }

    #[test]
    fn persisted_session_rejects_trailing_bytes() {
        let mut blob = encode_persisted_session(&info(0x42));
        blob.push(0);

        let err = decode_persisted_session(&blob).unwrap_err();

        assert_eq!(err, "invalid session blob: trailing bytes");
    }

    #[test]
    fn set_entropy_secret_updates_current_session() {
        let state = SessionState::new();
        state.set_session(info(0x42));
        assert!(state.set_entropy_secret(vec![1, 2, 3]));
        let got = state.current().expect("session should be present");
        assert_eq!(got.entropy_secret.as_deref(), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn set_entropy_secret_without_session_returns_false() {
        let state = SessionState::new();
        assert!(!state.set_entropy_secret(vec![1, 2, 3]));
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

    /// Clearing a never-set session is a no-op and must not synthesize a
    /// spurious `Disconnected` event for live subscribers.
    #[test]
    fn clear_when_empty_is_silent_no_op() {
        let state = SessionState::new();
        let mut stream = state.subscribe();
        // Drain the initial Disconnected.
        let _ = block_on(stream.next());

        state.clear_session();

        let pending = stream.next().now_or_never();
        assert!(pending.is_none(), "no event expected when clear is a no-op",);
    }

    /// Dropping a subscriber's stream must remove that sender from the
    /// broadcast list. The next broadcast prunes it; the surviving stream
    /// still receives the event.
    #[test]
    fn dropped_subscriber_is_pruned() {
        let state = SessionState::new();
        let mut survivor = state.subscribe();
        let dropping = state.subscribe();
        let _ = block_on(survivor.next());
        // Drain the initial item from the dropping stream too so we don't
        // accidentally test buffered-but-undelivered.
        drop(dropping);

        state.set_session(info(0x33));
        let next = block_on(survivor.next()).expect("survivor must receive Connected");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected),
        );

        // Internally, `set_session`'s broadcast call `retain`-prunes any
        // dropped senders. After the call the subscribers list should have
        // exactly one entry (the survivor).
        let inner = state.inner.lock().unwrap();
        assert_eq!(inner.subscribers.len(), 1, "dropped subscriber not pruned");
    }
}
