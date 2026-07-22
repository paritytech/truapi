//! Proactive renewal of statement-store allowances across period boundaries.
//!
//! Allowances are claimed per UTC-day period and die at the boundary, so a
//! long-lived host must re-register every account it promised to keep allowed
//! (RFC-0010 assigns renewal to the Account Holder). This module is the
//! chain-pure pass: given already-resolved targets, register each for the
//! requested period. Scheduling and target persistence live in
//! `signing_host::allowance_renewal`.

use std::time::Duration;

use futures::lock::Mutex;
use tracing::{debug, info, warn};

use super::extension::{ChainState, Metadata};
use super::ring::RingParams;
use super::rpc::RpcClient;
use super::slot::STATEMENT_STORE_PERIOD_SECONDS;
use super::{RegistrationOutcome, register_statement_account};

/// Cap between renewal ticks, mirroring the on-chain grace period after a
/// period boundary.
const MAX_TICK_INTERVAL: Duration = Duration::from_secs(3_600);
/// Margin after a period boundary before the boundary tick fires, so the
/// chain has rotated to the new period by the time we scan slots.
const PERIOD_BOUNDARY_MARGIN: Duration = Duration::from_secs(120);

/// One resolved renewal target: account id plus a label for reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRenewalTarget {
    /// Human-readable name used in logs and reports.
    pub label: String,
    /// Account to keep allowed.
    pub account_id: [u8; 32],
}

/// Outcome of renewing one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetRenewalStatus {
    /// The extrinsic reached a block; the target holds `seq` this period.
    Registered {
        /// Claimed slot sequence.
        seq: u32,
        /// Block hash the extrinsic landed in.
        block_hash: String,
    },
    /// The target already held a slot this period; nothing submitted.
    AlreadyAllocated {
        /// Existing slot sequence.
        seq: u32,
    },
    /// Registration failed; the target is retried on the next tick.
    Failed {
        /// Failure detail.
        reason: String,
    },
    /// Not attempted: the host ran out of slots earlier in the pass.
    SkippedExhausted,
}

/// Summary of one renewal pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRenewalReport {
    /// Period the pass registered for.
    pub period: u32,
    /// Per-target `(label, status)` in ledger order.
    pub outcomes: Vec<(String, TargetRenewalStatus)>,
    /// Whether the pass hit slot exhaustion for this period.
    pub slots_exhausted: bool,
}

/// Chain context shared by every registration in one renewal pass.
pub struct RenewalChainContext<'a> {
    /// People-chain RPC connection.
    pub rpc: &'a RpcClient,
    /// Decoded runtime metadata.
    pub metadata: &'a Metadata,
    /// Signed-extension chain state.
    pub chain_state: &'a ChainState,
    /// Ring the host's membership proof is built against.
    pub ring: &'a RingParams,
}

/// Register every target for `period`, continuing past per-target failures
/// and stopping early once the host's slots for the period are exhausted
/// (remaining targets are reported as skipped).
///
/// `registration_lock` is held per target, not for the whole pass, so an
/// on-demand allocation sharing the lock waits at most one registration.
pub async fn renew_targets(
    context: &RenewalChainContext<'_>,
    entropy: [u8; 32],
    period: u32,
    targets: &[ResolvedRenewalTarget],
    registration_lock: &Mutex<()>,
) -> StatementRenewalReport {
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        let result = {
            let _guard = registration_lock.lock().await;
            register_statement_account(
                context.rpc,
                context.metadata,
                context.chain_state,
                entropy,
                &target.account_id,
                period,
                context.ring,
            )
            .await
        };
        log_target_result(period, &target.label, &result);
        let exhausted = matches!(&result, Err(reason) if is_slot_exhaustion(reason));
        results.push(result);
        if exhausted {
            break;
        }
    }
    fold_outcomes(period, targets, results)
}

/// Delay until the next renewal tick: hourly, but always shortly after each
/// period boundary so expired allowances are refreshed within the grace window.
pub fn next_tick_delay(now_seconds: u64) -> Duration {
    let next_boundary =
        (now_seconds / STATEMENT_STORE_PERIOD_SECONDS + 1) * STATEMENT_STORE_PERIOD_SECONDS;
    let until_after_boundary =
        Duration::from_secs(next_boundary - now_seconds) + PERIOD_BOUNDARY_MARGIN;
    until_after_boundary.min(MAX_TICK_INTERVAL)
}

fn log_target_result(period: u32, label: &str, result: &Result<RegistrationOutcome, String>) {
    match result {
        Ok(RegistrationOutcome::Registered {
            block_hash, seq, ..
        }) => info!(period, label, seq, %block_hash, "renewed statement-store allowance"),
        Ok(RegistrationOutcome::AlreadyAllocated { seq }) => {
            debug!(
                period,
                label, seq, "statement-store allowance already fresh"
            );
        }
        Err(reason) => warn!(period, label, %reason, "statement-store renewal failed"),
    }
}

/// Pair each target with its registration result; targets past the end of
/// `results` were never attempted (the pass stopped on slot exhaustion).
fn fold_outcomes(
    period: u32,
    targets: &[ResolvedRenewalTarget],
    results: Vec<Result<RegistrationOutcome, String>>,
) -> StatementRenewalReport {
    let mut slots_exhausted = false;
    let mut results = results.into_iter();
    let outcomes = targets
        .iter()
        .map(|target| {
            let status = match results.next() {
                Some(Ok(RegistrationOutcome::Registered {
                    block_hash, seq, ..
                })) => TargetRenewalStatus::Registered { seq, block_hash },
                Some(Ok(RegistrationOutcome::AlreadyAllocated { seq })) => {
                    TargetRenewalStatus::AlreadyAllocated { seq }
                }
                Some(Err(reason)) => {
                    if is_slot_exhaustion(&reason) {
                        slots_exhausted = true;
                    }
                    TargetRenewalStatus::Failed { reason }
                }
                None => TargetRenewalStatus::SkippedExhausted,
            };
            (target.label.clone(), status)
        })
        .collect();
    StatementRenewalReport {
        period,
        outcomes,
        slots_exhausted,
    }
}

fn is_slot_exhaustion(reason: &str) -> bool {
    reason.contains("no free StatementStore slot")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(label: &str) -> ResolvedRenewalTarget {
        ResolvedRenewalTarget {
            label: label.to_string(),
            account_id: [0u8; 32],
        }
    }

    #[test]
    fn tick_delay_caps_at_one_hour_mid_day() {
        let mid_day = 86_400 * 20_000 + 43_200;
        assert_eq!(next_tick_delay(mid_day), Duration::from_secs(3_600));
    }

    #[test]
    fn tick_delay_lands_after_the_period_boundary() {
        let just_before_boundary = 86_400 * 20_001 - 10;
        assert_eq!(
            next_tick_delay(just_before_boundary),
            Duration::from_secs(10 + 120)
        );
    }

    #[test]
    fn tick_delay_at_boundary_reverts_to_hourly() {
        assert_eq!(next_tick_delay(86_400 * 20_001), Duration::from_secs(3_600));
    }

    #[test]
    fn mid_list_failure_does_not_stop_the_pass() {
        let targets = [target("a"), target("b"), target("c")];
        let report = fold_outcomes(
            7,
            &targets,
            vec![
                Ok(RegistrationOutcome::AlreadyAllocated { seq: 1 }),
                Err("rpc timeout".to_string()),
                Ok(RegistrationOutcome::Registered {
                    block_hash: "0xabc".to_string(),
                    seq: 2,
                    ring_index: 0,
                }),
            ],
        );
        assert_eq!(
            report,
            StatementRenewalReport {
                period: 7,
                outcomes: vec![
                    (
                        "a".to_string(),
                        TargetRenewalStatus::AlreadyAllocated { seq: 1 }
                    ),
                    (
                        "b".to_string(),
                        TargetRenewalStatus::Failed {
                            reason: "rpc timeout".to_string()
                        }
                    ),
                    (
                        "c".to_string(),
                        TargetRenewalStatus::Registered {
                            seq: 2,
                            block_hash: "0xabc".to_string()
                        }
                    ),
                ],
                slots_exhausted: false,
            }
        );
    }

    #[test]
    fn exhaustion_skips_remaining_targets() {
        let targets = [target("a"), target("b"), target("c")];
        let report = fold_outcomes(
            7,
            &targets,
            vec![
                Ok(RegistrationOutcome::AlreadyAllocated { seq: 0 }),
                Err("no free StatementStore slot in period 7 (max 8)".to_string()),
            ],
        );
        assert_eq!(
            report,
            StatementRenewalReport {
                period: 7,
                outcomes: vec![
                    (
                        "a".to_string(),
                        TargetRenewalStatus::AlreadyAllocated { seq: 0 }
                    ),
                    (
                        "b".to_string(),
                        TargetRenewalStatus::Failed {
                            reason: "no free StatementStore slot in period 7 (max 8)".to_string()
                        }
                    ),
                    ("c".to_string(), TargetRenewalStatus::SkippedExhausted),
                ],
                slots_exhausted: true,
            }
        );
    }
}
