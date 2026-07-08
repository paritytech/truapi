//! In-core Bulletin preimage submission over the shared chainHead runtime.
//!
//! One submission at a time: build + sign the `TransactionStorage.store`
//! extrinsic offline, dry-run it via `TaggedTransactionQueue_validate_transaction`
//! (broadcast is spec-guaranteed silent on invalid transactions, so the
//! dry-run is the only deterministic error signal), broadcast, then watch
//! best blocks for inclusion and read the dispatch outcome from
//! `System.Events`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use parity_scale_codec::{Decode, Encode};
use subxt::metadata::ArcMetadata;
use subxt::tx::{TransactionInvalid, TransactionUnknown};
use tracing::{instrument, warn};
use truapi::CallContext;
use truapi::v01::{
    OperationStartedResult, RemoteChainHeadBodyRequest, RemoteChainHeadCallRequest,
    RemoteChainHeadFollowItem, RemoteChainHeadFollowRequest, RemoteChainHeadHeaderRequest,
    RemoteChainHeadStorageRequest, RemoteChainHeadUnpinRequest,
    RemoteChainTransactionBroadcastRequest, RemoteChainTransactionStopRequest, RuntimeSpec,
    StorageQueryItem, StorageQueryType,
};
use truapi_platform::BulletinAllowanceKey;

use crate::chain_runtime::{
    ChainRuntime, wait_for_chain_head_call_output, wait_for_chain_head_initialized,
    wait_for_chain_head_storage_value,
};
use crate::host_logic::bulletin::{
    MortalityAnchor, build_signed_store_extrinsic, preimage_key, system_events_storage_key,
};
use crate::host_logic::extrinsic::{
    DecodedDispatchError, ExtrinsicOutcome, OfflineChainState, TransactionValidity,
    best_supported_metadata_version, decode_header_block_number, decode_runtime_metadata,
    decode_transaction_validity, extrinsic_outcome_from_events,
    validate_transaction_call_parameters,
};

/// Whole-submission budget, matching the host-side timeout this flow replaces.
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(120);
/// Budget for the follow to deliver `Initialized`.
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
/// Budget for one pre-broadcast runtime call or storage read.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(20);
/// Delay before retrying an operation start that hit `LimitReached`, and
/// before retrying an inaccessible `System.Events` read.
const OPERATION_RETRY_DELAY: Duration = Duration::from_millis(300);
/// Attempts for reading `System.Events` at the inclusion block.
const EVENTS_READ_ATTEMPTS: usize = 3;

/// Monotonic salt for per-submit bulletin follow ids.
static BULLETIN_FOLLOW_COUNTER: AtomicU64 = AtomicU64::new(1);

/// `TransactionStorage` module errors that mean the allowance account itself
/// was rejected (missing or exhausted authorization), i.e. the one condition
/// where refreshing the allowance key and retrying can help.
const ALLOWANCE_REJECTED_MODULE_ERRORS: &[&str] =
    &["AuthorizationNotFound", "PermanentAllowanceExceeded"];

/// Where a submission failed, for phase-tagged timeout reasons.
type Phase = &'static str;

/// Typed submission failure driving the retry decision at the runtime call
/// site. Wire mapping stays `v01::PreimageSubmitError::Unknown { reason }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BulletinSubmitError {
    /// Connection, follow, metadata, nonce, or anchor plumbing failed.
    ChainUnavailable { reason: String },
    /// The dry-run rejected the transaction for a non-allowance reason.
    InvalidTransaction { kind: String },
    /// The allowance account was rejected; refresh + one retry may help.
    AllowanceRejected { phase: AllowanceRejectionPhase },
    /// The dry-run saw a nonce race (`Future`/`Stale`); no refresh.
    NonceRace,
    /// The server had no free broadcast slot; retryable, nothing was sent.
    BroadcastSlotUnavailable,
    /// The 120 s budget elapsed; `phase` names the step reached.
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
            Self::BroadcastSlotUnavailable => "broadcast slot unavailable: retry".to_string(),
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
    /// Serializes submissions: one broadcast + one ephemeral follow at a
    /// time, and no same-account nonce races between concurrent submits.
    submit_lock: futures::lock::Mutex<()>,
    /// Decoded metadata for the current spec version.
    metadata_cache: StdMutex<Option<(u32, ArcMetadata)>>,
    /// Live broadcast operation id, stopped on every exit path.
    active_broadcast: StdMutex<Option<String>>,
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
            metadata_cache: StdMutex::new(None),
            active_broadcast: StdMutex::new(None),
            phase: StdMutex::new("connect"),
        }
    }

    /// Submit `value` as a Bulletin preimage signed by `allowance`, returning
    /// the preimage key once the transaction is included and its dispatch
    /// succeeded.
    #[instrument(skip_all, fields(runtime.method = "bulletin_rpc.submit_preimage"))]
    pub(crate) async fn submit_preimage(
        &self,
        cx: &CallContext,
        allowance: &BulletinAllowanceKey,
        value: Vec<u8>,
    ) -> Result<Vec<u8>, BulletinSubmitError> {
        // Serialize submissions, keeping the lock wait cancellable.
        let lock = self.submit_lock.lock().fuse();
        let lock_cancelled = cx.cancel().cancelled().fuse();
        pin_mut!(lock, lock_cancelled);
        let _guard = futures::select! {
            guard = lock => guard,
            _ = lock_cancelled => return Err(BulletinSubmitError::Cancelled),
        };

        // The budget starts once the lock is held; dropping the flow on
        // timeout/cancel drops its follow (releasing pins), and the explicit
        // stop below covers any live broadcast on every exit path.
        let flow = self.submit_flow(allowance, value).fuse();
        let timeout = futures_timer::Delay::new(SUBMIT_TIMEOUT).fuse();
        let cancelled = cx.cancel().cancelled().fuse();
        pin_mut!(flow, timeout, cancelled);
        let result = futures::select! {
            result = flow => result,
            () = timeout => Err(BulletinSubmitError::Timeout { phase: self.current_phase() }),
            _ = cancelled => Err(BulletinSubmitError::Cancelled),
        };
        self.stop_active_broadcast().await;
        result
    }

    async fn submit_flow(
        &self,
        allowance: &BulletinAllowanceKey,
        value: Vec<u8>,
    ) -> Result<Vec<u8>, BulletinSubmitError> {
        let key = preimage_key(&value);

        self.enter_phase("connect");
        let follow_id = format!(
            "truapi:bulletin:{}",
            BULLETIN_FOLLOW_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let mut follow = self.chain.remote_chain_head_follow(
            follow_id.clone(),
            RemoteChainHeadFollowRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                with_runtime: true,
            },
        );
        let (finalized_hash, runtime_spec) =
            wait_for_chain_head_initialized(&mut follow, "Bulletin", INITIALIZATION_TIMEOUT)
                .await
                .map_err(|reason| BulletinSubmitError::ChainUnavailable { reason })?;

        self.enter_phase("metadata");
        let state = self
            .offline_chain_state(&follow_id, &mut follow, &finalized_hash, runtime_spec)
            .await?;

        self.enter_phase("build");
        let anchor = self.mortality_anchor(&follow_id, &finalized_hash).await?;
        let nonce = self
            .account_nonce(&follow_id, &mut follow, &finalized_hash, allowance)
            .await?;
        let signed = build_signed_store_extrinsic(&state, &anchor, allowance, nonce, value)
            .map_err(|reason| BulletinSubmitError::InvalidTransaction { kind: reason })?;

        self.enter_phase("dry-run");
        self.dry_run(&follow_id, &mut follow, &finalized_hash, &signed.extrinsic)
            .await?;

        self.enter_phase("broadcast");
        let extrinsic_hash = signed.extrinsic_hash;
        let broadcast = self
            .chain
            .remote_chain_transaction_broadcast(RemoteChainTransactionBroadcastRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                transaction: signed.extrinsic,
            })
            .await
            .map_err(|failure| BulletinSubmitError::ChainUnavailable {
                reason: failure.reason(),
            })?;
        let Some(operation_id) = broadcast.operation_id else {
            return Err(BulletinSubmitError::BroadcastSlotUnavailable);
        };
        *self
            .active_broadcast
            .lock()
            .expect("broadcast slot poisoned") = Some(operation_id);

        self.enter_phase("watch");
        let (inclusion_block, extrinsic_index) = self
            .watch_for_inclusion(&follow_id, &mut follow, extrinsic_hash, nonce, allowance)
            .await?;

        self.enter_phase("events");
        self.require_dispatch_success(
            &follow_id,
            &mut follow,
            &state,
            &anchor,
            &inclusion_block,
            extrinsic_index,
        )
        .await?;

        Ok(key.to_vec())
    }

    /// Fetch (or reuse) decoded metadata for the follow's runtime spec and
    /// assemble the offline chain state. The genesis hash is deliberately the
    /// configured one, never provider-echoed.
    async fn offline_chain_state(
        &self,
        follow_id: &str,
        follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
        finalized_hash: &[u8],
        runtime_spec: RuntimeSpec,
    ) -> Result<OfflineChainState, BulletinSubmitError> {
        let spec_version = runtime_spec.spec_version;
        let transaction_version = runtime_spec.transaction_version.ok_or_else(|| {
            BulletinSubmitError::ChainUnavailable {
                reason: "runtime spec lacks a transaction version".to_string(),
            }
        })?;

        let cached = self
            .metadata_cache
            .lock()
            .expect("metadata cache poisoned")
            .clone();
        let metadata = match cached {
            Some((cached_spec, metadata)) if cached_spec == spec_version => metadata,
            _ => {
                let versions = self
                    .runtime_call(
                        follow_id,
                        follow,
                        finalized_hash,
                        "Metadata_metadata_versions",
                        Vec::new(),
                    )
                    .await?;
                let version = best_supported_metadata_version(&versions)
                    .map_err(|reason| BulletinSubmitError::ChainUnavailable { reason })?;
                let response = self
                    .runtime_call(
                        follow_id,
                        follow,
                        finalized_hash,
                        "Metadata_metadata_at_version",
                        version.encode(),
                    )
                    .await?;
                let metadata = ArcMetadata::from(
                    decode_runtime_metadata(&response)
                        .map_err(|reason| BulletinSubmitError::ChainUnavailable { reason })?,
                );
                *self.metadata_cache.lock().expect("metadata cache poisoned") =
                    Some((spec_version, metadata.clone()));
                metadata
            }
        };

        Ok(OfflineChainState {
            genesis_hash: self.genesis_hash,
            spec_version,
            transaction_version,
            metadata,
        })
    }

    /// Resolve the finalized anchor block's number for the mortal era.
    async fn mortality_anchor(
        &self,
        follow_id: &str,
        finalized_hash: &[u8],
    ) -> Result<MortalityAnchor, BulletinSubmitError> {
        let response = self
            .chain
            .remote_chain_head_header(RemoteChainHeadHeaderRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                follow_subscription_id: follow_id.to_string(),
                hash: finalized_hash.to_vec(),
            })
            .await
            .map_err(|failure| BulletinSubmitError::ChainUnavailable {
                reason: failure.reason(),
            })?;
        let header = response
            .header
            .ok_or_else(|| BulletinSubmitError::ChainUnavailable {
                reason: "anchor block header unavailable".to_string(),
            })?;
        let number = decode_header_block_number(&header)
            .map_err(|reason| BulletinSubmitError::ChainUnavailable { reason })?;
        let hash =
            finalized_hash
                .try_into()
                .map_err(|_| BulletinSubmitError::ChainUnavailable {
                    reason: "anchor block hash is not 32 bytes".to_string(),
                })?;
        Ok(MortalityAnchor { number, hash })
    }

    /// Read the allowance account's next nonce at the anchor block.
    async fn account_nonce(
        &self,
        follow_id: &str,
        follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
        finalized_hash: &[u8],
        allowance: &BulletinAllowanceKey,
    ) -> Result<u64, BulletinSubmitError> {
        let account =
            crate::host_logic::extrinsic::public_key_from_secret_bytes(allowance.as_secret_bytes())
                .map_err(|reason| BulletinSubmitError::InvalidTransaction { kind: reason })?;
        let output = self
            .runtime_call(
                follow_id,
                follow,
                finalized_hash,
                "AccountNonceApi_account_nonce",
                account.to_vec(),
            )
            .await?;
        let nonce =
            u32::decode(&mut &output[..]).map_err(|err| BulletinSubmitError::ChainUnavailable {
                reason: format!("invalid account nonce response: {err}"),
            })?;
        Ok(nonce.into())
    }

    /// Dry-run the signed extrinsic against the anchor block. Broadcast never
    /// reports invalid transactions, so this is the only deterministic signal
    /// for stale allowances, nonce races, and encoding errors.
    async fn dry_run(
        &self,
        follow_id: &str,
        follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
        finalized_hash: &[u8],
        extrinsic: &[u8],
    ) -> Result<(), BulletinSubmitError> {
        let parameters = validate_transaction_call_parameters(extrinsic, finalized_hash);
        let output = self
            .runtime_call(
                follow_id,
                follow,
                finalized_hash,
                "TaggedTransactionQueue_validate_transaction",
                parameters,
            )
            .await?;
        let validity = decode_transaction_validity(&output)
            .map_err(|reason| BulletinSubmitError::ChainUnavailable { reason })?;
        match validity {
            TransactionValidity::Valid(_) => Ok(()),
            TransactionValidity::Invalid(
                TransactionInvalid::Payment
                | TransactionInvalid::Custom(_)
                | TransactionInvalid::BadSigner,
            ) => Err(BulletinSubmitError::AllowanceRejected {
                phase: AllowanceRejectionPhase::DryRun,
            }),
            TransactionValidity::Invalid(
                TransactionInvalid::Future | TransactionInvalid::Stale,
            ) => Err(BulletinSubmitError::NonceRace),
            TransactionValidity::Invalid(other) => Err(BulletinSubmitError::InvalidTransaction {
                kind: format!("{other:?}"),
            }),
            TransactionValidity::Unknown(TransactionUnknown::CannotLookup) => {
                Err(BulletinSubmitError::ChainUnavailable {
                    reason: "transaction validity could not be looked up".to_string(),
                })
            }
            TransactionValidity::Unknown(other) => Err(BulletinSubmitError::InvalidTransaction {
                kind: format!("{other:?}"),
            }),
        }
    }

    /// Watch best/finalized blocks until the broadcast extrinsic appears in a
    /// body. Runs as one event loop over the follow stream so no block event
    /// is lost while a chainHead operation is in flight; block bodies are
    /// only fetched once the allowance account's nonce is seen to advance.
    async fn watch_for_inclusion(
        &self,
        follow_id: &str,
        follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
        extrinsic_hash: [u8; 32],
        our_nonce: u64,
        allowance: &BulletinAllowanceKey,
    ) -> Result<(Vec<u8>, u32), BulletinSubmitError> {
        let account =
            crate::host_logic::extrinsic::public_key_from_secret_bytes(allowance.as_secret_bytes())
                .expect("validated earlier in the submit flow");

        let stopped = || BulletinSubmitError::BroadcastUnverified {
            reason: "chain follow stopped".to_string(),
        };

        // Parent links learned from NewBlock events, used to walk from a
        // nonce-advanced block back to already-checked ancestors without
        // extra header fetches.
        let mut parents: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        // Blocks whose bodies were checked (or skipped as inaccessible).
        let mut checked: HashSet<Vec<u8>> = HashSet::new();
        // Blocks waiting for a nonce gate check.
        let mut gate_queue: VecDeque<Vec<u8>> = VecDeque::new();
        // Blocks that passed the gate and await a body check.
        let mut body_queue: VecDeque<Vec<u8>> = VecDeque::new();
        let mut pending_nonce: Option<(String, Vec<u8>)> = None;
        let mut pending_body: Option<(String, Vec<u8>)> = None;

        loop {
            // Start at most one operation at a time, bodies first.
            if pending_nonce.is_none() && pending_body.is_none() {
                if let Some(block) = body_queue.pop_front() {
                    match self.start_body(follow_id, &block).await? {
                        Some(operation_id) => pending_body = Some((operation_id, block)),
                        None => body_queue.push_front(block),
                    }
                } else if let Some(block) = gate_queue.pop_front() {
                    match self.start_nonce_probe(follow_id, &block, &account).await? {
                        Some(operation_id) => pending_nonce = Some((operation_id, block)),
                        None => gate_queue.push_front(block),
                    }
                }
            }

            let item = follow.next().await.ok_or_else(stopped)?;
            match item {
                RemoteChainHeadFollowItem::Stop => return Err(stopped()),
                RemoteChainHeadFollowItem::NewBlock {
                    block_hash,
                    parent_block_hash,
                    ..
                } => {
                    parents.insert(block_hash, parent_block_hash);
                }
                RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash } => {
                    if !checked.contains(&best_block_hash) {
                        gate_queue.push_back(best_block_hash);
                    }
                }
                RemoteChainHeadFollowItem::Finalized {
                    finalized_block_hashes,
                    pruned_block_hashes,
                } => {
                    for hash in finalized_block_hashes {
                        if !checked.contains(&hash) && !gate_queue.contains(&hash) {
                            gate_queue.push_back(hash);
                        }
                    }
                    self.unpin(follow_id, pruned_block_hashes).await;
                }
                RemoteChainHeadFollowItem::OperationCallDone {
                    operation_id,
                    output,
                } if pending_nonce
                    .as_ref()
                    .is_some_and(|(id, _)| *id == operation_id) =>
                {
                    let (_, block) = pending_nonce.take().expect("checked above");
                    let nonce = u32::decode(&mut &output[..]).map_err(|err| {
                        BulletinSubmitError::BroadcastUnverified {
                            reason: format!("invalid nonce probe response: {err}"),
                        }
                    })?;
                    if u64::from(nonce) > our_nonce {
                        // The account advanced at or before `block`: check its
                        // body and those of its unchecked ancestors.
                        for hash in ancestors_to_check(&parents, &checked, block) {
                            if !body_queue.contains(&hash) {
                                body_queue.push_back(hash);
                            }
                        }
                    } else {
                        // This block does not contain our tx; release its pin.
                        checked.insert(block.clone());
                        self.unpin(follow_id, vec![block]).await;
                    }
                }
                RemoteChainHeadFollowItem::OperationBodyDone {
                    operation_id,
                    value,
                } if pending_body
                    .as_ref()
                    .is_some_and(|(id, _)| *id == operation_id) =>
                {
                    let (_, block) = pending_body.take().expect("checked above");
                    let matched = value.iter().position(|extrinsic| {
                        sp_crypto_hashing::blake2_256(extrinsic) == extrinsic_hash
                    });
                    if let Some(index) = matched {
                        return Ok((block, index as u32));
                    }
                    checked.insert(block.clone());
                    self.unpin(follow_id, vec![block]).await;
                }
                RemoteChainHeadFollowItem::OperationInaccessible { operation_id }
                | RemoteChainHeadFollowItem::OperationError { operation_id, .. } => {
                    if pending_nonce
                        .as_ref()
                        .is_some_and(|(id, _)| *id == operation_id)
                    {
                        let (_, block) = pending_nonce.take().expect("checked above");
                        checked.insert(block);
                    } else if pending_body
                        .as_ref()
                        .is_some_and(|(id, _)| *id == operation_id)
                    {
                        let (_, block) = pending_body.take().expect("checked above");
                        checked.insert(block);
                    }
                }
                _ => {}
            }
        }
    }

    /// Read `System.Events` at the inclusion block and require an
    /// `ExtrinsicSuccess` for our extrinsic index.
    async fn require_dispatch_success(
        &self,
        follow_id: &str,
        follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
        state: &OfflineChainState,
        anchor: &MortalityAnchor,
        inclusion_block: &[u8],
        extrinsic_index: u32,
    ) -> Result<(), BulletinSubmitError> {
        let unverified = |reason: String| BulletinSubmitError::BroadcastUnverified { reason };

        let key = system_events_storage_key();
        let mut events_bytes = None;
        for _attempt in 0..EVENTS_READ_ATTEMPTS {
            let response = self
                .chain
                .remote_chain_head_storage(RemoteChainHeadStorageRequest {
                    genesis_hash: self.genesis_hash.to_vec(),
                    follow_subscription_id: follow_id.to_string(),
                    hash: inclusion_block.to_vec(),
                    items: vec![StorageQueryItem {
                        key: key.clone(),
                        query_type: StorageQueryType::Value,
                    }],
                    child_trie: None,
                })
                .await
                .map_err(|failure| unverified(failure.reason()))?;
            let operation_id = match response.operation {
                OperationStartedResult::Started { operation_id } => operation_id,
                OperationStartedResult::LimitReached => {
                    futures_timer::Delay::new(OPERATION_RETRY_DELAY).await;
                    continue;
                }
            };
            // The block executed our extrinsic, so System.Events is never
            // absent there: a missing value means the read was inaccessible.
            match wait_for_chain_head_storage_value(
                follow,
                &operation_id,
                &key,
                "Bulletin",
                OPERATION_TIMEOUT,
            )
            .await
            .map_err(unverified)?
            {
                Some(bytes) => {
                    events_bytes = Some(bytes);
                    break;
                }
                None => futures_timer::Delay::new(OPERATION_RETRY_DELAY).await,
            }
        }
        let Some(events_bytes) = events_bytes else {
            return Err(unverified(
                "included, dispatch outcome unavailable".to_string(),
            ));
        };

        let client = state
            .client_at(anchor.number)
            .map_err(|reason| unverified(format!("events decoding unavailable: {reason}")))?;
        match extrinsic_outcome_from_events(&client, events_bytes, extrinsic_index)
            .map_err(unverified)?
        {
            ExtrinsicOutcome::Success => Ok(()),
            ExtrinsicOutcome::Failed(DecodedDispatchError::Module { pallet, error })
                if pallet == "TransactionStorage"
                    && ALLOWANCE_REJECTED_MODULE_ERRORS.contains(&error.as_str()) =>
            {
                Err(BulletinSubmitError::AllowanceRejected {
                    phase: AllowanceRejectionPhase::Dispatch,
                })
            }
            ExtrinsicOutcome::Failed(DecodedDispatchError::Module { pallet, error }) => {
                Err(BulletinSubmitError::IncludedButFailed { pallet, error })
            }
            ExtrinsicOutcome::Failed(DecodedDispatchError::Other(reason)) => {
                Err(BulletinSubmitError::IncludedButFailed {
                    pallet: "unknown".to_string(),
                    error: reason,
                })
            }
            ExtrinsicOutcome::NotFound => Err(unverified(
                "included, but the block reported no dispatch outcome".to_string(),
            )),
        }
    }

    /// Start one runtime-call operation at `hash`, retrying `LimitReached`
    /// once, and wait for its output.
    async fn runtime_call(
        &self,
        follow_id: &str,
        follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
        hash: &[u8],
        function: &str,
        call_parameters: Vec<u8>,
    ) -> Result<Vec<u8>, BulletinSubmitError> {
        let mut attempts = 0;
        let operation_id = loop {
            attempts += 1;
            let response = self
                .chain
                .remote_chain_head_call(RemoteChainHeadCallRequest {
                    genesis_hash: self.genesis_hash.to_vec(),
                    follow_subscription_id: follow_id.to_string(),
                    hash: hash.to_vec(),
                    function: function.to_string(),
                    call_parameters: call_parameters.clone(),
                })
                .await
                .map_err(|failure| BulletinSubmitError::ChainUnavailable {
                    reason: failure.reason(),
                })?;
            match response.operation {
                OperationStartedResult::Started { operation_id } => break operation_id,
                OperationStartedResult::LimitReached if attempts < 2 => {
                    futures_timer::Delay::new(OPERATION_RETRY_DELAY).await;
                }
                OperationStartedResult::LimitReached => {
                    return Err(BulletinSubmitError::ChainUnavailable {
                        reason: format!("{function}: chainHead operation limit reached"),
                    });
                }
            }
        };
        wait_for_chain_head_call_output(follow, &operation_id, "Bulletin", OPERATION_TIMEOUT)
            .await
            .map_err(|reason| BulletinSubmitError::ChainUnavailable { reason })
    }

    /// Start a nonce probe at `block`; `Ok(None)` means the operation limit
    /// was reached and the caller should retry on the next event.
    async fn start_nonce_probe(
        &self,
        follow_id: &str,
        block: &[u8],
        account: &[u8; 32],
    ) -> Result<Option<String>, BulletinSubmitError> {
        let response = self
            .chain
            .remote_chain_head_call(RemoteChainHeadCallRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                follow_subscription_id: follow_id.to_string(),
                hash: block.to_vec(),
                function: "AccountNonceApi_account_nonce".to_string(),
                call_parameters: account.to_vec(),
            })
            .await
            .map_err(|failure| BulletinSubmitError::BroadcastUnverified {
                reason: failure.reason(),
            })?;
        Ok(match response.operation {
            OperationStartedResult::Started { operation_id } => Some(operation_id),
            OperationStartedResult::LimitReached => None,
        })
    }

    /// Start a body fetch at `block`; `Ok(None)` means the operation limit
    /// was reached and the caller should retry on the next event.
    async fn start_body(
        &self,
        follow_id: &str,
        block: &[u8],
    ) -> Result<Option<String>, BulletinSubmitError> {
        let response = self
            .chain
            .remote_chain_head_body(RemoteChainHeadBodyRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                follow_subscription_id: follow_id.to_string(),
                hash: block.to_vec(),
            })
            .await
            .map_err(|failure| BulletinSubmitError::BroadcastUnverified {
                reason: failure.reason(),
            })?;
        Ok(match response.operation {
            OperationStartedResult::Started { operation_id } => Some(operation_id),
            OperationStartedResult::LimitReached => None,
        })
    }

    /// Release pins for blocks the watch no longer needs; failures only warn.
    async fn unpin(&self, follow_id: &str, hashes: Vec<Vec<u8>>) {
        if hashes.is_empty() {
            return;
        }
        if let Err(failure) = self
            .chain
            .remote_chain_head_unpin(RemoteChainHeadUnpinRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                follow_subscription_id: follow_id.to_string(),
                hashes,
            })
            .await
        {
            warn!(reason = %failure.reason(), "Bulletin block unpin failed");
        }
    }

    /// Stop the live broadcast, if any. Called on every submit exit path.
    async fn stop_active_broadcast(&self) {
        let operation_id = self
            .active_broadcast
            .lock()
            .expect("broadcast slot poisoned")
            .take();
        let Some(operation_id) = operation_id else {
            return;
        };
        if let Err(failure) = self
            .chain
            .remote_chain_transaction_stop(RemoteChainTransactionStopRequest {
                genesis_hash: self.genesis_hash.to_vec(),
                operation_id,
            })
            .await
        {
            warn!(reason = %failure.reason(), "Bulletin broadcast stop failed");
        }
    }

    fn enter_phase(&self, phase: Phase) {
        *self.phase.lock().expect("phase slot poisoned") = phase;
    }

    fn current_phase(&self) -> Phase {
        *self.phase.lock().expect("phase slot poisoned")
    }
}

/// Collect `start` and its not-yet-checked ancestors from a provider-supplied
/// `parents` map, oldest lookups first.
///
/// `parents` comes from untrusted `NewBlock` follow events, so the walk guards
/// against self-parent and cyclic links with a visited set: without it a
/// crafted `parent == block` link would spin forever, and since the enclosing
/// watch loop never `.await`s inside the walk, the whole worker would freeze
/// and hold the submit lock permanently.
fn ancestors_to_check(
    parents: &HashMap<Vec<u8>, Vec<u8>>,
    checked: &HashSet<Vec<u8>>,
    start: Vec<u8>,
) -> Vec<Vec<u8>> {
    let mut cursor = start.clone();
    let mut visited: HashSet<Vec<u8>> = HashSet::from([start.clone()]);
    let mut walk = vec![start];
    while let Some(parent) = parents.get(&cursor) {
        if checked.contains(parent) || !visited.insert(parent.clone()) {
            break;
        }
        walk.push(parent.clone());
        cursor = parent.clone();
    }
    walk
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> Vec<u8> {
        vec![byte]
    }

    #[test]
    fn ancestors_walk_collects_unchecked_chain() {
        // c -> b -> a, none checked: walk collects all three newest-first.
        let parents = HashMap::from([(h(3), h(2)), (h(2), h(1))]);
        let checked = HashSet::new();
        assert_eq!(
            ancestors_to_check(&parents, &checked, h(3)),
            vec![h(3), h(2), h(1)]
        );
    }

    #[test]
    fn ancestors_walk_stops_at_checked_parent() {
        let parents = HashMap::from([(h(3), h(2)), (h(2), h(1))]);
        let checked = HashSet::from([h(2)]);
        assert_eq!(ancestors_to_check(&parents, &checked, h(3)), vec![h(3)]);
    }

    #[test]
    fn ancestors_walk_terminates_on_self_parent() {
        // A crafted self-referential parent must not loop forever.
        let parents = HashMap::from([(h(5), h(5))]);
        let checked = HashSet::new();
        assert_eq!(ancestors_to_check(&parents, &checked, h(5)), vec![h(5)]);
    }

    #[test]
    fn ancestors_walk_terminates_on_cycle() {
        // A -> B -> A cycle must terminate once it wraps around.
        let parents = HashMap::from([(h(1), h(2)), (h(2), h(1))]);
        let checked = HashSet::new();
        assert_eq!(
            ancestors_to_check(&parents, &checked, h(1)),
            vec![h(1), h(2)]
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
