//! Core-owned auth/session UI state machine. Every [`AuthState`] emission to
//! the host funnels through [`AuthStateMachine`], so transitions stay ordered
//! and a stale session-store tick can never tear down an in-flight pairing.

use std::sync::{Arc, Mutex};

use futures::channel::oneshot;
use truapi_platform::{AuthPresenter, AuthState, Platform, SessionUiInfo};

/// Serialized auth-state machine bound to the platform's `auth_state_changed`
/// sink. Each transition mutates under the lock, releases it, then emits the
/// new state (when it actually changed), so `auth_state_changed` handlers may
/// safely re-enter the runtime (e.g. a host cancelling the login it just
/// observed). The cancel channel for an in-flight login lives inside the
/// `Pairing` state, making its registration atomic with the transition.
pub(crate) struct AuthStateMachine {
    platform: Arc<dyn Platform>,
    inner: Arc<Mutex<AuthStateInner>>,
}

impl Clone for AuthStateMachine {
    fn clone(&self) -> Self {
        Self {
            platform: self.platform.clone(),
            inner: self.inner.clone(),
        }
    }
}

#[derive(Default)]
struct AuthStateInner {
    state: AuthState,
    /// Increments on every `pairing_started`; lets an abandoned flow's reset
    /// guard distinguish its own `Pairing` from a newer flow's.
    pairing_epoch: u64,
    /// Resolves the in-flight login's cancel receiver. Present exactly while
    /// the state is `Pairing`.
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl AuthStateMachine {
    /// Create an auth state machine that reports transitions to `platform`.
    pub(super) fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            inner: Arc::new(Mutex::new(AuthStateInner::default())),
        }
    }

    /// Enter `Pairing`. Returns the cancel receiver and the pairing epoch, or
    /// `None` when a pairing is already in flight (single-flight guard).
    pub(super) fn pairing_started(&self, deeplink: String) -> Option<(oneshot::Receiver<()>, u64)> {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let epoch = self.transition(|inner| {
            if matches!(inner.state, AuthState::Pairing { .. }) {
                return None;
            }
            inner.state = AuthState::Pairing { deeplink };
            inner.pairing_epoch = inner.pairing_epoch.wrapping_add(1);
            inner.cancel_tx = Some(cancel_tx);
            Some(inner.pairing_epoch)
        })?;
        Some((cancel_rx, epoch))
    }

    /// `Pairing` -> `LoginFailed`: the in-flight login reported a failure.
    pub(super) fn login_failed(&self, reason: String) {
        self.transition(|inner| {
            if !matches!(inner.state, AuthState::Pairing { .. }) {
                return None;
            }
            inner.cancel_tx = None;
            inner.state = AuthState::LoginFailed { reason };
            Some(())
        });
    }

    /// `Disconnected`/`LoginFailed` -> `LoginFailed`: a login failed before
    /// it reached `Pairing` (device identity or bootstrap errors). A no-op
    /// while `Pairing`, so a concurrent second login attempt failing early
    /// cannot tear down the first one's presentation.
    pub(super) fn login_failed_before_pairing(&self, reason: String) {
        self.transition(|inner| {
            if matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Connected(_)
            ) {
                return None;
            }
            inner.state = AuthState::LoginFailed { reason };
            Some(())
        });
    }

    /// `Pairing`/`LoginFailed` -> `Disconnected` (host cancelled or
    /// dismissed). Wakes the in-flight login, which resolves as `Rejected`.
    pub(super) fn login_cancelled(&self) {
        self.transition(|inner| {
            if !matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::LoginFailed { .. }
            ) {
                return None;
            }
            if let Some(cancel_tx) = inner.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
            inner.state = AuthState::Disconnected;
            Some(())
        });
    }

    /// Any state -> `Connected`. A login in flight is cancelled: another
    /// runtime won the race, and the waking flow resolves as
    /// `AlreadyConnected`. Emits only when the connected info changed.
    pub(super) fn connected(&self, info: &SessionUiInfo) {
        self.transition(|inner| {
            if let Some(cancel_tx) = inner.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
            if matches!(&inner.state, AuthState::Connected(current) if current == info) {
                return None;
            }
            inner.state = AuthState::Connected(info.clone());
            Some(())
        });
    }

    /// Session store reports no session. A no-op while `Pairing`: the login
    /// flow owns its own terminal transition, and a boot-time store tick must
    /// not tear down the pairing UI.
    pub(super) fn store_disconnected(&self) {
        self.transition(|inner| {
            if matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Disconnected
            ) {
                return None;
            }
            inner.state = AuthState::Disconnected;
            Some(())
        });
    }

    /// Reset a `Pairing` left behind by a dropped login future, but only when
    /// it still belongs to `epoch` (a newer flow's pairing is left alone).
    pub(super) fn reset_abandoned_pairing(&self, epoch: u64) {
        self.transition(|inner| {
            if !matches!(inner.state, AuthState::Pairing { .. }) || inner.pairing_epoch != epoch {
                return None;
            }
            inner.cancel_tx = None;
            inner.state = AuthState::Disconnected;
            Some(())
        });
    }

    /// Run `apply` under the lock; when it changed the state (returned
    /// `Some`), emit the new state to the host after releasing the lock.
    fn transition<T>(&self, apply: impl FnOnce(&mut AuthStateInner) -> Option<T>) -> Option<T> {
        let mut inner = self.inner.lock().expect("auth state mutex poisoned");
        let applied = apply(&mut inner)?;
        let state = inner.state.clone();
        drop(inner);
        AuthPresenter::auth_state_changed(self.platform.as_ref(), state);
        Some(applied)
    }
}
