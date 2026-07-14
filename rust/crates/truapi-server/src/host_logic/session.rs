//! Pairing-host active-session state. The runtime updates this when pairing or
//! unpairing with a signing host changes the inter-host session, and
//! account-management methods read it instead of round-tripping host callbacks
//! on every product call.
//!
//! Host-spec B.1.5 and B.3.1 define the remote account keys and session topics:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L85-L103>
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L119-L131>
//! The persisted blob is core-owned and host-local; storage.md captures current
//! cross-host persistence status quo:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/storage.md?plain=1#L58-L98>

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use std::sync::{Arc, Mutex};

use truapi::v01::HostAccountConnectionStatusSubscribeItem;
use truapi::versioned::account::HostAccountConnectionStatusSubscribeItem as VersionedItem;

/// Session info for a pairing host's active signing-host session. The 32-byte
/// sr25519 public key plus optional usernames are sourced from the signing host
/// and People-chain identity record.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SessionInfo {
    /// 32-byte sr25519 root public key owned by the signing host.
    pub public_key: [u8; 32],
    /// SSO channel state negotiated by this pairing host with the signing host.
    /// Sessions restored from older test fixtures may leave it empty.
    pub sso: Option<SsoSessionInfo>,
    /// Wallet-provided source for deterministic product entropy.
    pub root_entropy_source: Option<[u8; 32]>,
    /// Wallet identity account id used for People-chain username lookup.
    pub identity_account_id: Option<[u8; 32]>,
    /// Short username (e.g. `alice`).
    pub lite_username: Option<String>,
    /// Fully qualified username (e.g. `Alice Smith`).
    pub full_username: Option<String>,
}

impl SessionInfo {
    /// Whether the session already carries a usable username.
    pub(crate) fn has_username(&self) -> bool {
        non_empty_username(&self.full_username) || non_empty_username(&self.lite_username)
    }

    /// Apply resolved username fields without replacing populated values with
    /// empty strings.
    pub(crate) fn apply_usernames(
        &mut self,
        lite_username: Option<String>,
        full_username: Option<String>,
    ) {
        if non_empty_username(&full_username) {
            self.full_username = full_username;
        }
        if non_empty_username(&lite_username) {
            self.lite_username = lite_username;
        }
    }
}

fn non_empty_username(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|value| !value.is_empty())
}

/// SSO session material negotiated by the pairing host with the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SsoSessionInfo {
    /// Pairing host's own 64-byte expanded sr25519 statement-store secret.
    pub ss_secret: [u8; 64],
    /// Pairing host's own session sr25519 statement-store public key.
    pub ss_public_key: [u8; 32],
    /// Pairing host's P-256 ECDH private key.
    pub enc_secret: [u8; 32],
    /// Signing host's persistent P-256 public key.
    pub peer_enc_pubkey: [u8; 65],
    /// Signing host's identity sr25519 account id.
    pub identity_account_id: [u8; 32],
    /// Pairing host -> signing host topic id.
    pub session_id_own: [u8; 32],
    /// Signing host -> pairing host topic id.
    pub session_id_peer: [u8; 32],
    /// Statement channel for pairing-host requests.
    pub request_channel: [u8; 32],
    /// Statement channel for signing-host responses to pairing-host requests.
    pub response_channel: [u8; 32],
    /// Statement channel for signing-host initiated requests.
    pub peer_request_channel: [u8; 32],
}

/// Encode the active-session fields the core currently understands into an
/// opaque host-global session blob.
pub fn encode_persisted_session(info: &SessionInfo) -> Vec<u8> {
    info.encode()
}

/// Decode a core-owned persisted session blob.
pub fn decode_persisted_session(blob: &[u8]) -> Result<SessionInfo, String> {
    let mut input = blob;
    let decoded =
        SessionInfo::decode(&mut input).map_err(|err| format!("invalid session blob: {err}"))?;
    if !input.is_empty() {
        return Err("invalid session blob: trailing bytes".to_string());
    }
    Ok(decoded)
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
    /// every live subscriber if this is a transition from no-session or an
    /// actual session replacement.
    pub fn set_session(&self, info: SessionInfo) {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        let should_broadcast = inner.current.as_ref() != Some(&info);
        inner.current = Some(info);
        if should_broadcast {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Connected,
            );
        }
    }

    /// Replace the active session only when it still matches `expected`.
    pub fn replace_session_if_current(&self, expected: &SessionInfo, info: SessionInfo) -> bool {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        if inner.current.as_ref() != Some(expected) {
            return false;
        }

        let should_broadcast = inner.current.as_ref() != Some(&info);
        inner.current = Some(info);
        if should_broadcast {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Connected,
            );
        }
        true
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

/// Broadcast one connection-status transition and prune dropped subscribers.
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
            sso: None,
            root_entropy_source: None,
            identity_account_id: None,
            lite_username: Some("alice".to_string()),
            full_username: None,
        }
    }

    #[test]
    fn session_username_helpers_check_and_apply_non_empty_values() {
        let mut session = info(0x42);
        session.lite_username = None;
        session.full_username = None;

        assert!(!session.has_username());

        session.apply_usernames(Some(String::new()), Some("Alice Smith".to_string()));
        assert!(session.has_username());
        assert_eq!(session.full_username.as_deref(), Some("Alice Smith"));
        assert_eq!(session.lite_username, None);

        session.apply_usernames(Some("alice".to_string()), Some(String::new()));
        assert_eq!(session.full_username.as_deref(), Some("Alice Smith"));
        assert_eq!(session.lite_username.as_deref(), Some("alice"));
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
    fn replace_session_if_current_rejects_stale_expected_session() {
        let state = SessionState::new();
        let original = info(0x01);
        let replacement = info(0x02);
        state.set_session(original.clone());
        state.set_session(replacement.clone());

        assert!(!state.replace_session_if_current(&original, info(0x03)));
        assert_eq!(state.current(), Some(replacement));
    }

    #[test]
    fn replace_session_if_current_updates_matching_session() {
        let state = SessionState::new();
        let original = info(0x01);
        let replacement = info(0x02);
        state.set_session(original.clone());

        assert!(state.replace_session_if_current(&original, replacement.clone()));

        assert_eq!(state.current(), Some(replacement));
    }

    #[test]
    fn persisted_session_round_trips() {
        let mut session = info(0x42);
        session.root_entropy_source = Some([1; 32]);
        session.full_username = Some("Alice Smith".to_string());

        let blob = encode_persisted_session(&session);
        let decoded = decode_persisted_session(&blob).expect("session should decode");

        assert_eq!(decoded, session);
    }

    #[test]
    fn persisted_sso_session_round_trips() {
        let mut session = info(0x42);
        session.sso = Some(SsoSessionInfo {
            ss_secret: [1; 64],
            ss_public_key: [2; 32],
            enc_secret: [3; 32],
            peer_enc_pubkey: [4; 65],
            identity_account_id: [5; 32],
            session_id_own: [6; 32],
            session_id_peer: [7; 32],
            request_channel: [8; 32],
            response_channel: [9; 32],
            peer_request_channel: [10; 32],
        });

        let blob = encode_persisted_session(&session);
        let decoded = decode_persisted_session(&blob).expect("session should decode");

        assert_eq!(decoded, session);
    }

    #[test]
    fn persisted_session_rejects_trailing_bytes() {
        let mut blob = encode_persisted_session(&info(0x42));
        blob.push(0);

        let err = decode_persisted_session(&blob).unwrap_err();

        assert_eq!(err, "invalid session blob: trailing bytes");
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
    fn set_session_with_same_info_does_not_re_emit_connected() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x01));

        let pending = stream.next().now_or_never();
        assert!(
            pending.is_none(),
            "no transition event expected for equivalent session"
        );
    }

    #[test]
    fn set_session_with_replacement_re_emits_connected() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x02));

        let next = block_on(stream.next()).expect("expected replacement Connected event");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
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
