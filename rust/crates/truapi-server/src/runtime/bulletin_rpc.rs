//! In-core Bulletin preimage submission over the shared Subxt client.
//!
//! One submission at a time: build + sign the `TransactionStorage.store`
//! extrinsic against the current best block (Subxt resolves metadata, nonce,
//! and the mortality anchor), dry-run it via Subxt's transaction validation
//! (broadcast is spec-guaranteed silent on invalid transactions, so the
//! dry-run is the only deterministic error signal), then submit through
//! Subxt's transaction watch and classify the dispatch outcome from the
//! inclusion block's events.

use std::sync::Mutex as StdMutex;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use futures::{FutureExt, pin_mut};
use subxt::client::{Block, Blocks, OnlineClientAtBlockImpl};
use subxt::config::substrate::SubstrateConfig;
use subxt::error::{DispatchError, ExtrinsicError, TransactionEventsError};
use subxt::tx::{
    SubmittableTransaction, TransactionInBlock, TransactionInvalid, TransactionStatus,
    TransactionUnknown, ValidationResult,
};
#[cfg(not(target_arch = "wasm32"))]
use subxt_rpcs::RpcClient;
use tracing::{instrument, warn};
use truapi::CallContext;
use truapi_platform::BulletinAllowanceKey;

use crate::chain_runtime::ChainRuntime;
#[cfg(not(target_arch = "wasm32"))]
use crate::chain_runtime::RuntimeFailure;
use crate::host_logic::bulletin::{
    STORE_PALLET_NAME, allowance_signer, build_signed_store_transaction, preimage_key,
};
use crate::host_logic::extrinsic::Sr25519Signer;

/// Retry once when a broadcast cannot be verified after a successful dry-run.
/// This covers the post-allocation propagation window where dry-run can
/// succeed against one node while the authoring path rejects or drops the
/// broadcast.
const SUBMIT_ATTEMPTS: usize = 2;
/// Number of newer best blocks to try before treating a dry-run allowance
/// rejection as real. Wallet allocation can return before the freshly granted
/// Bulletin authorization is visible to the chain state used by dry-run.
const ALLOWANCE_DRY_RUN_PROPAGATION_BLOCKS: usize = 20;
/// Budget for the stream to produce the next best block used by a dry-run
/// retry.
const ALLOWANCE_DRY_RUN_BLOCK_TIMEOUT: Duration = Duration::from_secs(10);
/// Budget for the best-block stream to replay the current chain head.
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
/// Quiet window after which the newest replayed block is taken as the head.
/// The replay delivers the initialized finalized block first and any known
/// best block right after; active chains never need the full window.
const BEST_BLOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// `TransactionStorage` module errors that mean the allowance account itself
/// was rejected (missing or exhausted authorization), i.e. the one condition
/// where refreshing the allowance key and retrying can help.
const ALLOWANCE_REJECTED_MODULE_ERRORS: &[&str] =
    &["AuthorizationNotFound", "PermanentAllowanceExceeded"];

/// Where a submission failed, for phase-tagged timeout reasons.
type Phase = &'static str;

/// The at-block client every submission step runs against.
type BulletinAtBlock = OnlineClientAtBlockImpl<SubstrateConfig>;

/// A signed store transaction bound to its build block, ready for the
/// dry-run and submission steps.
type SignedStore = SubmittableTransaction<SubstrateConfig, BulletinAtBlock>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DryRunStatus {
    Valid,
    AllowanceRejected,
}

/// Typed submission failure driving the retry decision at the runtime call
/// site. Wire mapping stays `v01::PreimageSubmitError::Unknown { reason }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BulletinSubmitError {
    /// Connection, block stream, metadata, or nonce plumbing failed.
    ChainUnavailable { reason: String },
    /// The dry-run rejected the transaction for a non-allowance reason.
    InvalidTransaction { kind: String },
    /// The allowance account was rejected; refresh + one retry may help.
    AllowanceRejected { phase: AllowanceRejectionPhase },
    /// The dry-run saw a nonce race (`Future`/`Stale`); no refresh.
    NonceRace,
    /// The submission budget elapsed; `phase` names the step reached.
    Timeout { phase: Phase },
    /// The transaction was broadcast but inclusion could not be verified.
    BroadcastUnverified { reason: String },
    /// The extrinsic landed but its dispatch failed for a non-allowance
    /// reason.
    IncludedButFailed { pallet: String, error: String },
    /// The calling context was cancelled.
    Cancelled,
}

/// Which stage rejected the allowance account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllowanceRejectionPhase {
    DryRun,
    Dispatch,
}

impl BulletinSubmitError {
    fn is_retryable_submission_uncertain(&self, phase: Phase) -> bool {
        matches!(self, Self::Timeout { phase } if *phase == "watch")
            || (phase == "watch" && matches!(self, Self::BroadcastUnverified { .. }))
    }

    /// Structured reason string carried in the wire error.
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::ChainUnavailable { reason } => {
                format!("bulletin chain unavailable: {reason}")
            }
            Self::InvalidTransaction { kind } => format!("invalid: {kind}"),
            Self::AllowanceRejected { phase } => {
                let phase = match phase {
                    AllowanceRejectionPhase::DryRun => "dry-run",
                    AllowanceRejectionPhase::Dispatch => "dispatch",
                };
                format!("allowance rejected: {phase}")
            }
            Self::NonceRace => "nonce race: retry".to_string(),
            Self::Timeout { phase } => format!("timeout: {phase}, inclusion unverified"),
            Self::BroadcastUnverified { reason } => format!("inclusion unverified: {reason}"),
            Self::IncludedButFailed { pallet, error } => {
                format!("dispatch error: {pallet}.{error}")
            }
            Self::Cancelled => "cancelled".to_string(),
        }
    }
}

/// Bulletin-chain submission service shared by all product runtimes.
pub(crate) struct BulletinRpc {
    chain: ChainRuntime,
    genesis_hash: [u8; 32],
    /// Serializes submissions: no same-account nonce races between
    /// concurrent submits.
    submit_lock: futures::lock::Mutex<()>,
    /// Last phase entered, for timeout reasons.
    phase: StdMutex<Phase>,
}

impl BulletinRpc {
    /// Build a bulletin submission service over the shared chain runtime.
    pub(crate) fn new(chain: ChainRuntime, genesis_hash: [u8; 32]) -> Self {
        Self {
            chain,
            genesis_hash,
            submit_lock: futures::lock::Mutex::new(()),
            phase: StdMutex::new("connect"),
        }
    }

    /// Open a raw RPC client over the configured Bulletin chain.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn client(&self, label: &'static str) -> Result<RpcClient, RuntimeFailure> {
        self.chain
            .rpc_client(label, &self.genesis_hash)
            .await
            .map(RpcClient::new)
    }

    /// Submit `value` as a Bulletin preimage signed by `allowance`, returning
    /// the preimage key once the transaction is included and its dispatch
    /// succeeded.
    #[instrument(skip_all, fields(runtime.method = "bulletin_rpc.submit_preimage"))]
    pub(crate) async fn submit_preimage(
        &self,
        cx: &CallContext,
        budget: Duration,
        allowance: &BulletinAllowanceKey,
        value: &[u8],
    ) -> Result<Vec<u8>, BulletinSubmitError> {
        // Serialize submissions, keeping the lock wait cancellable.
        let lock = self.submit_lock.lock().fuse();
        let lock_cancelled = cx.cancel().cancelled().fuse();
        pin_mut!(lock, lock_cancelled);
        let _guard = futures::select! {
            guard = lock => guard,
            _ = lock_cancelled => return Err(BulletinSubmitError::Cancelled),
        };

        // The whole-submission budget is set by the Preimage::submit boundary
        // and passed in explicitly; the context is used only for cancellation.
        // The budget starts once the lock is held; dropping the flow on
        // timeout/cancel drops its in-flight chain work.
        let started = Instant::now();
        let mut attempt = 0;
        loop {
            attempt += 1;
            let Some(remaining) = budget.checked_sub(started.elapsed()) else {
                return Err(BulletinSubmitError::Timeout {
                    phase: self.current_phase(),
                });
            };
            let flow = self.submit_flow(allowance, value).fuse();
            let timeout = futures_timer::Delay::new(remaining).fuse();
            let cancelled = cx.cancel().cancelled().fuse();
            pin_mut!(flow, timeout, cancelled);
            let result = futures::select! {
                result = flow => result,
                () = timeout => Err(BulletinSubmitError::Timeout { phase: self.current_phase() }),
                _ = cancelled => Err(BulletinSubmitError::Cancelled),
            };
            match result {
                Err(err)
                    if attempt < SUBMIT_ATTEMPTS
                        && err.is_retryable_submission_uncertain(self.current_phase()) =>
                {
                    warn!(
                        attempt,
                        reason = %err.reason(),
                        "Bulletin preimage broadcast not included; retrying"
                    );
                }
                result => return result,
            }
        }
    }

    async fn submit_flow(
        &self,
        allowance: &BulletinAllowanceKey,
        value: &[u8],
    ) -> Result<Vec<u8>, BulletinSubmitError> {
        let key = preimage_key(value);

        self.enter_phase("connect");
        let client = self
            .chain
            .online_client(&self.genesis_hash)
            .await
            .map_err(|failure| BulletinSubmitError::ChainUnavailable {
                reason: failure.reason(),
            })?;
        let mut best_blocks = client.stream_best_blocks().await.map_err(|error| {
            BulletinSubmitError::ChainUnavailable {
                reason: format!("best-block stream unavailable: {error}"),
            }
        })?;
        let head = initial_best_block(&mut best_blocks).await?;

        self.enter_phase("build");
        let signer = allowance_signer(allowance)
            .map_err(|reason| BulletinSubmitError::InvalidTransaction { kind: reason })?;
        let signed = self
            .build_signed_and_dry_run(&mut best_blocks, head, &signer, value)
            .await?;
        drop(best_blocks);

        self.enter_phase("watch");
        let in_block = watch_until_included(&signed).await?;

        self.enter_phase("events");
        require_dispatch_success(&in_block).await?;

        Ok(key.to_vec())
    }

    /// Build, sign, and dry-run the extrinsic against the chosen best block.
    /// Broadcast never reports invalid transactions, so dry-run is the only
    /// deterministic signal for stale allowances, nonce races, and encoding
    /// errors.
    async fn build_signed_and_dry_run(
        &self,
        best_blocks: &mut Blocks<SubstrateConfig>,
        head: Block<SubstrateConfig>,
        signer: &Sr25519Signer,
        value: &[u8],
    ) -> Result<SignedStore, BulletinSubmitError> {
        let mut block = head;
        let mut allowance_rejections = 0;
        loop {
            self.enter_phase("build");
            let at_block =
                block
                    .at()
                    .await
                    .map_err(|error| BulletinSubmitError::ChainUnavailable {
                        reason: format!("block {} unavailable: {error}", block.number()),
                    })?;
            let signed = match build_signed_store_transaction(&at_block, signer, value).await {
                Ok(signed) => signed,
                Err(error) => {
                    warn!(
                        block = block.number(),
                        error = %error,
                        "Bulletin store transaction assembly failed"
                    );
                    return Err(map_store_transaction_build_error(error));
                }
            };

            self.enter_phase("dry-run");
            let validity = match signed.validate().await {
                Ok(validity) => validity,
                Err(error) => {
                    warn!(
                        block = block.number(),
                        error = %error,
                        "Bulletin transaction dry-run failed"
                    );
                    return Err(BulletinSubmitError::ChainUnavailable {
                        reason: format!("transaction dry-run unavailable: {error}"),
                    });
                }
            };
            match Self::classify_dry_run_validity(validity)? {
                DryRunStatus::Valid => return Ok(signed),
                DryRunStatus::AllowanceRejected
                    if allowance_rejections < ALLOWANCE_DRY_RUN_PROPAGATION_BLOCKS =>
                {
                    allowance_rejections += 1;
                    warn!(
                        attempt = allowance_rejections,
                        "Bulletin allowance not visible to dry-run yet; rebuilding at next block"
                    );
                    block =
                        next_best_block(best_blocks, ALLOWANCE_DRY_RUN_BLOCK_TIMEOUT, "dry-run")
                            .await?;
                }
                DryRunStatus::AllowanceRejected => {
                    return Err(BulletinSubmitError::AllowanceRejected {
                        phase: AllowanceRejectionPhase::DryRun,
                    });
                }
            }
        }
    }

    fn classify_dry_run_validity(
        validity: ValidationResult,
    ) -> Result<DryRunStatus, BulletinSubmitError> {
        match validity {
            ValidationResult::Valid(_) => Ok(DryRunStatus::Valid),
            ValidationResult::Invalid(
                TransactionInvalid::Payment
                | TransactionInvalid::Custom(_)
                | TransactionInvalid::BadSigner,
            ) => Ok(DryRunStatus::AllowanceRejected),
            ValidationResult::Invalid(TransactionInvalid::Future | TransactionInvalid::Stale) => {
                Err(BulletinSubmitError::NonceRace)
            }
            ValidationResult::Invalid(other) => Err(BulletinSubmitError::InvalidTransaction {
                kind: format!("{other:?}"),
            }),
            ValidationResult::Unknown(TransactionUnknown::CannotLookup) => {
                Err(BulletinSubmitError::ChainUnavailable {
                    reason: "transaction validity could not be looked up".to_string(),
                })
            }
            ValidationResult::Unknown(other) => Err(BulletinSubmitError::InvalidTransaction {
                kind: format!("{other:?}"),
            }),
        }
    }

    fn enter_phase(&self, phase: Phase) {
        *self.phase.lock().expect("phase slot poisoned") = phase;
    }

    fn current_phase(&self) -> Phase {
        *self.phase.lock().expect("phase slot poisoned")
    }
}

/// Take the stream's replayed view of the current chain head: the initialized
/// finalized block arrives first, followed by the newest known best block.
/// Returns the newest block seen before a quiet [`BEST_BLOCK_TIMEOUT`] window;
/// falling back to the finalized block is safe for cold starts.
async fn initial_best_block(
    blocks: &mut Blocks<SubstrateConfig>,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
    let mut block = next_best_block(blocks, INITIALIZATION_TIMEOUT, "connect").await?;
    loop {
        match next_best_block(blocks, BEST_BLOCK_TIMEOUT, "connect").await {
            Ok(newer) => block = newer,
            Err(BulletinSubmitError::Timeout { .. }) => return Ok(block),
            Err(other) => return Err(other),
        }
    }
}

async fn next_best_block(
    blocks: &mut Blocks<SubstrateConfig>,
    timeout: Duration,
    phase: Phase,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
    let timeout = futures_timer::Delay::new(timeout).fuse();
    let next = blocks.next().fuse();
    pin_mut!(timeout, next);
    futures::select! {
        block = next => match block {
            Some(Ok(block)) => Ok(block),
            Some(Err(error)) => Err(BulletinSubmitError::ChainUnavailable {
                reason: format!("Bulletin best-block stream failed: {error}"),
            }),
            None => Err(BulletinSubmitError::ChainUnavailable {
                reason: "Bulletin best-block stream ended".to_string(),
            }),
        },
        () = timeout => Err(BulletinSubmitError::Timeout { phase }),
    }
}

fn map_store_transaction_build_error(error: ExtrinsicError) -> BulletinSubmitError {
    match error {
        ExtrinsicError::AccountNonceError { reason, .. } => BulletinSubmitError::ChainUnavailable {
            reason: format!("account nonce unavailable: {reason}"),
        },
        other => BulletinSubmitError::InvalidTransaction {
            kind: format!("store transaction assembly failed: {other}"),
        },
    }
}

/// Submit the signed transaction and watch its progress until it lands in a
/// best or finalized block.
async fn watch_until_included(
    signed: &SignedStore,
) -> Result<TransactionInBlock<SubstrateConfig, BulletinAtBlock>, BulletinSubmitError> {
    let unverified = |reason: String| BulletinSubmitError::BroadcastUnverified { reason };
    let mut progress = signed
        .submit_and_watch()
        .await
        .map_err(|error| unverified(format!("transaction submit failed: {error}")))?;
    while let Some(status) = progress.next().await {
        let status =
            status.map_err(|error| unverified(format!("transaction watch failed: {error}")))?;
        match status {
            TransactionStatus::InBestBlock(block) | TransactionStatus::InFinalizedBlock(block) => {
                return Ok(block);
            }
            TransactionStatus::Invalid { message } => {
                return Err(unverified(format!(
                    "transaction invalid after successful dry-run: {message}"
                )));
            }
            TransactionStatus::Dropped { message } => {
                return Err(unverified(format!("transaction dropped: {message}")));
            }
            TransactionStatus::Error { message } => {
                return Err(unverified(format!("transaction error: {message}")));
            }
            TransactionStatus::Validated
            | TransactionStatus::Broadcasted
            | TransactionStatus::NoLongerInBestBlock => {}
        }
    }
    Err(unverified(
        "transaction watch stream ended before inclusion".to_string(),
    ))
}

/// Require a successful dispatch outcome from the inclusion block's events.
/// Fail-closed: inclusion without an explicit `System.ExtrinsicSuccess` event
/// is reported as unverified, never as success.
async fn require_dispatch_success(
    in_block: &TransactionInBlock<SubstrateConfig, BulletinAtBlock>,
) -> Result<(), BulletinSubmitError> {
    let unverified = |reason: String| BulletinSubmitError::BroadcastUnverified { reason };
    match in_block.wait_for_success().await {
        Ok(events) => {
            for event in events.iter() {
                let event =
                    event.map_err(|err| unverified(format!("invalid transaction event: {err}")))?;
                if event.pallet_name() == "System" && event.event_name() == "ExtrinsicSuccess" {
                    return Ok(());
                }
            }
            Err(unverified(
                "included, but the block reported no dispatch outcome".to_string(),
            ))
        }
        Err(TransactionEventsError::ExtrinsicFailed(error)) => Err(classify_dispatch_error(error)),
        Err(other) => Err(unverified(format!(
            "transaction events unavailable: {other}"
        ))),
    }
}

/// Map a dispatch failure to the submission error, singling out the
/// allowance-rejection module errors that a key refresh can fix.
fn classify_dispatch_error(error: DispatchError) -> BulletinSubmitError {
    let DispatchError::Module(module_error) = &error else {
        return BulletinSubmitError::IncludedButFailed {
            pallet: "unknown".to_string(),
            error: error.to_string(),
        };
    };
    match module_error.details() {
        Ok(details) => {
            let pallet = details.pallet.name().to_string();
            let error = details.variant.name.clone();
            if pallet == STORE_PALLET_NAME
                && ALLOWANCE_REJECTED_MODULE_ERRORS.contains(&error.as_str())
            {
                BulletinSubmitError::AllowanceRejected {
                    phase: AllowanceRejectionPhase::Dispatch,
                }
            } else {
                BulletinSubmitError::IncludedButFailed { pallet, error }
            }
        }
        Err(_) => BulletinSubmitError::IncludedButFailed {
            pallet: "unknown".to_string(),
            error: module_error.details_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::extrinsic::tests::bulletin_metadata;
    use subxt::metadata::ArcMetadata;

    #[test]
    fn dry_run_classifies_allowance_rejections_for_retry() {
        for validity in [
            ValidationResult::Invalid(TransactionInvalid::Payment),
            ValidationResult::Invalid(TransactionInvalid::Custom(7)),
            ValidationResult::Invalid(TransactionInvalid::BadSigner),
        ] {
            assert_eq!(
                BulletinRpc::classify_dry_run_validity(validity).unwrap(),
                DryRunStatus::AllowanceRejected
            );
        }
    }

    #[test]
    fn retries_only_uncertain_watch_phase_submissions() {
        let unverified = BulletinSubmitError::BroadcastUnverified {
            reason: "transaction invalid after successful dry-run".to_string(),
        };
        assert!(unverified.is_retryable_submission_uncertain("watch"));
        assert!(!unverified.is_retryable_submission_uncertain("events"));

        assert!(
            BulletinSubmitError::Timeout { phase: "watch" }
                .is_retryable_submission_uncertain("connect")
        );
        assert!(
            !BulletinSubmitError::Timeout { phase: "dry-run" }
                .is_retryable_submission_uncertain("dry-run")
        );
    }

    /// Decode a `DispatchError::Module` for the named error variant out of
    /// the bulletin fixture metadata.
    fn module_error(error_name: &str) -> DispatchError {
        let metadata = ArcMetadata::from(bulletin_metadata());
        let pallet = metadata.pallet_by_name(STORE_PALLET_NAME).unwrap();
        let variant_index = (0..=u8::MAX)
            .find(|index| {
                pallet
                    .error_variant_by_index(*index)
                    .is_some_and(|variant| variant.name == error_name)
            })
            .unwrap_or_else(|| panic!("fixture metadata lacks the {error_name} error"));
        // `DispatchError::Module` is variant 3: (pallet index, 4 error bytes).
        let bytes = [3, pallet.error_index(), variant_index, 0, 0, 0];
        DispatchError::decode_from(&bytes, metadata).unwrap()
    }

    /// Any error variant that is not an allowance rejection.
    fn non_allowance_error_name() -> String {
        let metadata = ArcMetadata::from(bulletin_metadata());
        let pallet = metadata.pallet_by_name(STORE_PALLET_NAME).unwrap();
        (0..=u8::MAX)
            .find_map(|index| {
                let name = &pallet.error_variant_by_index(index)?.name;
                (!ALLOWANCE_REJECTED_MODULE_ERRORS.contains(&name.as_str())).then(|| name.clone())
            })
            .expect("fixture metadata has a non-allowance error variant")
    }

    #[test]
    fn dispatch_errors_classify_allowance_rejections() {
        assert_eq!(
            classify_dispatch_error(module_error("AuthorizationNotFound")),
            BulletinSubmitError::AllowanceRejected {
                phase: AllowanceRejectionPhase::Dispatch
            }
        );

        let other = non_allowance_error_name();
        assert_eq!(
            classify_dispatch_error(module_error(&other)),
            BulletinSubmitError::IncludedButFailed {
                pallet: STORE_PALLET_NAME.to_string(),
                error: other
            }
        );
    }

    #[test]
    fn error_reason_strings_are_stable() {
        assert_eq!(
            BulletinSubmitError::AllowanceRejected {
                phase: AllowanceRejectionPhase::DryRun
            }
            .reason(),
            "allowance rejected: dry-run"
        );
        assert_eq!(BulletinSubmitError::NonceRace.reason(), "nonce race: retry");
        assert_eq!(
            BulletinSubmitError::Timeout { phase: "watch" }.reason(),
            "timeout: watch, inclusion unverified"
        );
        assert_eq!(
            BulletinSubmitError::IncludedButFailed {
                pallet: "TransactionStorage".to_string(),
                error: "BadContext".to_string()
            }
            .reason(),
            "dispatch error: TransactionStorage.BadContext"
        );
    }
}
