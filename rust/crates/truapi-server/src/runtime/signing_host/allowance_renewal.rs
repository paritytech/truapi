//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
use tracing::{debug, info, warn};
use truapi_platform::{CoreStorage, CoreStorageKey};

use super::SigningHost;
use super::sso_responder::current_unix_secs;
use crate::host_logic::product_account::derive_sr25519_hard_path;
use crate::runtime::RuntimeServices;
use crate::runtime::statement_allowance::renewal::{
    RenewalChainContext, ResolvedRenewalTarget, StatementRenewalReport, next_tick_delay,
    renew_targets,
};
use crate::runtime::statement_allowance::{
    self, fetch_chain_state, fetch_metadata, find_including_ring,
};

/// Fallback tick delay when the system clock is unusable.
const CLOCK_FAILURE_TICK_DELAY: Duration = Duration::from_secs(3_600);

/// A statement-store account the signing host promised to keep renewed.
///
/// Entropy-derived variants are recipes, not raw account ids, so the ledger
/// survives root-entropy rotation (the CLI rotates auto-managed accounts on
/// slot exhaustion).
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum StatementRenewalTarget {
    /// `//allowance//statement-store//{product_id}` from the active root entropy.
    ProductStatementAllowance {
        /// Product the allowance account belongs to.
        product_id: String,
    },
    /// `//wallet//sso` from the active root entropy.
    WalletSso,
    /// A fixed account, e.g. a pairing peer's device statement key.
    Account {
        /// Account to keep allowed.
        account_id: [u8; 32],
        /// Human-readable name used in logs and reports.
        label: String,
    },
}

/// Renewal coordination state owned by [`SigningHost`].
#[derive(Default)]
pub(super) struct RenewalState {
    /// Serializes slot registrations between the renewal pass and on-demand
    /// allocation so both cannot race for the same free slot.
    registration_lock: Mutex<()>,
    loop_started: AtomicBool,
}

impl RenewalState {
    pub(super) fn registration_lock(&self) -> &Mutex<()> {
        &self.registration_lock
    }
}

/// Read the renewal ledger; an absent slot is an empty ledger.
pub(super) async fn read_targets(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<StatementRenewalTarget>, String> {
    let Some(blob) = storage
        .read_core_storage(CoreStorageKey::StatementRenewalTargets)
        .await
        .map_err(|err| format!("renewal ledger read failed: {}", err.reason))?
    else {
        return Ok(Vec::new());
    };
    decode_targets(&blob)
}

/// Append `new_targets` to the ledger, preserving order and skipping entries
/// already present.
pub(super) async fn track_targets(
    storage: &(impl CoreStorage + ?Sized),
    new_targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
    let mut targets = read_targets(storage).await?;
    let mut changed = false;
    for target in new_targets {
        if !targets.contains(&target) {
            targets.push(target);
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    storage
        .write_core_storage(CoreStorageKey::StatementRenewalTargets, targets.encode())
        .await
        .map_err(|err| format!("renewal ledger write failed: {}", err.reason))
}

fn decode_targets(blob: &[u8]) -> Result<Vec<StatementRenewalTarget>, String> {
    let mut input = blob;
    let targets = Vec::<StatementRenewalTarget>::decode(&mut input)
        .map_err(|err| format!("invalid persisted renewal targets: {err}"))?;
    if !input.is_empty() {
        return Err("invalid persisted renewal targets: trailing bytes".to_string());
    }
    Ok(targets)
}

/// Resolve a ledger entry into a concrete account for this session's entropy.
fn resolve_target(
    entropy: &[u8],
    target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, String> {
    match target {
        StatementRenewalTarget::ProductStatementAllowance { product_id } => {
            let pair = derive_sr25519_hard_path(
                entropy,
                &["allowance", "statement-store", product_id.as_str()],
            )
            .map_err(|err| err.to_string())?;
            Ok(ResolvedRenewalTarget {
                label: format!("product:{product_id}"),
                account_id: pair.public.to_bytes(),
            })
        }
        StatementRenewalTarget::WalletSso => {
            let pair = derive_sr25519_hard_path(entropy, &["wallet", "sso"])
                .map_err(|err| err.to_string())?;
            Ok(ResolvedRenewalTarget {
                label: "wallet-sso".to_string(),
                account_id: pair.public.to_bytes(),
            })
        }
        StatementRenewalTarget::Account { account_id, label } => Ok(ResolvedRenewalTarget {
            label: label.clone(),
            account_id: *account_id,
        }),
    }
}

/// One renewal pass: resolve the ledger against the active session and renew
/// every target for the current period.
pub(super) async fn renew_now(
    services: &Arc<RuntimeServices>,
    signing_host: &SigningHost,
) -> Result<StatementRenewalReport, String> {
    let entropy = signing_host.root_entropy().map_err(|err| err.reason())?;
    let period = statement_allowance::slot::current_period(current_unix_secs()?);
    let targets = read_targets(signing_host.platform.as_ref()).await?;
    let resolved = targets
        .iter()
        .map(|target| resolve_target(&entropy, target))
        .collect::<Result<Vec<_>, String>>()?;
    if resolved.is_empty() {
        return Ok(StatementRenewalReport {
            period,
            outcomes: Vec::new(),
            slots_exhausted: false,
        });
    }

    let bandersnatch = statement_allowance::bandersnatch_entropy(&entropy);
    let rpc = statement_allowance::rpc::RpcClient::new(
        services
            .statement_store
            .client("statement-allowance renewal")
            .await?,
    );
    let metadata = fetch_metadata(&rpc).await?;
    let chain_state = fetch_chain_state(&rpc).await?;
    let current = statement_allowance::ring::read_current_ring_index(&rpc).await?;
    let ring = find_including_ring(&rpc, &metadata, bandersnatch, current)
        .await?
        .ok_or_else(|| {
            "signing account is not a LitePeople ring member; cannot renew statement-store allowances"
                .to_string()
        })?;
    let context = RenewalChainContext {
        rpc: &rpc,
        metadata: &metadata,
        chain_state: &chain_state,
        ring: &ring,
    };
    Ok(renew_targets(
        &context,
        bandersnatch,
        period,
        &resolved,
        signing_host.renewal.registration_lock(),
    )
    .await)
}

/// Spawn the periodic renewal loop; repeated calls are no-ops. The loop holds
/// only weak references, so it exits when the owning runtime is dropped.
pub(super) fn start_renewal_loop(services: &Arc<RuntimeServices>, signing_host: &Arc<SigningHost>) {
    if signing_host
        .renewal
        .loop_started
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let weak_services = Arc::downgrade(services);
    let weak_host = Arc::downgrade(signing_host);
    let spawner = services.spawner.clone();
    spawner(Box::pin(async move {
        loop {
            {
                let (Some(services), Some(signing_host)) =
                    (weak_services.upgrade(), weak_host.upgrade())
                else {
                    return;
                };
                run_tick(&services, &signing_host).await;
            }
            let delay = match current_unix_secs() {
                Ok(now) => next_tick_delay(now),
                Err(_) => CLOCK_FAILURE_TICK_DELAY,
            };
            futures_timer::Delay::new(delay).await;
        }
    }));
}

async fn run_tick(services: &Arc<RuntimeServices>, signing_host: &SigningHost) {
    if signing_host.root_entropy().is_err() {
        debug!("skipping statement-store renewal tick; no active session");
        return;
    }
    match renew_now(services, signing_host).await {
        Ok(report) if report.slots_exhausted => {
            warn!(
                period = report.period,
                "statement-store renewal hit slot exhaustion"
            );
        }
        Ok(report) => {
            info!(
                period = report.period,
                targets = report.outcomes.len(),
                "statement-store renewal pass complete"
            );
        }
        Err(reason) => warn!(%reason, "statement-store renewal tick failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use truapi::latest::GenericError;

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    }

    #[truapi_platform::async_trait]
    impl CoreStorage for MemStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            Ok(self
                .inner
                .lock()
                .expect("storage mutex poisoned")
                .get(&key.encode())
                .cloned())
        }

        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .insert(key.encode(), value);
            Ok(())
        }

        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .remove(&key.encode());
            Ok(())
        }
    }

    fn product(product_id: &str) -> StatementRenewalTarget {
        StatementRenewalTarget::ProductStatementAllowance {
            product_id: product_id.to_string(),
        }
    }

    #[test]
    fn ledger_round_trips_dedupes_and_preserves_order() {
        let storage = MemStorage::default();

        futures::executor::block_on(async {
            track_targets(
                &storage,
                vec![StatementRenewalTarget::WalletSso, product("a.dot")],
            )
            .await
            .unwrap();
            track_targets(
                &storage,
                vec![
                    product("a.dot"),
                    StatementRenewalTarget::Account {
                        account_id: [9; 32],
                        label: "device".to_string(),
                    },
                ],
            )
            .await
            .unwrap();

            assert_eq!(
                read_targets(&storage).await.unwrap(),
                vec![
                    StatementRenewalTarget::WalletSso,
                    product("a.dot"),
                    StatementRenewalTarget::Account {
                        account_id: [9; 32],
                        label: "device".to_string(),
                    },
                ]
            );
        });
    }

    #[test]
    fn ledger_rejects_trailing_bytes() {
        let mut blob = vec![product("a.dot")].encode();
        blob.push(0xff);
        assert!(decode_targets(&blob).is_err());
    }

    #[test]
    fn product_target_resolves_to_allocation_derivation() {
        let entropy = [7u8; 32];
        let expected =
            derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", "a.dot"])
                .unwrap()
                .public
                .to_bytes();

        let resolved = resolve_target(&entropy, &product("a.dot")).unwrap();
        assert_eq!(
            resolved,
            ResolvedRenewalTarget {
                label: "product:a.dot".to_string(),
                account_id: expected,
            }
        );
    }

    #[test]
    fn wallet_sso_target_resolves_to_wallet_sso_derivation() {
        let entropy = [7u8; 32];
        let expected = derive_sr25519_hard_path(&entropy, &["wallet", "sso"])
            .unwrap()
            .public
            .to_bytes();

        let resolved = resolve_target(&entropy, &StatementRenewalTarget::WalletSso).unwrap();
        assert_eq!(
            resolved,
            ResolvedRenewalTarget {
                label: "wallet-sso".to_string(),
                account_id: expected,
            }
        );
    }
}
