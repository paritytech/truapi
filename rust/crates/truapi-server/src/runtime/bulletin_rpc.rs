//! In-core Bulletin preimage submission over the shared Subxt client.
//!
//! One submission at a time: build + sign the `TransactionStorage.store`
//! extrinsic against the current best block (Subxt resolves metadata, nonce,
//! and the mortality anchor), dry-run it via Subxt's typed transaction
//! validation, then submit through Subxt's transaction watch and classify the
//! dispatch outcome from the inclusion block's events.

use std::sync::Mutex as StdMutex;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::chain_runtime::{ChainRuntime, RuntimeFailure};
use crate::host_logic::bulletin::{
    STORE_PALLET_NAME, allowance_signer, build_signed_store_transaction, preimage_key,
};
use crate::host_logic::extrinsic::Sr25519Signer;
use crate::runtime::BulletinAllowanceKey;
use futures::{FutureExt, pin_mut};
use subxt::client::{Block, Blocks, OnlineClientAtBlockImpl};
use subxt::config::substrate::SubstrateConfig;
use subxt::error::{
    DispatchError, TransactionEventsError, TransactionProgressError, TransactionStatusError,
};
use subxt::tx::{
    SubmittableTransaction, TransactionInBlock, TransactionInvalid, TransactionStatus,
    TransactionUnknown, ValidationResult,
};
use tracing::{instrument, warn};
use truapi::CallContext;

/// Retry once when a broadcast or its reported inclusion cannot be verified
/// after a successful dry-run. This covers the post-allocation propagation
/// window where RPC views can briefly disagree about the transaction.
const SUBMIT_ATTEMPTS: usize = 2;
/// Number of newer best blocks to try before treating a dry-run allowance
/// rejection as real. Three blocks are about 18 seconds on Bulletin and leave
/// room in the end-to-end submit timeout for one refresh and retry.
const ALLOWANCE_DRY_RUN_PROPAGATION_BLOCKS: usize = 3;
/// Bound each wait for a newer best block while allowance state propagates.
#[cfg(not(test))]
const ALLOWANCE_DRY_RUN_BLOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(test)]
const ALLOWANCE_DRY_RUN_BLOCK_WAIT_TIMEOUT: Duration = Duration::from_millis(25);
/// Budget for the best-block stream to replay the current chain head.
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
/// Quiet window after which the newest replayed block is taken as the head.
/// The replay delivers the initialized finalized block first and any known
/// best block right after; active chains never need the full window.
#[cfg(not(test))]
const BEST_BLOCK_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const BEST_BLOCK_TIMEOUT: Duration = Duration::from_millis(10);

/// `TransactionStorage` module errors that mean the allowance account itself
/// was rejected (missing or exhausted authorization), i.e. the one condition
/// where refreshing the allowance key and retrying can help.
const ALLOWANCE_REJECTED_MODULE_ERRORS: &[&str] =
    &["AuthorizationNotFound", "PermanentAllowanceExceeded"];

/// Where a submission currently is, for retry decisions and timeout reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub(crate) enum SubmissionPhase {
    #[display("connect")]
    Connect,
    #[display("build")]
    Build,
    #[display("dry-run")]
    DryRun,
    #[display("watch")]
    Watch,
    #[display("events")]
    Events,
}

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
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub(crate) enum BulletinSubmitError {
    /// The host-owned physical connection could not provide a Subxt client.
    #[display("bulletin chain unavailable: {_0}")]
    Host(#[error(source)] RuntimeFailure),
    /// Subxt failed while building, validating, submitting, or reading events.
    #[display("subxt: {_0}")]
    Subxt(#[error(source)] Box<subxt::Error>),
    /// The allowance key could not be converted to its purpose-scoped signer.
    #[display("invalid allowance key: {_0}")]
    InvalidAllowanceKey(#[error(not(source))] String),
    /// The runtime rejected the transaction for a non-allowance reason.
    #[display("invalid: {_0:?}")]
    InvalidTransaction(#[error(not(source))] TransactionInvalid),
    /// The runtime could not determine transaction validity.
    #[display("unknown transaction validity: {_0:?}")]
    UnknownTransaction(#[error(not(source))] TransactionUnknown),
    /// The allowance account was rejected; refresh + one retry may help.
    #[display("allowance rejected: {phase}")]
    AllowanceRejected { phase: AllowanceRejectionPhase },
    /// The submission budget elapsed; `phase` names the step reached.
    #[display(
        "timeout: {phase}{}",
        if matches!(*phase, SubmissionPhase::Watch | SubmissionPhase::Events) {
            ", inclusion unverified"
        } else {
            ""
        }
    )]
    Timeout { phase: SubmissionPhase },
    /// Inclusion events contained no explicit dispatch outcome.
    #[display("inclusion unverified: included block reported no dispatch outcome")]
    DispatchOutcomeMissing,
    /// The Subxt best-block stream ended without reporting an error.
    #[display("Bulletin best-block stream ended")]
    BestBlockStreamEnded,
    /// The calling context was cancelled.
    #[display("cancelled")]
    Cancelled,
}

/// Which stage rejected the allowance account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub(crate) enum AllowanceRejectionPhase {
    #[display("dry-run")]
    DryRun,
    #[display("dispatch")]
    Dispatch,
}

impl BulletinSubmitError {
    fn is_retryable_submission_uncertain(&self, phase: SubmissionPhase) -> bool {
        match self {
            Self::Subxt(_) if phase == SubmissionPhase::Watch => true,
            Self::Subxt(error) if phase == SubmissionPhase::Events => matches!(
                error.as_ref(),
                subxt::Error::TransactionEventsError(
                    TransactionEventsError::CannotFindTransactionInBlock { .. }
                )
            ),
            _ => false,
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
    phase: StdMutex<SubmissionPhase>,
}

impl BulletinRpc {
    /// Build a bulletin submission service over the shared chain runtime.
    pub(crate) fn new(chain: ChainRuntime, genesis_hash: [u8; 32]) -> Self {
        Self {
            chain,
            genesis_hash,
            submit_lock: futures::lock::Mutex::new(()),
            phase: StdMutex::new(SubmissionPhase::Connect),
        }
    }

    /// Submit `value` as a Bulletin preimage signed by `allowance`, returning
    /// the preimage key once the transaction is included and its dispatch
    /// succeeded.
    #[instrument(skip_all, fields(runtime.method = "bulletin_rpc.submit_preimage"))]
    pub(crate) async fn submit_preimage(
        &self,
        cx: &CallContext,
        deadline: Instant,
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

        // The Preimage::submit boundary owns one absolute chain deadline across
        // the initial submission and an optional refreshed-allowance retry.
        // The context is used only for cancellation. Dropping the flow on
        // timeout/cancel drops its in-flight chain work.
        let mut attempt = 0;
        loop {
            attempt += 1;
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
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
                        reason = %err,
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

        self.enter_phase(SubmissionPhase::Connect);
        let client = self
            .chain
            .online_client(&self.genesis_hash)
            .await
            .map_err(BulletinSubmitError::Host)?;
        let mut best_blocks = client
            .stream_best_blocks()
            .await
            .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
        let head = initial_best_block(&mut best_blocks).await?;

        self.enter_phase(SubmissionPhase::Build);
        let signer =
            allowance_signer(allowance).map_err(BulletinSubmitError::InvalidAllowanceKey)?;
        let signed = self
            .build_signed_and_dry_run(&mut best_blocks, head, &signer, value)
            .await?;
        drop(best_blocks);

        self.enter_phase(SubmissionPhase::Watch);
        let in_block = watch_until_included(&signed).await?;

        self.enter_phase(SubmissionPhase::Events);
        require_dispatch_success(&in_block).await?;

        Ok(key.to_vec())
    }

    /// Build, sign, and dry-run the extrinsic against the chosen best block.
    /// Dry-run provides the typed signal used to distinguish stale allowances,
    /// nonce races, and other runtime validity failures.
    async fn build_signed_and_dry_run(
        &self,
        best_blocks: &mut Blocks<SubstrateConfig>,
        head: Block<SubstrateConfig>,
        signer: &Sr25519Signer,
        value: &[u8],
    ) -> Result<SignedStore, BulletinSubmitError> {
        let mut block = head;
        let mut allowance_rejections = 0;
        let mut allowance_rejection_started = None;
        loop {
            self.enter_phase(SubmissionPhase::Build);
            let at_block = block
                .at()
                .await
                .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
            let signed = build_signed_store_transaction(&at_block, signer, value)
                .await
                .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;

            self.enter_phase(SubmissionPhase::DryRun);
            let validity = signed
                .validate()
                .await
                .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
            match Self::classify_dry_run_validity(validity)? {
                DryRunStatus::Valid => return Ok(signed),
                DryRunStatus::AllowanceRejected => {
                    let started = allowance_rejection_started.get_or_insert_with(Instant::now);
                    let elapsed = started.elapsed();
                    if allowance_rejections >= ALLOWANCE_DRY_RUN_PROPAGATION_BLOCKS {
                        warn!(
                            rejections = allowance_rejections + 1,
                            elapsed_ms = elapsed.as_millis(),
                            stop = "block-limit",
                            "Bulletin allowance remained unavailable to dry-run"
                        );
                        return Err(BulletinSubmitError::AllowanceRejected {
                            phase: AllowanceRejectionPhase::DryRun,
                        });
                    }
                    allowance_rejections += 1;
                    warn!(
                        attempt = allowance_rejections,
                        elapsed_ms = elapsed.as_millis(),
                        block_wait_timeout_ms = ALLOWANCE_DRY_RUN_BLOCK_WAIT_TIMEOUT.as_millis(),
                        "Bulletin allowance not visible to dry-run yet; rebuilding at next block"
                    );
                    block = match next_best_block(
                        best_blocks,
                        ALLOWANCE_DRY_RUN_BLOCK_WAIT_TIMEOUT,
                        SubmissionPhase::DryRun,
                    )
                    .await
                    {
                        Err(BulletinSubmitError::Timeout { .. }) => {
                            warn!(
                                rejections = allowance_rejections,
                                elapsed_ms = started.elapsed().as_millis(),
                                stop = "block-wait-timeout",
                                "Bulletin allowance remained unavailable to dry-run"
                            );
                            return Err(BulletinSubmitError::AllowanceRejected {
                                phase: AllowanceRejectionPhase::DryRun,
                            });
                        }
                        result => result?,
                    };
                }
            }
        }
    }

    fn classify_dry_run_validity(
        validity: ValidationResult,
    ) -> Result<DryRunStatus, BulletinSubmitError> {
        match validity {
            ValidationResult::Valid(_) => Ok(DryRunStatus::Valid),
            // Bulletin's fee/authorization transaction extensions report
            // stale or missing allowance through these generic validity
            // variants. The runtime does not publish stable Custom codes, so
            // narrowing Custom would bypass the bounded refresh path.
            ValidationResult::Invalid(
                TransactionInvalid::Payment
                | TransactionInvalid::Custom(_)
                | TransactionInvalid::BadSigner,
            ) => Ok(DryRunStatus::AllowanceRejected),
            ValidationResult::Invalid(other) => Err(BulletinSubmitError::InvalidTransaction(other)),
            ValidationResult::Unknown(other) => Err(BulletinSubmitError::UnknownTransaction(other)),
        }
    }

    fn enter_phase(&self, phase: SubmissionPhase) {
        *self.phase.lock().expect("phase slot poisoned") = phase;
    }

    fn current_phase(&self) -> SubmissionPhase {
        *self.phase.lock().expect("phase slot poisoned")
    }
}

/// Take the stream's replayed view of the current chain head: the initialized
/// finalized block arrives first, followed by the newest known best block.
/// Returns the newest block seen during one bounded [`BEST_BLOCK_TIMEOUT`]
/// replay window; falling back to the finalized block is safe for cold starts.
/// The deadline is absolute so a continuously advancing best-block stream
/// cannot keep initialization alive forever.
async fn initial_best_block(
    blocks: &mut Blocks<SubstrateConfig>,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
    let mut block =
        next_best_block(blocks, INITIALIZATION_TIMEOUT, SubmissionPhase::Connect).await?;
    let replay_deadline = futures_timer::Delay::new(BEST_BLOCK_TIMEOUT).fuse();
    pin_mut!(replay_deadline);
    loop {
        let next = blocks.next().fuse();
        pin_mut!(next);
        futures::select! {
            item = next => match item {
                Some(Ok(newer)) => block = newer,
                Some(Err(error)) => {
                    return Err(BulletinSubmitError::Subxt(Box::new(error.into())));
                }
                None => return Err(BulletinSubmitError::BestBlockStreamEnded),
            },
            () = replay_deadline => return Ok(block),
        }
    }
}

async fn next_best_block(
    blocks: &mut Blocks<SubstrateConfig>,
    timeout: Duration,
    phase: SubmissionPhase,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
    let timeout = futures_timer::Delay::new(timeout).fuse();
    let next = blocks.next().fuse();
    pin_mut!(timeout, next);
    futures::select! {
        block = next => match block {
            Some(Ok(block)) => Ok(block),
            Some(Err(error)) => Err(BulletinSubmitError::Subxt(Box::new(error.into()))),
            None => Err(BulletinSubmitError::BestBlockStreamEnded),
        },
        () = timeout => Err(BulletinSubmitError::Timeout { phase }),
    }
}

/// Submit the signed transaction and watch its progress until it lands in a
/// best or finalized block.
async fn watch_until_included(
    signed: &SignedStore,
) -> Result<TransactionInBlock<SubstrateConfig, BulletinAtBlock>, BulletinSubmitError> {
    let mut progress = signed
        .submit_and_watch()
        .await
        .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
    while let Some(status) = progress.next().await {
        let status = status.map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
        match status {
            TransactionStatus::InBestBlock(block) | TransactionStatus::InFinalizedBlock(block) => {
                return Ok(block);
            }
            TransactionStatus::Invalid { message } => {
                return Err(BulletinSubmitError::Subxt(Box::new(
                    TransactionStatusError::Invalid(message).into(),
                )));
            }
            TransactionStatus::Dropped { message } => {
                return Err(BulletinSubmitError::Subxt(Box::new(
                    TransactionStatusError::Dropped(message).into(),
                )));
            }
            TransactionStatus::Error { message } => {
                return Err(BulletinSubmitError::Subxt(Box::new(
                    TransactionStatusError::Error(message).into(),
                )));
            }
            TransactionStatus::Validated
            | TransactionStatus::Broadcasted
            | TransactionStatus::NoLongerInBestBlock => {}
        }
    }
    Err(BulletinSubmitError::Subxt(Box::new(
        TransactionProgressError::UnexpectedEndOfTransactionStatusStream.into(),
    )))
}

/// Require a successful dispatch outcome from the inclusion block's events.
/// Fail-closed: inclusion without an explicit `System.ExtrinsicSuccess` event
/// is reported as unverified, never as success.
async fn require_dispatch_success(
    in_block: &TransactionInBlock<SubstrateConfig, BulletinAtBlock>,
) -> Result<(), BulletinSubmitError> {
    match in_block.wait_for_success().await {
        Ok(events) => {
            for event in events.iter() {
                let event =
                    event.map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
                if event.pallet_name() == "System" && event.event_name() == "ExtrinsicSuccess" {
                    return Ok(());
                }
            }
            Err(BulletinSubmitError::DispatchOutcomeMissing)
        }
        Err(TransactionEventsError::ExtrinsicFailed(error)) => Err(classify_dispatch_error(error)),
        Err(other) => Err(BulletinSubmitError::Subxt(Box::new(other.into()))),
    }
}

/// Map a dispatch failure to the submission error, singling out the
/// allowance-rejection module errors that a key refresh can fix.
fn classify_dispatch_error(error: DispatchError) -> BulletinSubmitError {
    let DispatchError::Module(module_error) = &error else {
        return BulletinSubmitError::Subxt(Box::new(TransactionEventsError::from(error).into()));
    };
    match module_error.details() {
        Ok(details) => {
            if details.pallet.name() == STORE_PALLET_NAME
                && ALLOWANCE_REJECTED_MODULE_ERRORS.contains(&details.variant.name.as_str())
            {
                BulletinSubmitError::AllowanceRejected {
                    phase: AllowanceRejectionPhase::Dispatch,
                }
            } else {
                BulletinSubmitError::Subxt(Box::new(TransactionEventsError::from(error).into()))
            }
        }
        Err(_) => BulletinSubmitError::Subxt(Box::new(TransactionEventsError::from(error).into())),
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
        let unverified = BulletinSubmitError::Subxt(Box::new(
            TransactionStatusError::Invalid("bad nonce".to_string()).into(),
        ));
        assert!(unverified.is_retryable_submission_uncertain(SubmissionPhase::Watch));
        assert!(!unverified.is_retryable_submission_uncertain(SubmissionPhase::Events));

        assert!(
            !BulletinSubmitError::Timeout {
                phase: SubmissionPhase::Watch
            }
            .is_retryable_submission_uncertain(SubmissionPhase::Watch)
        );
        assert!(
            !BulletinSubmitError::Timeout {
                phase: SubmissionPhase::DryRun
            }
            .is_retryable_submission_uncertain(SubmissionPhase::DryRun)
        );

        let inconsistent_inclusion = BulletinSubmitError::Subxt(Box::new(
            TransactionEventsError::CannotFindTransactionInBlock {
                block_hash: [1_u8; 32].into(),
                transaction_hash: [2_u8; 32].into(),
            }
            .into(),
        ));
        assert!(inconsistent_inclusion.is_retryable_submission_uncertain(SubmissionPhase::Events));
        assert!(!inconsistent_inclusion.is_retryable_submission_uncertain(SubmissionPhase::DryRun));
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
        assert!(matches!(
            classify_dispatch_error(module_error("AuthorizationNotFound")),
            BulletinSubmitError::AllowanceRejected {
                phase: AllowanceRejectionPhase::Dispatch
            }
        ));

        let other = non_allowance_error_name();
        let error = classify_dispatch_error(module_error(&other));
        let BulletinSubmitError::Subxt(error) = error else {
            panic!("non-allowance dispatch error must remain a Subxt error");
        };
        assert!(matches!(
            error.as_ref(),
            subxt::Error::TransactionEventsError(TransactionEventsError::ExtrinsicFailed(_))
        ));
    }

    #[test]
    fn error_reason_strings_are_stable() {
        assert_eq!(
            BulletinSubmitError::AllowanceRejected {
                phase: AllowanceRejectionPhase::DryRun
            }
            .to_string(),
            "allowance rejected: dry-run"
        );
        assert_eq!(
            BulletinSubmitError::InvalidTransaction(TransactionInvalid::Future).to_string(),
            "invalid: Future"
        );
        assert_eq!(
            BulletinSubmitError::Timeout {
                phase: SubmissionPhase::Watch
            }
            .to_string(),
            "timeout: watch, inclusion unverified"
        );
        assert_eq!(
            BulletinSubmitError::Timeout {
                phase: SubmissionPhase::Connect
            }
            .to_string(),
            "timeout: connect"
        );
        assert_eq!(
            BulletinSubmitError::Subxt(Box::new(
                TransactionStatusError::Invalid("bad nonce".to_string()).into(),
            ))
            .to_string(),
            "subxt: The transaction is not valid: bad nonce"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod orchestration {
        use super::*;
        use crate::chain_runtime::{RuntimeChainProvider, RuntimeFailure};
        use crate::host_logic::extrinsic::tests::BULLETIN_METADATA_BYTES;
        use crate::subscription::thread_per_subscription_spawner;
        use async_trait::async_trait;
        use futures::StreamExt;
        use futures::channel::mpsc;
        use futures::stream::BoxStream;
        use parity_scale_codec::{Compact, Encode};
        use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive};
        use serde_json::{Value as JsonValue, json};
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex};
        use subxt::events::Phase;
        use subxt::ext::scale_encode::{EncodeAsFields, Field};
        use subxt::ext::scale_value::{Primitive, Value as ScaleValue};
        use truapi_platform::JsonRpcConnection;

        const FOLLOW_ID: &str = "bulletin-follow";
        const BLOCK_HASH: &str =
            "0x1111111111111111111111111111111111111111111111111111111111111111";
        const INCLUDED_HASH: &str =
            "0x2222222222222222222222222222222222222222222222222222222222222222";

        #[derive(Clone, Copy)]
        enum TransactionOutcome {
            Included,
            IncludedWithMissingBody,
            Invalid,
            Dropped,
        }

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum ValidationOutcome {
            Valid,
            AllowanceRejected,
        }

        struct ScriptedState {
            transaction_outcomes: VecDeque<TransactionOutcome>,
            validation_outcomes: VecDeque<ValidationOutcome>,
            next_operation: usize,
            next_transaction: usize,
            next_best_block: usize,
            current_best_hash: String,
            advance_best_block_after_rejection: bool,
            stall_headers: bool,
            last_transaction: Option<String>,
            omit_transaction_from_next_body: bool,
        }

        struct BulletinScriptedProvider {
            state: Arc<Mutex<ScriptedState>>,
            sent: Arc<Mutex<Vec<String>>>,
            sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
            receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
            events: String,
        }

        impl BulletinScriptedProvider {
            fn new(outcomes: impl IntoIterator<Item = TransactionOutcome>) -> Self {
                Self::with_options(outcomes, false)
            }

            fn with_options(
                outcomes: impl IntoIterator<Item = TransactionOutcome>,
                stall_headers: bool,
            ) -> Self {
                let (sender, receiver) = mpsc::unbounded();
                Self {
                    state: Arc::new(Mutex::new(ScriptedState {
                        transaction_outcomes: outcomes.into_iter().collect(),
                        validation_outcomes: VecDeque::new(),
                        next_operation: 0,
                        next_transaction: 0,
                        next_best_block: 0,
                        current_best_hash: BLOCK_HASH.to_string(),
                        advance_best_block_after_rejection: false,
                        stall_headers,
                        last_transaction: None,
                        omit_transaction_from_next_body: false,
                    })),
                    sent: Arc::new(Mutex::new(Vec::new())),
                    sender: Arc::new(Mutex::new(Some(sender))),
                    receiver: Mutex::new(Some(receiver)),
                    events: format!("0x{}", hex::encode(success_events())),
                }
            }

            fn with_validation_outcomes(
                self,
                outcomes: impl IntoIterator<Item = ValidationOutcome>,
                advance_best_block_after_rejection: bool,
            ) -> Self {
                {
                    let mut state = self.state.lock().unwrap();
                    state.validation_outcomes = outcomes.into_iter().collect();
                    state.advance_best_block_after_rejection = advance_best_block_after_rejection;
                }
                self
            }

            fn method_count(&self, method: &str) -> usize {
                self.sent
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|request| {
                        serde_json::from_str::<JsonValue>(request)
                            .ok()
                            .and_then(|value| {
                                value
                                    .get("method")
                                    .and_then(JsonValue::as_str)
                                    .map(ToOwned::to_owned)
                            })
                            .as_deref()
                            == Some(method)
                    })
                    .count()
            }

            fn runtime_call_count(&self, runtime_method: &str) -> usize {
                self.sent
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|request| {
                        serde_json::from_str::<JsonValue>(request)
                            .ok()
                            .and_then(|value| {
                                (value.get("method").and_then(JsonValue::as_str)
                                    == Some("chainHead_v1_call"))
                                .then(|| {
                                    value
                                        .get("params")
                                        .and_then(|params| params.get(2))
                                        .and_then(JsonValue::as_str)
                                        .map(ToOwned::to_owned)
                                })
                                .flatten()
                            })
                            .as_deref()
                            == Some(runtime_method)
                    })
                    .count()
            }
        }

        struct BulletinScriptedConnection {
            state: Arc<Mutex<ScriptedState>>,
            sent: Arc<Mutex<Vec<String>>>,
            sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
            receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
            events: String,
        }

        impl JsonRpcConnection for BulletinScriptedConnection {
            fn send(&self, request: String) {
                self.sent.lock().unwrap().push(request.clone());
                let frames =
                    scripted_frames(&request, &self.events, &mut self.state.lock().unwrap());
                if let Some(sender) = self.sender.lock().unwrap().as_ref() {
                    for frame in frames {
                        sender.unbounded_send(frame).unwrap();
                    }
                }
            }

            fn responses(&self) -> BoxStream<'static, String> {
                self.receiver
                    .lock()
                    .unwrap()
                    .take()
                    .expect("responses called once")
                    .boxed()
            }

            fn close(&self) {
                self.sender.lock().unwrap().take();
            }
        }

        #[async_trait]
        impl RuntimeChainProvider for BulletinScriptedProvider {
            async fn connect(
                &self,
                _genesis_hash: Vec<u8>,
            ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
                Ok(Arc::new(BulletinScriptedConnection {
                    state: self.state.clone(),
                    sent: self.sent.clone(),
                    sender: self.sender.clone(),
                    receiver: Mutex::new(self.receiver.lock().unwrap().take()),
                    events: self.events.clone(),
                }))
            }
        }

        fn scripted_frames(request: &str, events: &str, state: &mut ScriptedState) -> Vec<String> {
            let request: JsonValue = serde_json::from_str(request).unwrap();
            let id = request.get("id").cloned().unwrap_or(JsonValue::Null);
            let method = request["method"].as_str().unwrap();
            let response = |result: JsonValue| {
                json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
            };
            let follow_event = |result: JsonValue| {
                json!({
                    "jsonrpc": "2.0",
                    "method": "chainHead_v1_followEvent",
                    "params": {"subscription": FOLLOW_ID, "result": result}
                })
                .to_string()
            };

            match method {
                "chainHead_v1_follow" => vec![
                    response(json!(FOLLOW_ID)),
                    follow_event(json!({
                        "event": "initialized",
                        "finalizedBlockHashes": [BLOCK_HASH],
                        "finalizedBlockRuntime": null
                    })),
                ],
                "chainHead_v1_header" if state.stall_headers => Vec::new(),
                "chainHead_v1_header" => vec![response(json!(encoded_header()))],
                "chainHead_v1_call" => {
                    state.next_operation += 1;
                    let operation_id = format!("call-{}", state.next_operation);
                    let runtime_method = request["params"][2].as_str().unwrap();
                    let validation_outcome =
                        if runtime_method == "TaggedTransactionQueue_validate_transaction" {
                            state
                                .validation_outcomes
                                .pop_front()
                                .unwrap_or(ValidationOutcome::Valid)
                        } else {
                            ValidationOutcome::Valid
                        };
                    let output = runtime_call_output(runtime_method, validation_outcome);
                    let mut frames = vec![
                        response(json!({"result": "started", "operationId": operation_id})),
                        follow_event(json!({
                            "event": "operationCallDone",
                            "operationId": operation_id,
                            "output": format!("0x{}", hex::encode(output))
                        })),
                    ];
                    if validation_outcome == ValidationOutcome::AllowanceRejected
                        && state.advance_best_block_after_rejection
                    {
                        state.next_best_block += 1;
                        let block_hash = scripted_block_hash(state.next_best_block);
                        let parent_block_hash =
                            std::mem::replace(&mut state.current_best_hash, block_hash.clone());
                        frames.push(follow_event(json!({
                            "event": "newBlock",
                            "blockHash": block_hash,
                            "parentBlockHash": parent_block_hash,
                            "newRuntime": null
                        })));
                        frames.push(follow_event(json!({
                            "event": "bestBlockChanged",
                            "bestBlockHash": block_hash
                        })));
                    }
                    frames
                }
                "transactionWatch_v1_submitAndWatch" => {
                    state.next_transaction += 1;
                    state.last_transaction = request["params"][0].as_str().map(ToOwned::to_owned);
                    let subscription_id = format!("tx-{}", state.next_transaction);
                    let outcome = state
                        .transaction_outcomes
                        .pop_front()
                        .expect("scripted transaction outcome");
                    let status = match outcome {
                        TransactionOutcome::Included => json!({
                            "event": "bestChainBlockIncluded",
                            "block": {"hash": INCLUDED_HASH, "index": "0"}
                        }),
                        TransactionOutcome::IncludedWithMissingBody => {
                            state.omit_transaction_from_next_body = true;
                            json!({
                                "event": "bestChainBlockIncluded",
                                "block": {"hash": INCLUDED_HASH, "index": "0"}
                            })
                        }
                        TransactionOutcome::Invalid => {
                            json!({"event": "invalid", "error": "scripted invalid"})
                        }
                        TransactionOutcome::Dropped => {
                            json!({"event": "dropped", "error": "scripted dropped"})
                        }
                    };
                    vec![
                        response(json!(subscription_id)),
                        json!({
                            "jsonrpc": "2.0",
                            "method": "transactionWatch_v1_watchEvent",
                            "params": {"subscription": subscription_id, "result": status}
                        })
                        .to_string(),
                    ]
                }
                "chainHead_v1_body" => {
                    state.next_operation += 1;
                    let operation_id = format!("body-{}", state.next_operation);
                    let transaction = state
                        .last_transaction
                        .clone()
                        .expect("submitted transaction available");
                    let transactions = if std::mem::take(&mut state.omit_transaction_from_next_body)
                    {
                        vec![]
                    } else {
                        vec![transaction]
                    };
                    vec![
                        response(json!({"result": "started", "operationId": operation_id})),
                        follow_event(json!({
                            "event": "operationBodyDone",
                            "operationId": operation_id,
                            "value": transactions
                        })),
                    ]
                }
                "chainHead_v1_storage" => {
                    state.next_operation += 1;
                    let operation_id = format!("storage-{}", state.next_operation);
                    let key = request["params"][2][0]["key"].clone();
                    vec![
                        response(json!({"result": "started", "operationId": operation_id})),
                        follow_event(json!({
                            "event": "operationStorageItems",
                            "operationId": operation_id,
                            "items": [{"key": key, "value": events}]
                        })),
                        follow_event(json!({
                            "event": "operationStorageDone",
                            "operationId": operation_id
                        })),
                    ]
                }
                "chainHead_v1_unpin" | "chainHead_v1_unfollow" | "transactionWatch_v1_unwatch" => {
                    vec![response(JsonValue::Null)]
                }
                other => {
                    vec![json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": format!("unexpected method {other}")}
                })
                .to_string()]
                }
            }
        }

        fn encoded_header() -> String {
            let mut bytes = Vec::new();
            [0u8; 32].encode_to(&mut bytes);
            Compact(1u32).encode_to(&mut bytes);
            [0u8; 32].encode_to(&mut bytes);
            [0u8; 32].encode_to(&mut bytes);
            Vec::<u8>::new().encode_to(&mut bytes);
            format!("0x{}", hex::encode(bytes))
        }

        fn scripted_block_hash(index: usize) -> String {
            format!("0x{index:064x}")
        }

        fn runtime_call_output(method: &str, validation_outcome: ValidationOutcome) -> Vec<u8> {
            match method {
                "Core_version" => (
                    "bulletin",
                    "bulletin",
                    1u32,
                    1u32,
                    1u32,
                    Vec::<([u8; 8], u32)>::new(),
                    1u32,
                )
                    .encode(),
                "Metadata_metadata_versions" => vec![14u32].encode(),
                "Metadata_metadata_at_version" => {
                    let mut output = vec![1];
                    Compact(u32::try_from(BULLETIN_METADATA_BYTES.len()).unwrap())
                        .encode_to(&mut output);
                    output.extend_from_slice(BULLETIN_METADATA_BYTES);
                    output
                }
                "AccountNonceApi_account_nonce" => 0u32.encode(),
                "TaggedTransactionQueue_validate_transaction"
                    if validation_outcome == ValidationOutcome::Valid =>
                {
                    let mut output = vec![0];
                    0u64.encode_to(&mut output);
                    Vec::<Vec<u8>>::new().encode_to(&mut output);
                    Vec::<Vec<u8>>::new().encode_to(&mut output);
                    64u64.encode_to(&mut output);
                    true.encode_to(&mut output);
                    output
                }
                "TaggedTransactionQueue_validate_transaction" => {
                    // Result::Err(TransactionValidityError::Invalid(
                    // TransactionInvalid::Payment)).
                    vec![1, 0, 1]
                }
                other => panic!("unexpected runtime call {other}"),
            }
        }

        fn success_events() -> Vec<u8> {
            let metadata = ArcMetadata::from(bulletin_metadata());
            let system = metadata.pallet_by_name("System").unwrap();
            let event = system
                .event_variants()
                .unwrap()
                .iter()
                .find(|event| event.name == "ExtrinsicSuccess")
                .unwrap();
            let values = ScaleValue::unnamed_composite(
                event
                    .fields
                    .iter()
                    .map(|field| default_value(metadata.types(), field.ty.id)),
            );
            let mut fields = event
                .fields
                .iter()
                .map(|field| Field::new(field.ty.id, field.name.as_deref()));

            let mut bytes = Vec::new();
            Compact(1u32).encode_to(&mut bytes);
            Phase::ApplyExtrinsic(0).encode_to(&mut bytes);
            system.event_index().encode_to(&mut bytes);
            event.index.encode_to(&mut bytes);
            values
                .encode_as_fields_to(&mut fields, metadata.types(), &mut bytes)
                .unwrap();
            Vec::<[u8; 32]>::new().encode_to(&mut bytes);
            bytes
        }

        fn default_value(types: &PortableRegistry, type_id: u32) -> ScaleValue {
            let ty = types.resolve(type_id).expect("metadata type exists");
            match &ty.type_def {
                TypeDef::Composite(composite) => ScaleValue::unnamed_composite(
                    composite
                        .fields
                        .iter()
                        .map(|field| default_value(types, field.ty.id)),
                ),
                TypeDef::Variant(variants) => {
                    let variant = variants.variants.first().expect("variant exists");
                    ScaleValue::unnamed_variant(
                        variant.name.clone(),
                        variant
                            .fields
                            .iter()
                            .map(|field| default_value(types, field.ty.id)),
                    )
                }
                TypeDef::Sequence(_) => ScaleValue::unnamed_composite([]),
                TypeDef::Array(array) => ScaleValue::unnamed_composite(
                    (0..array.len).map(|_| default_value(types, array.type_param.id)),
                ),
                TypeDef::Tuple(tuple) => ScaleValue::unnamed_composite(
                    tuple
                        .fields
                        .iter()
                        .map(|field| default_value(types, field.id)),
                ),
                TypeDef::Primitive(primitive) => match primitive {
                    TypeDefPrimitive::Bool => ScaleValue::bool(false),
                    TypeDefPrimitive::Char => ScaleValue::char('\0'),
                    TypeDefPrimitive::Str => ScaleValue::string(""),
                    TypeDefPrimitive::U8
                    | TypeDefPrimitive::U16
                    | TypeDefPrimitive::U32
                    | TypeDefPrimitive::U64
                    | TypeDefPrimitive::U128 => ScaleValue::u128(0),
                    TypeDefPrimitive::U256 => ScaleValue::primitive(Primitive::U256([0; 32])),
                    TypeDefPrimitive::I8
                    | TypeDefPrimitive::I16
                    | TypeDefPrimitive::I32
                    | TypeDefPrimitive::I64
                    | TypeDefPrimitive::I128 => ScaleValue::i128(0),
                    TypeDefPrimitive::I256 => ScaleValue::primitive(Primitive::I256([0; 32])),
                },
                TypeDef::Compact(_) => ScaleValue::u128(0),
                TypeDef::BitSequence(_) => {
                    ScaleValue::bit_sequence(subxt::ext::scale_bits::Bits::new())
                }
            }
        }

        fn allowance_fixture() -> BulletinAllowanceKey {
            BulletinAllowanceKey::from_secret_bytes(
                hex::decode(
                    "0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
                     573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
                )
                .unwrap(),
            )
            .unwrap()
        }

        fn rpc(provider: Arc<BulletinScriptedProvider>) -> BulletinRpc {
            BulletinRpc::new(
                ChainRuntime::new(provider, thread_per_subscription_spawner()),
                [0x42; 32],
            )
        }

        #[test]
        fn submit_preimage_drives_subxt_happy_path() {
            let provider = Arc::new(BulletinScriptedProvider::new([
                TransactionOutcome::Included,
            ]));
            let value = b"scripted bulletin happy path";
            let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                value,
            ))
            .unwrap();

            assert_eq!(result, preimage_key(value));
            assert_eq!(
                provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"),
                1
            );
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                1
            );
            assert_eq!(provider.method_count("chainHead_v1_storage"), 1);
        }

        #[test]
        fn submit_preimage_retries_uncertain_broadcast_once() {
            let provider = Arc::new(BulletinScriptedProvider::new([
                TransactionOutcome::Dropped,
                TransactionOutcome::Included,
            ]));
            let value = b"scripted bulletin retry";
            let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                value,
            ))
            .unwrap();

            assert_eq!(result, preimage_key(value));
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                2
            );
        }

        #[test]
        fn submit_preimage_retries_inconsistent_inclusion_once() {
            let provider = Arc::new(BulletinScriptedProvider::new([
                TransactionOutcome::IncludedWithMissingBody,
                TransactionOutcome::Included,
            ]));
            let value = b"scripted bulletin inconsistent inclusion";
            let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                value,
            ))
            .unwrap();

            assert_eq!(result, preimage_key(value));
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                2
            );
            assert_eq!(provider.method_count("chainHead_v1_body"), 2);
        }

        #[test]
        fn submit_preimage_does_not_infer_allowance_rejection_from_authoring_invalidity() {
            let provider = Arc::new(BulletinScriptedProvider::new([
                TransactionOutcome::Invalid,
                TransactionOutcome::Invalid,
            ]));
            let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                b"scripted stale allowance",
            ))
            .unwrap_err();

            let BulletinSubmitError::Subxt(error) = error else {
                panic!("invalid authoring status must remain a Subxt error");
            };
            assert!(matches!(
                error.as_ref(),
                subxt::Error::TransactionStatusError(TransactionStatusError::Invalid(message))
                    if message == "scripted invalid"
            ));
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                2
            );
            assert_eq!(
                provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"),
                2
            );
        }

        #[test]
        fn submit_preimage_classifies_allowance_only_from_the_follow_up_dry_run() {
            let provider = Arc::new(
                BulletinScriptedProvider::new([TransactionOutcome::Invalid])
                    .with_validation_outcomes(
                        [
                            ValidationOutcome::Valid,
                            ValidationOutcome::AllowanceRejected,
                        ],
                        false,
                    ),
            );
            let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                b"scripted stale allowance",
            ))
            .unwrap_err();

            assert!(matches!(
                error,
                BulletinSubmitError::AllowanceRejected {
                    phase: AllowanceRejectionPhase::DryRun,
                }
            ));
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                1
            );
            assert_eq!(
                provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"),
                2
            );
        }

        #[test]
        fn submit_preimage_budget_reports_pre_broadcast_phase() {
            let provider = Arc::new(BulletinScriptedProvider::with_options([], true));
            let error = futures::executor::block_on(rpc(provider).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_millis(25),
                &allowance_fixture(),
                b"timeout",
            ))
            .unwrap_err();

            assert!(matches!(
                &error,
                BulletinSubmitError::Timeout {
                    phase: SubmissionPhase::Connect
                }
            ));
            assert_eq!(error.to_string(), "timeout: connect");
        }

        #[test]
        fn dry_run_rebuilds_at_a_new_best_block_then_submits() {
            let provider = Arc::new(
                BulletinScriptedProvider::new([TransactionOutcome::Included])
                    .with_validation_outcomes(
                        [
                            ValidationOutcome::AllowanceRejected,
                            ValidationOutcome::Valid,
                        ],
                        true,
                    ),
            );
            let value = b"scripted allowance propagation";
            let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                value,
            ))
            .unwrap();

            assert_eq!(result, preimage_key(value));
            assert_eq!(
                provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"),
                2
            );
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                1
            );
        }

        #[test]
        fn dry_run_propagation_block_wait_timeout_remains_an_allowance_rejection() {
            let provider = Arc::new(
                BulletinScriptedProvider::new([])
                    .with_validation_outcomes([ValidationOutcome::AllowanceRejected], false),
            );
            let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                b"scripted propagation block wait timeout",
            ))
            .unwrap_err();

            assert!(matches!(
                error,
                BulletinSubmitError::AllowanceRejected {
                    phase: AllowanceRejectionPhase::DryRun,
                }
            ));
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                0
            );
        }

        #[test]
        fn dry_run_propagation_stops_after_the_block_limit() {
            let provider = Arc::new(BulletinScriptedProvider::new([]).with_validation_outcomes(
                [ValidationOutcome::AllowanceRejected; ALLOWANCE_DRY_RUN_PROPAGATION_BLOCKS + 1],
                true,
            ));
            let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
                &CallContext::default(),
                Instant::now() + Duration::from_secs(2),
                &allowance_fixture(),
                b"scripted propagation block limit",
            ))
            .unwrap_err();

            assert!(matches!(
                error,
                BulletinSubmitError::AllowanceRejected {
                    phase: AllowanceRejectionPhase::DryRun,
                }
            ));
            assert_eq!(
                provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"),
                ALLOWANCE_DRY_RUN_PROPAGATION_BLOCKS + 1
            );
            assert_eq!(
                provider.method_count("transactionWatch_v1_submitAndWatch"),
                0
            );
        }
    }
}
