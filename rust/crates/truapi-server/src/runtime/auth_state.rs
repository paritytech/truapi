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
/// in-flight login states, making its registration atomic with the transition.
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
    /// guard distinguish its own login from a newer flow's.
    pairing_epoch: u64,
    /// Resolves the in-flight login's cancel receiver. Present while the state
    /// is `Pairing` or `Authenticating`.
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
            if matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Authenticating
            ) {
                return None;
            }
            inner.state = AuthState::Pairing { deeplink };
            inner.pairing_epoch = inner.pairing_epoch.wrapping_add(1);
            inner.cancel_tx = Some(cancel_tx);
            Some(inner.pairing_epoch)
        })?;
        Some((cancel_rx, epoch))
    }

    /// `Pairing` -> `Authenticating`: the wallet accepted the pairing request
    /// and the core is resolving and persisting the session.
    pub(super) fn authentication_started(&self, epoch: u64) {
        self.transition(|inner| {
            if !matches!(inner.state, AuthState::Pairing { .. }) || inner.pairing_epoch != epoch {
                return None;
            }
            inner.state = AuthState::Authenticating;
            Some(())
        });
    }

    /// Active login -> `LoginFailed`: the in-flight login reported a failure.
    pub(super) fn login_failed(&self, reason: String) {
        self.transition(|inner| {
            if !matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Authenticating
            ) {
                return None;
            }
            inner.cancel_tx = None;
            inner.state = AuthState::LoginFailed { reason };
            Some(())
        });
    }

    /// `Disconnected`/`LoginFailed` -> `LoginFailed`: a login failed before
    /// it reached `Pairing` (device identity or bootstrap errors). A no-op
    /// while another login is active, so a concurrent second login attempt
    /// failing early cannot tear down the first one's presentation.
    pub(super) fn login_failed_before_pairing(&self, reason: String) {
        self.transition(|inner| {
            if matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Authenticating | AuthState::Connected(_)
            ) {
                return None;
            }
            inner.state = AuthState::LoginFailed { reason };
            Some(())
        });
    }

    /// Active login/`LoginFailed` -> `Disconnected` (host cancelled or
    /// dismissed). Wakes the in-flight login, which resolves as `Rejected`.
    pub(super) fn login_cancelled(&self) {
        self.transition(|inner| {
            if !matches!(
                inner.state,
                AuthState::Pairing { .. }
                    | AuthState::Authenticating
                    | AuthState::LoginFailed { .. }
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

    /// Session store reports no session. A no-op while a login is active: the
    /// flow owns its own terminal transition, and a boot-time store tick must
    /// not tear down the login UI.
    pub(super) fn store_disconnected(&self) {
        self.transition(|inner| {
            if matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Authenticating | AuthState::Disconnected
            ) {
                return None;
            }
            inner.state = AuthState::Disconnected;
            Some(())
        });
    }

    /// Reset a login left behind by a dropped future, but only when it still
    /// belongs to `epoch` (a newer flow is left alone).
    pub(super) fn reset_abandoned_pairing(&self, epoch: u64) {
        self.transition(|inner| {
            if !matches!(
                inner.state,
                AuthState::Pairing { .. } | AuthState::Authenticating
            ) || inner.pairing_epoch != epoch
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::stub_platform;

    #[test]
    fn pairing_started_refuses_a_second_login_while_authenticating() {
        let platform = stub_platform();
        let machine = AuthStateMachine::new(platform.clone());
        let (_cancel_rx, epoch) = machine
            .pairing_started("polkadotapp://first".to_string())
            .expect("first login should start");
        machine.authentication_started(epoch);

        assert!(
            machine
                .pairing_started("polkadotapp://second".to_string())
                .is_none(),
            "an authenticating login must retain the single-flight guard"
        );
        assert_eq!(
            *platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned"),
            vec![
                AuthState::Pairing {
                    deeplink: "polkadotapp://first".to_string(),
                },
                AuthState::Authenticating,
            ]
        );
    }

    #[test]
    fn login_cancelled_while_authenticating_disconnects_and_wakes_the_login() {
        let platform = stub_platform();
        let machine = AuthStateMachine::new(platform.clone());
        let (cancel_rx, epoch) = machine
            .pairing_started("polkadotapp://pair".to_string())
            .expect("login should start");
        machine.authentication_started(epoch);

        machine.login_cancelled();

        futures::executor::block_on(cancel_rx).expect("cancel signal should be delivered");
        assert_eq!(
            *platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned"),
            vec![
                AuthState::Pairing {
                    deeplink: "polkadotapp://pair".to_string(),
                },
                AuthState::Authenticating,
                AuthState::Disconnected,
            ]
        );
    }
}
