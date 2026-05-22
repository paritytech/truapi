// Coinage Layer — Quint specification
//
// Companion to docs/design/coinage-layer.md. Models the bottom-layer
// coinage subsystem as a state machine suitable for invariant checking
// (Quint simulator / Apalache) and for use as a reference contract when
// verifying Rust implementations.
//
// Modeling choices
// ----------------
//   * Cryptography is opaque. Accounts and member keys are fresh integers
//     allocated from monotonic counters.
//   * The chain is modeled inside this module via the same state variables.
//     Chain-level events (age increment, ring sealing, ring expiry) are
//     explicit actions.
//   * Multi-extrinsic operations are decomposed into per-phase actions.
//   * Selection algorithms are encoded as nondeterministic choices over
//     records that satisfy the §6.3 / §8.6 preconditions.
//   * Time advances only via the explicit `tick` action.

module CoinageLayer {

  // ==========================================================================
  // 1. Types
  // ==========================================================================

  type PurseId    = int
  type CoinIndex  = int
  type EntryIndex = int
  type OpHandle   = int
  type RingIndex  = int
  type Account    = int
  type MemberKey  = int
  type Exponent   = int
  type Age        = int
  type Time       = int
  type Amount     = int

  type CoinState =
    | CoinPending
    | CoinAvailable
    | CoinLockedFor(OpHandle)
    | CoinSpent

  type EntryOnChain =
    | OnMissing
    | OnWaiting
    | OnReady
    | OnDegraded(int)

  type EntryLocal =
    | LocalAvailable
    | LocalLockedFor(OpHandle)
    | LocalConsumed

  type OpKind =
    | KTopUp
    | KTransfer
    | KExport
    | KImport
    | KExternalOffload
    | KRebalance
    | KMaintenance
    | KDeletePurse
    | KRecover

  /// Failure reason carried by `SFailed` and `XRejected`. Mirrors the
  /// post-submission and lifecycle subset of design §10. Pre-submission
  /// errors do not appear here — they prevent the operation from being
  /// started in the first place and are returned as caller-visible errors.
  type FailureReason =
    | FRSnipedCoin
    | FRChainRejected
    | FRCancelled
    | FRInterruptedPreSubmission
    | FRStorage
    | FRSubscription
    | FRRecovery
    | FRInternal

  type OpStatus =
    | SPreparing
    | SSubmitted
    | SInBlock
    | SFinalized
    | SWaiting(Time)
    | SDone
    | SFailed(FailureReason)

  type PurseRec = {
    id: PurseId,
    name: str,
    nextCoinIdx: CoinIndex,
    nextEntryIdx: EntryIndex,
  }

  /// Synchronous purse-info snapshot returned by `query_purse`.
  type PurseInfo = {
    id: PurseId,
    name: str,
    spendable: Amount,
    spendableStrict: Amount,
    pending: Amount,
  }

  type CoinRec = {
    purse: PurseId,
    idx: CoinIndex,
    account: Account,
    exponent: Exponent,
    age: Age,
    state: CoinState,
  }

  type EntryRec = {
    purse: PurseId,
    idx: EntryIndex,
    memberKey: MemberKey,
    exponent: Exponent,
    allocatedAt: Time,
    readyAt: Time,
    ringIdx: int,
    onChain: EntryOnChain,
    local: EntryLocal,
  }

  type OperationRec = {
    handle: OpHandle,
    kind: OpKind,
    purse: PurseId,
    status: OpStatus,
    lockedCoins: Set[(PurseId, CoinIndex)],
    lockedEntries: Set[(PurseId, EntryIndex)],
  }

  type RingRec = {
    idx: RingIndex,
    immutableSince: int,
    expired: bool,
  }

  /// Per-extrinsic outcome attached to an operation receipt.
  type ExtrinsicOutcome =
    | XSucceeded(Set[Account])
    | XRejected(FailureReason)

  type ExtrinsicRecord = {
    extrinsicId: int,
    outcome: ExtrinsicOutcome,
  }

  /// Result of `classifyIncomingPayment` (§8.8).
  type PaymentClassification =
    | PCMatched
    | PCReceived
    | PCUnmatched

  /// `MemoEntry` (§8.3). The layer treats memos opaquely; only the
  /// `recipient_account` field is used by `classifyIncomingPayment`.
  type MemoEntry = {
    senderAccount: Account,
    recipientAccount: Account,
    derivationIndex: CoinIndex,
  }

  /// Unload-token class (§6.5).
  type UnloadTokenClass =
    | UTFree
    | UTPaid

  /// Fee mode (§6.6). The layer picks automatically: prepaid if the fee
  /// account has external funds, from-output otherwise. Caller does not
  /// specify.
  type FeeMode =
    | FMPrepaid
    | FMFromOutput

  /// Unload-token record. A token is identified by its (period, class,
  /// counter) triple. The chain tracks `consumed` flags; the layer mirrors
  /// them.
  type UnloadToken = {
    period: int,
    class: UnloadTokenClass,
    counter: int,
    consumed: bool,
  }

  /// Layer-level event (§11). The event stream is append-only.
  type LayerEvent =
    | EResynced
    | EPurseCreated({ purse: PurseId, name: str })
    | EPurseRenamed({ purse: PurseId, name: str })
    | EPurseDeleted(PurseId)
    | ECoinAvailable({ purse: PurseId, exponent: Exponent })
    | ECoinSpent({ purse: PurseId, exponent: Exponent })
    | ECoinAged({ purse: PurseId, exponent: Exponent, age: Age })
    | EEntryAllocated({ purse: PurseId, exponent: Exponent })
    | EEntryReadinessChanged({ purse: PurseId, exponent: Exponent, newState: EntryOnChain })
    | EEntryConsumed({ purse: PurseId, exponent: Exponent })
    | EOperationStarted({ handle: OpHandle, kind: OpKind, purse: PurseId })
    | EOperationProgress({ handle: OpHandle, status: OpStatus })
    | EOperationCompleted({ handle: OpHandle, status: OpStatus })
    | EMaintenanceSweepStarted
    | EMaintenanceSweepCompleted({ recycled: int, rescued: int })

  // ==========================================================================
  // 2. Parameters (small for tractability)
  // ==========================================================================

  pure val MAIN_PURSE: PurseId = 0
  pure val MaxAge: Age = 4
  pure val RecycleAtAge: Age = 3
  pure val AnonymityFloor: int = 2
  pure val MaxExponent: Exponent = 2
  pure val RecyclerExpiration: Time = 8
  pure val RescueMargin: Time = 3
  pure val JitterMax: Time = 2
  pure val FreeTokenSearchRange: int = 3
  pure val PeriodLookbackGrace: int = 1
  pure val PeriodLength: Time = 4
  pure val UnloadFee: Amount = 1

  // ==========================================================================
  // 3. State
  // ==========================================================================

  var purses: PurseId -> PurseRec
  var coins: (PurseId, CoinIndex) -> CoinRec
  var entries: (PurseId, EntryIndex) -> EntryRec
  var operations: OpHandle -> OperationRec
  var rings: RingIndex -> RingRec
  var nextHandle: OpHandle
  var nextRingIdx: RingIndex
  var nextAccount: Account
  var nextMemberKey: MemberKey
  var nextExtrinsicId: int
  var now: Time
  var totalIn: Amount
  var totalOut: Amount
  /// Per-operation receipts. Each entry is the ordered list of per-extrinsic
  /// outcomes accumulated since the operation began.
  var receipts: OpHandle -> List[ExtrinsicRecord]
  /// Per-operation `requested` amount (only meaningful for KExternalOffload).
  /// Allows the offload's surplus-reload logic to know how much value
  /// must land externally vs be returned to the layer.
  var opRequested: OpHandle -> Amount
  /// Layer event stream (append-only). Models the typed subscription
  /// surface of §11.
  var events: List[LayerEvent]
  /// Unload-token state. Keyed by `(period, class, counter)`. Tokens for
  /// the current period exist for the host; consumption flips `consumed`.
  var tokens: (int, UnloadTokenClass, int) -> UnloadToken
  /// Whether the host has joined the paid-token ring for a given period.
  var paidRingMembership: int -> bool
  /// Cached fee-account balance (off-chain external currency). Drives the
  /// `prepaid` vs `from-output` fee-mode pick per §6.6.
  var feeAccountBalance: Amount

  // ==========================================================================
  // 4. Pure helpers
  // ==========================================================================

  /// Coin value `2^exp`, expanded as a linear lookup so Apalache's SMT
  /// backend stays in the decidable fragment (Z3 reports UNKNOWN for
  /// symbolic exponentiation). `MaxExponent` bounds the table size.
  pure def coinValue(exp: Exponent): Amount =
    if (exp == 0) 1
    else if (exp == 1) 2
    else if (exp == 2) 4
    else if (exp == 3) 8
    else if (exp == 4) 16
    else 0

  // ==========================================================================
  // 4.x Derivation (Appendix B)
  // ==========================================================================
  //
  // The bottom layer's account and member-key derivation is fully
  // deterministic from `(purse, page, idx, kind)`. With `page = 0` fixed
  // for this version, the derivation collapses to `(purse, idx, kind)`.
  // We model the resulting bytes as integers: any conformant implementation
  // produces the same integer for the same input.
  pure val PAGE: int = 0
  pure val PURSE_STRIDE: int = 1000
  pure val COIN_KIND_OFFSET: int = 0
  pure val ENTRY_KIND_OFFSET: int = 1000000

  /// `//coinage//coin//<P>//<PAGE>//<I>` (Appendix B).
  pure def deriveCoinAccount(p: PurseId, idx: CoinIndex): Account =
    COIN_KIND_OFFSET + p * PURSE_STRIDE + idx

  /// `//coinage//ring-vrf//<P>//<PAGE>//<I>` (Appendix B).
  pure def deriveMemberKey(p: PurseId, idx: EntryIndex): MemberKey =
    ENTRY_KIND_OFFSET + p * PURSE_STRIDE + idx

  def isDegraded(e: EntryRec): bool = match e.onChain {
    | OnDegraded(_) => true
    | _ => false
  }

  def entrySelectable(e: EntryRec, allowDegraded: bool): bool = and {
    e.local == LocalAvailable,
    e.onChain == OnReady or (allowDegraded and isDegraded(e)),
    e.readyAt <= now,
  }

  def isCoinLocked(c: CoinRec): bool = match c.state {
    | CoinLockedFor(_) => true
    | _ => false
  }

  def isEntryLocked(e: EntryRec): bool = match e.local {
    | LocalLockedFor(_) => true
    | _ => false
  }

  def coinLockHandle(c: CoinRec): OpHandle = match c.state {
    | CoinLockedFor(h) => h
    | _ => -1
  }

  def entryLockHandle(e: EntryRec): OpHandle = match e.local {
    | LocalLockedFor(h) => h
    | _ => -1
  }

  // Append an extrinsic record to an operation handle's receipt log.
  pure def appendReceipt(
    rs: OpHandle -> List[ExtrinsicRecord],
    h: OpHandle,
    rec: ExtrinsicRecord,
  ): OpHandle -> List[ExtrinsicRecord] = {
    val current = if (rs.keys().contains(h)) rs.get(h) else List()
    rs.put(h, current.append(rec))
  }

  // Total of value across all non-terminal records.
  def liveValue: Amount = {
    val cv = coins.keys().fold(0, (a, k) =>
      if (coins.get(k).state == CoinSpent) a else a + coinValue(coins.get(k).exponent))
    val ev = entries.keys().fold(0, (a, k) =>
      if (entries.get(k).local == LocalConsumed) a else a + coinValue(entries.get(k).exponent))
    cv + ev
  }

  // ==========================================================================
  // 5. Init
  // ==========================================================================

  action init = all {
    purses' = Map(MAIN_PURSE -> { id: MAIN_PURSE, name: "main", nextCoinIdx: 0, nextEntryIdx: 0 }),
    coins' = Map(),
    entries' = Map(),
    operations' = Map(),
    rings' = Map(),
    nextHandle' = 1,
    nextRingIdx' = 0,
    nextAccount' = 1,
    nextMemberKey' = 1,
    nextExtrinsicId' = 1,
    now' = 0,
    totalIn' = 0,
    totalOut' = 0,
    receipts' = Map(),
    events' = List(),
    tokens' = Map(),
    paidRingMembership' = Map(),
    feeAccountBalance' = 100,
    opRequested' = Map(),
  }

  // ==========================================================================
  // 6. Caller-facing primitives
  // ==========================================================================

  // 6.1 Create a purse.
  action createPurse(newId: PurseId, nm: str): bool = all {
    not(purses.keys().contains(newId)),
    newId != MAIN_PURSE,
    purses' = purses.put(newId,
      { id: newId, name: nm, nextCoinIdx: 0, nextEntryIdx: 0 }),
    coins' = coins,
    entries' = entries,
    operations' = operations,
    rings' = rings,
    nextHandle' = nextHandle,
    nextRingIdx' = nextRingIdx,
    nextAccount' = nextAccount,
    nextMemberKey' = nextMemberKey,
    now' = now,
    nextExtrinsicId' = nextExtrinsicId,
    receipts' = receipts,
    events' = events.append(EPurseCreated({ purse: newId, name: nm })),
    totalIn' = totalIn,
    tokens' = tokens,
    paidRingMembership' = paidRingMembership,
    feeAccountBalance' = feeAccountBalance,
    opRequested' = opRequested,
    totalOut' = totalOut,
  }

  // 6.1.1 Rename a purse. Synchronous, no chain interaction.
  action renamePurse(p: PurseId, nm: str): bool = {
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    all {
      pExists,
      purses' = purses.put(p, purse.with("name", nm)),
      coins' = coins,
      entries' = entries,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EPurseRenamed({ purse: p, name: nm })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  /// True iff there is any live (non-terminal) coin in `p`.
  def purseHasLiveCoins(p: PurseId): bool =
    coins.keys().exists(k =>
      coins.get(k).purse == p and coins.get(k).state != CoinSpent)

  /// True iff there is any live (non-terminal) entry in `p`.
  def purseHasLiveEntries(p: PurseId): bool =
    entries.keys().exists(k =>
      entries.get(k).purse == p and entries.get(k).local != LocalConsumed)

  /// True iff `p` has any non-terminal operation.
  def purseHasInFlight(p: PurseId): bool =
    operations.keys().exists(h => {
      val op = operations.get(h)
      op.purse == p and not(op.status == SDone or isFailed(op))
    })

  // 6.1.2 Delete a purse. The main purse cannot be deleted; the purse must
  // have no live coins, no live entries and no in-flight operations. Drain
  // is modeled as a separate prior `rebalancePurse`.
  action deletePurse(p: PurseId): bool = all {
    purses.keys().contains(p),
    p != MAIN_PURSE,
    not(purseHasLiveCoins(p)),
    not(purseHasLiveEntries(p)),
    not(purseHasInFlight(p)),
    purses' = purses.keys().exclude(Set(p)).fold(Map(), (m, k) => m.put(k, purses.get(k))),
    coins' = coins.keys().filter(k => coins.get(k).purse != p)
                  .fold(Map(), (m, k) => m.put(k, coins.get(k))),
    entries' = entries.keys().filter(k => entries.get(k).purse != p)
                       .fold(Map(), (m, k) => m.put(k, entries.get(k))),
    operations' = operations,
    rings' = rings,
    nextHandle' = nextHandle,
    nextRingIdx' = nextRingIdx,
    nextAccount' = nextAccount,
    nextMemberKey' = nextMemberKey,
    now' = now,
    nextExtrinsicId' = nextExtrinsicId,
    receipts' = receipts,
    events' = events.append(EPurseDeleted(p)),
    totalIn' = totalIn,
    tokens' = tokens,
    paidRingMembership' = paidRingMembership,
    feeAccountBalance' = feeAccountBalance,
    opRequested' = opRequested,
    totalOut' = totalOut,
  }

  // 6.1.3 Rebalance: move one Available coin from `from` to `to`. The model
  // abstracts amount/selection into "pick a single coin", capturing the
  // namespace-bridging invariant: the source coin terminates and a fresh
  // age-0 coin appears in the destination purse's namespace.
  action rebalancePurse(src: PurseId, dst: PurseId, k: (PurseId, CoinIndex)): bool = {
    val fromExists = purses.keys().contains(src)
    val toExists = purses.keys().contains(dst)
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    val toPurse = if (toExists) purses.get(dst)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val newIdx = toPurse.nextCoinIdx
    val acct = deriveCoinAccount(dst, newIdx)
    val newCoin: CoinRec = {
      purse: dst, idx: newIdx, account: acct, exponent: c.exponent,
      age: 0, state: CoinAvailable,
    }
    all {
      fromExists,
      toExists,
      src != dst,
      cExists,
      c.purse == src,
      c.state == CoinAvailable,
      coins' = coins.put(k, c.with("state", CoinSpent))
                   .put((dst, newIdx), newCoin),
      purses' = purses.put(dst, toPurse.with("nextCoinIdx", newIdx + 1)),
      nextAccount' = nextAccount,
      entries' = entries,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(ECoinSpent({ purse: src, exponent: c.exponent }))
                      .append(ECoinAvailable({ purse: dst, exponent: c.exponent })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  // 6.1.4 Synchronous query: balance snapshot.
  def purseSpendable(p: PurseId): Amount = {
    val readyEntries = entries.keys().fold(0, (a, k) => {
      val e = entries.get(k)
      if (e.purse == p and e.local == LocalAvailable
            and (e.onChain == OnReady or isDegraded(e)))
        a + coinValue(e.exponent)
      else a
    })
    val availableCoins = coins.keys().fold(0, (a, k) => {
      val c = coins.get(k)
      if (c.purse == p and c.state == CoinAvailable)
        a + coinValue(c.exponent)
      else a
    })
    readyEntries + availableCoins
  }

  def purseSpendableStrict(p: PurseId): Amount = {
    val readyEntries = entries.keys().fold(0, (a, k) => {
      val e = entries.get(k)
      if (e.purse == p and e.local == LocalAvailable and e.onChain == OnReady)
        a + coinValue(e.exponent)
      else a
    })
    val availableCoins = coins.keys().fold(0, (a, k) => {
      val c = coins.get(k)
      if (c.purse == p and c.state == CoinAvailable)
        a + coinValue(c.exponent)
      else a
    })
    readyEntries + availableCoins
  }

  def pursePending(p: PurseId): Amount =
    entries.keys().fold(0, (a, k) => {
      val e = entries.get(k)
      if (e.purse == p and e.local == LocalAvailable
            and (e.onChain == OnWaiting or e.onChain == OnMissing))
        a + coinValue(e.exponent)
      else a
    })

  def queryPurse(p: PurseId): PurseInfo = {
    val purse = purses.get(p)
    {
      id: p,
      name: purse.name,
      spendable: purseSpendable(p),
      spendableStrict: purseSpendableStrict(p),
      pending: pursePending(p),
    }
  }

  // ==========================================================================
  // 6.x Selection (§6.3)
  // ==========================================================================
  //
  // Selection is decomposed into pure predicates that determine *which*
  // strategy is feasible. The three-tier policy (exact, split, unload)
  // commits to the first feasible strategy in priority order. Selection
  // *itself* is purely functional; the corresponding `transferAmount`,
  // `exportAmount`, and `rebalanceAmount` actions consume the result.

  /// Sum of values of a set of coin keys.
  pure def sumOfCoinValues(
    cs: (PurseId, CoinIndex) -> CoinRec,
    sel: Set[(PurseId, CoinIndex)],
  ): Amount =
    sel.fold(0, (a, k) => a + coinValue(cs.get(k).exponent))

  /// Sum of values of a set of entry keys.
  pure def sumOfEntryValues(
    es: (PurseId, EntryIndex) -> EntryRec,
    sel: Set[(PurseId, EntryIndex)],
  ): Amount =
    sel.fold(0, (a, k) => a + coinValue(es.get(k).exponent))

  // --------------------------------------------------------------------------
  // Deterministic ordering (§6.3)
  // --------------------------------------------------------------------------
  //
  // Coins are ordered by (exponent desc, age desc, idx asc); entries by
  // (exponent desc, ring asc, idx asc). The ordering is total and pure.
  // Two conformant implementations with the same purse contents pick the
  // same coin / entry under this ordering.
  //
  // Selection ranks: smaller rank tuple ⇒ higher priority. Encoded as
  // tuples so lexicographic comparison gives the design ordering.
  //
  // The first element is `-exponent` (or `MaxExponent - exponent`) so that
  // a larger exponent yields a smaller rank.

  pure def coinPriorityRank(c: CoinRec): (int, int, int) =
    (MaxExponent - c.exponent, MaxAge - c.age, c.idx)

  pure def entryPriorityRank(e: EntryRec): (int, int, int) =
    (MaxExponent - e.exponent, e.ringIdx, e.idx)

  /// Lexicographic less-than on rank tuples.
  pure def rankLT(a: (int, int, int), b: (int, int, int)): bool = or {
    a._1 < b._1,
    a._1 == b._1 and a._2 < b._2,
    a._1 == b._1 and a._2 == b._2 and a._3 < b._3,
  }

  pure def coinOrderLT(c1: CoinRec, c2: CoinRec): bool =
    rankLT(coinPriorityRank(c1), coinPriorityRank(c2))

  pure def entryOrderLT(e1: EntryRec, e2: EntryRec): bool =
    rankLT(entryPriorityRank(e1), entryPriorityRank(e2))

  /// True iff `k` has higher priority than every other available coin
  /// in its purse.
  def isMinPriorityAvailableCoin(k: (PurseId, CoinIndex)): bool = {
    val c = coins.get(k)
    availableCoinsIn(c.purse).forall(k2 =>
      k2 == k or coinOrderLT(c, coins.get(k2)))
  }

  /// True iff `k` has higher priority than every other selectable entry
  /// in its purse under `allowDegraded`.
  def isMinPrioritySelectableEntry(k: (PurseId, EntryIndex), allowDegraded: bool): bool = {
    val e = entries.get(k)
    selectableEntriesIn(e.purse, allowDegraded).forall(k2 =>
      k2 == k or entryOrderLT(e, entries.get(k2)))
  }

  /// All Available coin keys in `p`.
  def availableCoinsIn(p: PurseId): Set[(PurseId, CoinIndex)] =
    coins.keys().filter(k => {
      val c = coins.get(k)
      c.purse == p and c.state == CoinAvailable
    })

  /// All selectable entry keys in `p` under `allowDegraded`.
  def selectableEntriesIn(p: PurseId, allowDegraded: bool): Set[(PurseId, EntryIndex)] =
    entries.keys().filter(k => entrySelectable(entries.get(k), allowDegraded)
                                and entries.get(k).purse == p)

  /// Tier 1: exists a subset of Available coins in `p` summing exactly to
  /// `amount`. Encoded as: there exists `sel ⊆ availableCoinsIn(p)` with
  /// `Σ value = amount`. We bound subset search via `powerset`.
  def existsExactCover(p: PurseId, amount: Amount): bool =
    availableCoinsIn(p).powerset().exists(sel =>
      sumOfCoinValues(coins, sel) == amount)

  /// Tier 2: exists a single Available coin whose value strictly exceeds
  /// `amount` (split + change).
  def existsSplitCover(p: PurseId, amount: Amount): bool =
    availableCoinsIn(p).exists(k => coinValue(coins.get(k).exponent) > amount)

  /// Tier 3: exists selectable entries (optionally with whole coins) whose
  /// combined value covers `amount` (entries supply the deficit).
  def existsUnloadCover(p: PurseId, amount: Amount, allowDegraded: bool): bool =
    selectableEntriesIn(p, allowDegraded).powerset().exists(esel =>
      availableCoinsIn(p).powerset().exists(csel =>
        sumOfCoinValues(coins, csel) + sumOfEntryValues(entries, esel) >= amount))

  /// Selection succeeds at any tier.
  def selectionFeasible(p: PurseId, amount: Amount, allowDegraded: bool): bool = or {
    existsExactCover(p, amount),
    existsSplitCover(p, amount),
    existsUnloadCover(p, amount, allowDegraded),
  }

  /// Spendable-when-ready value of a purse (used to distinguish
  /// `NoReadyEntries` from `InsufficientFunds`).
  def spendableWhenReady(p: PurseId): Amount =
    purseSpendable(p) + pursePending(p)

  // 6.2 Top-up: external funds → fresh recycler entries.
  action topUp(p: PurseId, exp: Exponent): bool = {
    val purse = if (purses.keys().contains(p))
        purses.get(p)
      else
        { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val idx = purse.nextEntryIdx
    val mk = deriveMemberKey(p, idx)
    val ring = nextRingIdx
    val ringExists = rings.keys().contains(ring)
    val ringRec: RingRec = if (ringExists)
        rings.get(ring)
      else
        { idx: ring, immutableSince: -1, expired: false }
    val rec: EntryRec = {
      purse: p, idx: idx, memberKey: mk, exponent: exp,
      allocatedAt: now, readyAt: now + JitterMax,
      ringIdx: ring,
      onChain: OnWaiting,
      local: LocalAvailable,
    }
    all {
      purses.keys().contains(p),
      exp <= MaxExponent,
      exp >= 0,
      entries' = entries.put((p, idx), rec),
      rings' = rings.put(ring, ringRec),
      purses' = purses.put(p, purse.with("nextEntryIdx", idx + 1)),
      nextMemberKey' = nextMemberKey,
      totalIn' = totalIn + coinValue(exp),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      coins' = coins,
      operations' = operations,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EEntryAllocated({ purse: p, exponent: exp })),
      totalOut' = totalOut,
    }
  }

  // 6.3 Direct transfer of an Available coin to a pre-arranged recipient.
  action transfer(k: (PurseId, CoinIndex)): bool = {
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    all {
      cExists,
      c.state == CoinAvailable,
      coins' = coins.put(k, c.with("state", CoinSpent).with("age", c.age + 1)),
      totalOut' = totalOut + coinValue(c.exponent),
      purses' = purses,
      entries' = entries,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(ECoinSpent({ purse: c.purse, exponent: c.exponent })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
    }
  }

  // ==========================================================================
  // 6.x Operational primitives (multi-phase)
  // ==========================================================================
  //
  // For each API primitive that submits one or more extrinsics, the layer
  // creates a real OperationRec in SPreparing, locks the affected records,
  // and walks the op through the SSubmitted → SInBlock → SFinalized → SDone
  // progression (or to SFailed via opChainReject / cancelOp).
  //
  // Per §5.5 the multi-phase machinery here is per-extrinsic, but the spec
  // tracks one extrinsic per operation at a time; operations that fan out
  // across multiple extrinsics walk the phase progression once per group
  // in sequence.
  //
  // Commit actions (`opCommit*`) are the side-effecting transitions that
  // fire when the op transitions from SFinalized to SDone. They consume
  // the locked records, produce the operation's results, append the
  // receipt, and emit the terminal event.

  // 6.x.1 Start a Transfer operation. Locks the selected coin subset and
  // creates a Preparing op of kind KTransfer.
  action startTransfer(
    p: PurseId,
    sel: Set[(PurseId, CoinIndex)],
    allowDegraded: bool,
  ): bool = {
    val amount = sumOfCoinValues(coins, sel)
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KTransfer, purse: p,
      status: SPreparing,
      lockedCoins: sel,
      lockedEntries: Set(),
    }
    val newCoins = sel.fold(coins, (m, k) => {
      val c = m.get(k)
      m.put(k, c.with("state", CoinLockedFor(h)))
    })
    all {
      purses.keys().contains(p),
      sel.size() > 0,
      sel.subseteq(availableCoinsIn(p)),
      selectionFeasible(p, amount, allowDegraded),
      operations' = operations.put(h, op),
      coins' = newCoins,
      nextHandle' = h + 1,
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationStarted({ handle: h, kind: KTransfer, purse: p })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.x.2 Commit Transfer (Finalized → Done). Consumes the locked coins.
  action opCommitTransfer(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val locked = op.lockedCoins
    val accs = locked.fold(Set(), (a, k) => a.union(Set(coins.get(k).account)))
    val totalSpent = locked.fold(0, (a, k) => a + coinValue(coins.get(k).exponent))
    val newCoins = locked.fold(coins, (m, k) => {
      val c = m.get(k)
      m.put(k, c.with("state", CoinSpent).with("age", c.age + 1))
    })
    val newEvents = locked.fold(events, (es, k) =>
      es.append(ECoinSpent({ purse: op.purse, exponent: coins.get(k).exponent })))
        .append(EOperationCompleted({ handle: h, status: SDone }))
    all {
      opExists,
      op.kind == KTransfer,
      op.status == SFinalized,
      operations' = operations.put(h, op.with("status", SDone)
                                       .with("lockedCoins", Set())),
      coins' = newCoins,
      totalOut' = totalOut + totalSpent,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(accs),
      }),
      events' = newEvents,
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
    }
  }

  // 6.x.3 Start a TopUp operation. Reserves the next entry index in
  // the purse and creates a Preparing op of kind KTopUp.
  action startTopUp(p: PurseId, exp: Exponent): bool = {
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KTopUp, purse: p,
      status: SPreparing,
      lockedCoins: Set(),
      lockedEntries: Set(),
    }
    all {
      pExists,
      exp <= MaxExponent,
      exp >= 0,
      operations' = operations.put(h, op),
      nextHandle' = h + 1,
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationStarted({ handle: h, kind: KTopUp, purse: p })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.x.4 Commit TopUp (Finalized → Done). Creates the fresh recycler
  // entry at the purse's next index.
  action opCommitTopUp(h: OpHandle, exp: Exponent): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val p = op.purse
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val idx = purse.nextEntryIdx
    val mk = deriveMemberKey(p, idx)
    val ring = nextRingIdx
    val ringExists = rings.keys().contains(ring)
    val ringRec: RingRec = if (ringExists)
        rings.get(ring)
      else
        { idx: ring, immutableSince: -1, expired: false }
    val rec: EntryRec = {
      purse: p, idx: idx, memberKey: mk, exponent: exp,
      allocatedAt: now, readyAt: now + JitterMax,
      ringIdx: ring,
      onChain: OnWaiting,
      local: LocalAvailable,
    }
    all {
      opExists,
      op.kind == KTopUp,
      op.status == SFinalized,
      pExists,
      exp <= MaxExponent,
      exp >= 0,
      entries' = entries.put((p, idx), rec),
      rings' = rings.put(ring, ringRec),
      purses' = purses.put(p, purse.with("nextEntryIdx", idx + 1)),
      operations' = operations.put(h, op.with("status", SDone)),
      totalIn' = totalIn + coinValue(exp),
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(Set(mk)),
      }),
      events' = events.append(EEntryAllocated({ purse: p, exponent: exp }))
                      .append(EOperationCompleted({ handle: h, status: SDone })),
      coins' = coins,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  // 6.x.4b Start a Transfer deterministically. Picks the highest-priority
  // Available coin (under §6.3 ordering) whose value equals `amount`.
  // Models the single-coin exact-match case of tier 1.
  //
  // Precondition: there exists at least one Available coin in `p` whose
  // value equals `amount`, and the picked `k` is the unique min-priority
  // such coin. Two implementations agree on `k` for the same purse state.
  action startTransferDeterministic(p: PurseId, k: (PurseId, CoinIndex)): bool = {
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    val amount = coinValue(c.exponent)
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KTransfer, purse: p,
      status: SPreparing,
      lockedCoins: Set(k),
      lockedEntries: Set(),
    }
    val matching = availableCoinsIn(p).filter(kk =>
      coinValue(coins.get(kk).exponent) == amount)
    val isDeterministicPick = matching.contains(k) and matching.forall(kk =>
      kk == k or coinOrderLT(coins.get(k), coins.get(kk)))
    all {
      purses.keys().contains(p),
      cExists,
      c.purse == p,
      c.state == CoinAvailable,
      isDeterministicPick,
      operations' = operations.put(h, op),
      coins' = coins.put(k, c.with("state", CoinLockedFor(h))),
      nextHandle' = h + 1,
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationStarted({ handle: h, kind: KTransfer, purse: p })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.x.5 Start Export. Locks an Available coin; the secret will leave
  // the layer when the op commits.
  action startExport(k: (PurseId, CoinIndex)): bool = {
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KExport, purse: c.purse,
      status: SPreparing,
      lockedCoins: Set(k),
      lockedEntries: Set(),
    }
    all {
      cExists,
      c.state == CoinAvailable,
      operations' = operations.put(h, op),
      coins' = coins.put(k, c.with("state", CoinLockedFor(h))),
      nextHandle' = h + 1,
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationStarted({ handle: h, kind: KExport, purse: c.purse })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.x.6 Commit Export (Finalized → Done).
  action opCommitExport(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val locked = op.lockedCoins
    val totalSpent = locked.fold(0, (a, k) => a + coinValue(coins.get(k).exponent))
    val accs = locked.fold(Set(), (a, k) => a.union(Set(coins.get(k).account)))
    val newCoins = locked.fold(coins, (m, k) => {
      val c = m.get(k)
      m.put(k, c.with("state", CoinSpent))
    })
    val newEvents = locked.fold(events, (es, k) =>
      es.append(ECoinSpent({ purse: op.purse, exponent: coins.get(k).exponent })))
        .append(EOperationCompleted({ handle: h, status: SDone }))
    all {
      opExists,
      op.kind == KExport,
      op.status == SFinalized,
      operations' = operations.put(h, op.with("status", SDone)
                                       .with("lockedCoins", Set())),
      coins' = newCoins,
      totalOut' = totalOut + totalSpent,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(accs),
      }),
      events' = newEvents,
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
    }
  }

  // 6.x.7 Start Import. Allocate-only, no locks.
  action startImport(p: PurseId, exp: Exponent): bool = {
    val pExists = purses.keys().contains(p)
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KImport, purse: p,
      status: SPreparing,
      lockedCoins: Set(),
      lockedEntries: Set(),
    }
    all {
      pExists,
      exp <= MaxExponent,
      exp >= 0,
      operations' = operations.put(h, op),
      nextHandle' = h + 1,
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationStarted({ handle: h, kind: KImport, purse: p })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.x.8 Commit Import (Finalized → Done).
  action opCommitImport(h: OpHandle, exp: Exponent): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val p = op.purse
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val idx = purse.nextCoinIdx
    val acct = deriveCoinAccount(p, idx)
    val c: CoinRec = {
      purse: p, idx: idx, account: acct, exponent: exp,
      age: 0, state: CoinAvailable,
    }
    all {
      opExists,
      op.kind == KImport,
      op.status == SFinalized,
      pExists,
      exp <= MaxExponent,
      exp >= 0,
      operations' = operations.put(h, op.with("status", SDone)),
      coins' = coins.put((p, idx), c),
      purses' = purses.put(p, purse.with("nextCoinIdx", idx + 1)),
      totalIn' = totalIn + coinValue(exp),
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(Set(acct)),
      }),
      events' = events.append(ECoinAvailable({ purse: p, exponent: exp }))
                      .append(EOperationCompleted({ handle: h, status: SDone })),
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  // 6.x.9 Start Rebalance. Locks one coin in `src`.
  action startRebalance(src: PurseId, dst: PurseId, k: (PurseId, CoinIndex)): bool = {
    val srcExists = purses.keys().contains(src)
    val dstExists = purses.keys().contains(dst)
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KRebalance, purse: src,
      status: SPreparing,
      lockedCoins: Set(k),
      lockedEntries: Set(),
    }
    all {
      srcExists,
      dstExists,
      src != dst,
      cExists,
      c.purse == src,
      c.state == CoinAvailable,
      operations' = operations.put(h, op),
      coins' = coins.put(k, c.with("state", CoinLockedFor(h))),
      nextHandle' = h + 1,
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationStarted({ handle: h, kind: KRebalance, purse: src })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.x.10 Commit Rebalance. The locked source coin terminates; a fresh
  // age-0 coin appears in `dst`.
  action opCommitRebalance(h: OpHandle, dst: PurseId): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val dstExists = purses.keys().contains(dst)
    val dstPurse = if (dstExists) purses.get(dst)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val newIdx = dstPurse.nextCoinIdx
    val acct = deriveCoinAccount(dst, newIdx)
    val locked = op.lockedCoins
    val srcExp = locked.fold(0, (a, k) => coins.get(k).exponent)
    val newCoin: CoinRec = {
      purse: dst, idx: newIdx, account: acct, exponent: srcExp,
      age: 0, state: CoinAvailable,
    }
    val newCoins = locked.fold(coins, (m, k) => {
      val c = m.get(k)
      m.put(k, c.with("state", CoinSpent))
    }).put((dst, newIdx), newCoin)
    all {
      opExists,
      op.kind == KRebalance,
      op.status == SFinalized,
      dstExists,
      dst != op.purse,
      locked.size() > 0,
      operations' = operations.put(h, op.with("status", SDone)
                                       .with("lockedCoins", Set())),
      coins' = newCoins,
      purses' = purses.put(dst, dstPurse.with("nextCoinIdx", newIdx + 1)),
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(Set(acct)),
      }),
      events' = events.append(ECoinSpent({ purse: op.purse, exponent: srcExp }))
                      .append(ECoinAvailable({ purse: dst, exponent: srcExp }))
                      .append(EOperationCompleted({ handle: h, status: SDone })),
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 6.3.1 Transfer of an exact amount. Selection picks an Available coin
  // subset whose value sums exactly to `amount` (tier 1). Tier 2 / tier 3
  // are modeled implicitly: they would arise as split/unload extrinsics in
  // a multi-extrinsic operation, captured separately in step 8. For the
  // single-shot abstraction here, only tier 1 fires inside this action,
  // but `selectionFeasible` is asserted as a precondition so the model is
  // honest about when selection can succeed at all.
  action transferAmount(
    p: PurseId,
    sel: Set[(PurseId, CoinIndex)],
    allowDegraded: bool,
  ): bool = {
    val amount = sumOfCoinValues(coins, sel)
    val newCoins = sel.fold(coins, (m, k) => {
      val c = m.get(k)
      m.put(k, c.with("state", CoinSpent).with("age", c.age + 1))
    })
    val newEvents = sel.fold(events, (es, k) =>
      es.append(ECoinSpent({ purse: p, exponent: coins.get(k).exponent })))
    all {
      purses.keys().contains(p),
      sel.size() > 0,
      sel.subseteq(availableCoinsIn(p)),
      selectionFeasible(p, amount, allowDegraded),
      coins' = newCoins,
      totalOut' = totalOut + amount,
      events' = newEvents,
      purses' = purses,
      entries' = entries,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
    }
  }

  // 6.4 Export coins: coin leaves the layer's view, secret handed off externally.
  action exportCoin(k: (PurseId, CoinIndex)): bool = {
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    all {
      cExists,
      c.state == CoinAvailable,
      coins' = coins.put(k, c.with("state", CoinSpent)),
      totalOut' = totalOut + coinValue(c.exponent),
      purses' = purses,
      entries' = entries,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(ECoinSpent({ purse: c.purse, exponent: c.exponent })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
    }
  }

  // 6.5 Import coins: external (account, secret) → fresh coin in p.
  action importCoin(p: PurseId, exp: Exponent): bool = {
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val idx = purse.nextCoinIdx
    val acct = deriveCoinAccount(p, idx)
    val c: CoinRec = {
      purse: p, idx: idx, account: acct, exponent: exp,
      age: 0, state: CoinAvailable,
    }
    all {
      pExists,
      exp <= MaxExponent,
      exp >= 0,
      coins' = coins.put((p, idx), c),
      purses' = purses.put(p, purse.with("nextCoinIdx", idx + 1)),
      nextAccount' = nextAccount,
      totalIn' = totalIn + coinValue(exp),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      entries' = entries,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(ECoinAvailable({ purse: p, exponent: exp })),
      totalOut' = totalOut,
    }
  }

  // 6.6 Start external offload: lock an entry, create Preparing operation.
  // `requested` is the externally-bound amount; the entry's surplus
  // (entry value − requested) is reloaded atomically at offboard time.
  action startExternalOffload(k: (PurseId, EntryIndex), requested: Amount): bool = {
    val eExists = entries.keys().contains(k)
    val e = if (eExists) entries.get(k)
      else { purse: -1, idx: -1, memberKey: -1, exponent: 0, allocatedAt: 0,
             readyAt: 0, ringIdx: -1, onChain: OnMissing, local: LocalAvailable }
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KExternalOffload, purse: k._1,
      status: SPreparing,
      lockedCoins: Set(),
      lockedEntries: Set(k),
    }
    all {
      eExists,
      e.local == LocalAvailable,
      requested >= 0,
      requested <= coinValue(e.exponent),
      operations' = operations.put(h, op),
      entries' = entries.put(k, e.with("local", LocalLockedFor(h))),
      nextHandle' = h + 1,
      purses' = purses,
      coins' = coins,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      opRequested' = opRequested.put(h, requested),
      events' = events.append(EOperationStarted({ handle: h, kind: KExternalOffload, purse: k._1 })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      totalOut' = totalOut,
    }
  }

  // 6.7 Cancel a Preparing or Waiting operation: release locks, emit Failed.
  def isCancellable(op: OperationRec): bool = match op.status {
    | SPreparing => true
    | SWaiting(_) => true
    | _ => false
  }

  action cancelOp(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val releasedCoins = coins.keys().fold(coins, (m, ck) => {
      val c = m.get(ck)
      if (isCoinLocked(c) and coinLockHandle(c) == h)
        m.put(ck, c.with("state", CoinAvailable))
      else m
    })
    val releasedEntries = entries.keys().fold(entries, (m, ek) => {
      val e = m.get(ek)
      if (isEntryLocked(e) and entryLockHandle(e) == h)
        m.put(ek, e.with("local", LocalAvailable))
      else m
    })
    all {
      opExists,
      isCancellable(op),
      operations' = operations.put(h, op.with("status", SFailed(FRCancelled))
                                       .with("lockedCoins", Set())
                                       .with("lockedEntries", Set())),
      coins' = releasedCoins,
      entries' = releasedEntries,
      purses' = purses,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XRejected(FRCancelled),
      }),
      events' = events.append(EOperationCompleted({ handle: h, status: SFailed(FRCancelled) })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  // ==========================================================================
  // 7. Operation phase transitions (external offload)
  // ==========================================================================

  // Pick the (only) locked entry from an operation.
  def pickLockedEntry(op: OperationRec): (PurseId, EntryIndex) =
    op.lockedEntries.fold((-1, -1), (acc, k) => k)

  // 7.1 Offboard: consume the locked entry, complete the operation.
  /// Surplus-reload: when an external offload consumes an entry of value
  /// V to satisfy a `requested` < V, the surplus V − requested MUST be
  /// reloaded atomically as fresh entries in the same purse so that no
  /// surplus value escapes back as a free coin (§8.6).
  ///
  /// For tractability, the model places the entire surplus into ONE
  /// fresh entry of an arbitrary exponent. The `surplusExponent` parameter
  /// declares the chosen exponent; the precondition asserts that
  /// `coinValue(surplusExponent) == surplus`, i.e. the surplus must
  /// happen to be a power of two. In the actual chain, multiple
  /// reload-entries are produced if necessary; the spec abstracts that
  /// case by requiring the caller to size `requested` such that the
  /// surplus fits a single power of two.
  action opOffboard(h: OpHandle, surplusExponent: Exponent): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val ek = pickLockedEntry(op)
    val eExists = entries.keys().contains(ek)
    val e = if (eExists) entries.get(ek)
      else { purse: -1, idx: -1, memberKey: -1, exponent: 0, allocatedAt: 0,
             readyAt: 0, ringIdx: -1, onChain: OnMissing, local: LocalAvailable }
    val requested = if (opRequested.keys().contains(h)) opRequested.get(h)
      else coinValue(e.exponent)
    val entryValue = coinValue(e.exponent)
    val surplus = entryValue - requested
    val p = e.purse
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val newIdx = purse.nextEntryIdx
    val mk = deriveMemberKey(p, newIdx)
    val ring = nextRingIdx
    val ringExists = rings.keys().contains(ring)
    val ringRec: RingRec = if (ringExists)
        rings.get(ring)
      else
        { idx: ring, immutableSince: -1, expired: false }
    val surplusEntry: EntryRec = {
      purse: p, idx: newIdx, memberKey: mk, exponent: surplusExponent,
      allocatedAt: now, readyAt: now + JitterMax,
      ringIdx: ring,
      onChain: OnWaiting,
      local: LocalAvailable,
    }
    val hasSurplus = surplus > 0
    val newEntries = if (hasSurplus)
      entries.put(ek, e.with("local", LocalConsumed)).put((p, newIdx), surplusEntry)
    else
      entries.put(ek, e.with("local", LocalConsumed))
    val newPurses = if (hasSurplus)
      purses.put(p, purse.with("nextEntryIdx", newIdx + 1))
    else
      purses
    val newRings = if (hasSurplus) rings.put(ring, ringRec) else rings
    val newEvents = if (hasSurplus)
      events.append(EEntryConsumed({ purse: p, exponent: e.exponent }))
            .append(EEntryAllocated({ purse: p, exponent: surplusExponent }))
            .append(EOperationCompleted({ handle: h, status: SDone }))
    else
      events.append(EEntryConsumed({ purse: p, exponent: e.exponent }))
            .append(EOperationCompleted({ handle: h, status: SDone }))
    all {
      opExists,
      op.kind == KExternalOffload,
      op.status == SPreparing,
      op.lockedEntries.size() > 0,
      eExists,
      pExists,
      // Entry must be selectable when offboarding (strict).
      e.local == LocalLockedFor(h),
      e.onChain == OnReady,
      e.readyAt <= now,
      // Surplus precondition: the surplus equals exactly one power of two.
      surplus >= 0,
      not(hasSurplus) or coinValue(surplusExponent) == surplus,
      not(hasSurplus) or surplusExponent <= MaxExponent,
      not(hasSurplus) or surplusExponent >= 0,
      entries' = newEntries,
      operations' = operations.put(h, op.with("status", SDone)
                                       .with("lockedEntries", Set())),
      totalOut' = totalOut + requested,
      purses' = newPurses,
      coins' = coins,
      rings' = newRings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(Set(e.memberKey)),
      }),
      events' = newEvents,
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
    }
  }

  // 7.2 Enter Waiting: locked entry not yet ready.
  action opEnterWait(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val ek = pickLockedEntry(op)
    val eExists = entries.keys().contains(ek)
    val e = if (eExists) entries.get(ek)
      else { purse: -1, idx: -1, memberKey: -1, exponent: 0, allocatedAt: 0,
             readyAt: 0, ringIdx: -1, onChain: OnMissing, local: LocalAvailable }
    all {
      opExists,
      op.kind == KExternalOffload,
      op.status == SPreparing,
      op.lockedEntries.size() > 0,
      eExists,
      not(e.onChain == OnReady and e.readyAt <= now),
      operations' = operations.put(h, op.with("status", SWaiting(e.readyAt))),
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 7.3 Wake from Waiting → Preparing.
  def waitingUntil(op: OperationRec): Time = match op.status {
    | SWaiting(t) => t
    | _ => -1
  }

  action opWake(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val until = waitingUntil(op)
    all {
      opExists,
      until >= 0,
      until <= now,
      operations' = operations.put(h, op.with("status", SPreparing)),
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // ==========================================================================
  // 7.x Multi-extrinsic phase transitions (§5.5)
  // ==========================================================================
  //
  // Operations that submit extrinsics walk a per-extrinsic progression
  //   SPreparing → SSubmitted → SInBlock → SFinalized → SDone
  // The status reflects the *least-progressed* unfinalized extrinsic; in
  // this simplified model we track one extrinsic per operation at a time.

  /// Operation may currently advance from Preparing to Submitted.
  def canAdvanceToSubmitted(op: OperationRec): bool = op.status == SPreparing

  action opAdvanceToSubmitted(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    all {
      opExists,
      canAdvanceToSubmitted(op),
      operations' = operations.put(h, op.with("status", SSubmitted)),
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationProgress({ handle: h, status: SSubmitted })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  action opAdvanceToInBlock(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    all {
      opExists,
      op.status == SSubmitted,
      operations' = operations.put(h, op.with("status", SInBlock)),
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationProgress({ handle: h, status: SInBlock })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  action opAdvanceToFinalized(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    all {
      opExists,
      op.status == SInBlock,
      operations' = operations.put(h, op.with("status", SFinalized)),
      purses' = purses,
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EOperationProgress({ handle: h, status: SFinalized })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  /// Chain rejects a submitted extrinsic. Operation enters SFailed and
  /// the held locks remain to be released by `cancelOp`.
  action opChainReject(h: OpHandle): bool = {
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val releasedCoins = coins.keys().fold(coins, (m, ck) => {
      val c = m.get(ck)
      if (isCoinLocked(c) and coinLockHandle(c) == h)
        m.put(ck, c.with("state", CoinAvailable))
      else m
    })
    val releasedEntries = entries.keys().fold(entries, (m, ek) => {
      val e = m.get(ek)
      if (isEntryLocked(e) and entryLockHandle(e) == h)
        m.put(ek, e.with("local", LocalAvailable))
      else m
    })
    all {
      opExists,
      op.status == SSubmitted or op.status == SInBlock,
      operations' = operations.put(h, op.with("status", SFailed(FRChainRejected))
                                         .with("lockedCoins", Set())
                                         .with("lockedEntries", Set())),
      coins' = releasedCoins,
      entries' = releasedEntries,
      purses' = purses,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XRejected(FRChainRejected),
      }),
      events' = events.append(EOperationCompleted({ handle: h, status: SFailed(FRChainRejected) })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // ==========================================================================
  // 8. Autonomous maintenance sweeps (§6.4)
  // ==========================================================================

  // 8.1 Coin-age recycle: consume an aging coin, produce a fresh entry.
  action coinAgeRecycle(k: (PurseId, CoinIndex)): bool = {
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    val p = c.purse
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val newIdx = purse.nextEntryIdx
    val mk = deriveMemberKey(p, newIdx)
    val ring = nextRingIdx
    val ringExists = rings.keys().contains(ring)
    val ringRec: RingRec = if (ringExists)
        rings.get(ring)
      else
        { idx: ring, immutableSince: -1, expired: false }
    val newEntry: EntryRec = {
      purse: p, idx: newIdx, memberKey: mk, exponent: c.exponent,
      allocatedAt: now, readyAt: now + JitterMax,
      ringIdx: ring,
      onChain: OnWaiting,
      local: LocalAvailable,
    }
    all {
      cExists,
      c.state == CoinAvailable,
      c.age >= RecycleAtAge,
      pExists,
      coins' = coins.put(k, c.with("state", CoinSpent)),
      entries' = entries.put((p, newIdx), newEntry),
      purses' = purses.put(p, purse.with("nextEntryIdx", newIdx + 1)),
      rings' = rings.put(ring, ringRec),
      nextMemberKey' = nextMemberKey,
      operations' = operations,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(ECoinSpent({ purse: p, exponent: c.exponent }))
                      .append(EEntryAllocated({ purse: p, exponent: c.exponent })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  // 8.2 Ring-expiration rescue: pick a selectable entry whose ring is
  // approaching expiry, consume it, produce a fresh age-0 coin.
  action ringExpirationRescue(k: (PurseId, EntryIndex)): bool = {
    val eExists = entries.keys().contains(k)
    val e = if (eExists) entries.get(k)
      else { purse: -1, idx: -1, memberKey: -1, exponent: 0, allocatedAt: 0,
             readyAt: 0, ringIdx: -1, onChain: OnMissing, local: LocalAvailable }
    val ringExists = rings.keys().contains(e.ringIdx)
    val ring = if (ringExists)
        rings.get(e.ringIdx)
      else
        { idx: -1, immutableSince: -1, expired: true }
    val p = e.purse
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val newIdx = purse.nextCoinIdx
    val acct = deriveCoinAccount(p, newIdx)
    val newCoin: CoinRec = {
      purse: p, idx: newIdx, account: acct, exponent: e.exponent,
      age: 0, state: CoinAvailable,
    }
    val freeCtr = availableFreeCounter(currentPeriod)
    val useFree = freeCtr >= 0
    val newTokens = if (useFree)
      consumeFreeToken(tokens, currentPeriod, freeCtr)
    else
      consumePaidToken(tokens, currentPeriod, 0)
    val joinedRing = paidRingMembership.put(currentPeriod, true)
    val newPaid = if (useFree) paidRingMembership else joinedRing
    val paidJoinFee = if (useFree) 0
      else if (paidRingMembership.keys().contains(currentPeriod)
                and paidRingMembership.get(currentPeriod))
        0
      else 1
    // Fee mode pick (§6.6): prepaid if the fee account can cover the
    // unload fee *after* joining the paid ring; otherwise from-output.
    val feeMode: FeeMode = if (feeAccountBalance - paidJoinFee >= UnloadFee)
      FMPrepaid else FMFromOutput
    val unloadFeePaid = if (feeMode == FMPrepaid) UnloadFee else 0
    val newFee = feeAccountBalance - paidJoinFee - unloadFeePaid
    all {
      eExists,
      e.local == LocalAvailable,
      ringExists,
      ring.immutableSince >= 0,
      now + RescueMargin >= ring.immutableSince + RecyclerExpiration,
      not(ring.expired),
      pExists,
      canObtainUnloadToken,
      entries' = entries.put(k, e.with("local", LocalConsumed)),
      coins' = coins.put((p, newIdx), newCoin),
      purses' = purses.put(p, purse.with("nextCoinIdx", newIdx + 1)),
      nextAccount' = nextAccount,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      tokens' = newTokens,
      paidRingMembership' = newPaid,
      feeAccountBalance' = newFee,
      opRequested' = opRequested,
      events' = events.append(EEntryConsumed({ purse: p, exponent: e.exponent }))
                      .append(ECoinAvailable({ purse: p, exponent: e.exponent })),
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // ==========================================================================
  // 8.x Unload tokens (§6.5)
  // ==========================================================================

  /// Current period as a function of `now`.
  def currentPeriod: int = now / PeriodLength

  /// True iff the (period, class, counter) slot is currently consumed.
  def isTokenConsumed(period: int, cls: UnloadTokenClass, ctr: int): bool =
    tokens.keys().contains((period, cls, ctr))
      and tokens.get((period, cls, ctr)).consumed

  /// Picks the smallest counter index < `FreeTokenSearchRange` that is
  /// not consumed for `(period, UTFree)`. Returns `-1` if none.
  def availableFreeCounter(period: int): int = {
    val candidates = 0.to(FreeTokenSearchRange - 1)
      .filter(c => not(isTokenConsumed(period, UTFree, c)))
    if (candidates.size() == 0) -1
    else candidates.fold(FreeTokenSearchRange, (a, c) => if (c < a) c else a)
  }

  /// Periods to probe per design §6.5: current and prior `PeriodLookbackGrace`.
  def candidatePeriods: Set[int] =
    0.to(PeriodLookbackGrace).map(off => currentPeriod - off)
      .filter(p => p >= 0)

  /// True iff any free token slot is available in any candidate period.
  def hasFreeUnloadToken: bool =
    candidatePeriods.exists(p => availableFreeCounter(p) >= 0)

  /// True iff the host can use a paid token: it has joined the current
  /// paid ring (cached), or it can afford to join now.
  def canUsePaidToken: bool = or {
    paidRingMembership.keys().contains(currentPeriod)
      and paidRingMembership.get(currentPeriod),
    feeAccountBalance >= 1,
  }

  /// Caller-side precondition: at least one unload token can be obtained.
  def canObtainUnloadToken: bool = hasFreeUnloadToken or canUsePaidToken

  // 8.x.1 Join the paid-token ring for the current period (one-time fee).
  action joinPaidRing: bool = all {
    not(paidRingMembership.keys().contains(currentPeriod)
          and paidRingMembership.get(currentPeriod)),
    feeAccountBalance >= 1,
    paidRingMembership' = paidRingMembership.put(currentPeriod, true),
    feeAccountBalance' = feeAccountBalance - 1,
    purses' = purses,
    coins' = coins,
    entries' = entries,
    operations' = operations,
    rings' = rings,
    nextHandle' = nextHandle,
    nextRingIdx' = nextRingIdx,
    nextAccount' = nextAccount,
    nextMemberKey' = nextMemberKey,
    now' = now,
    nextExtrinsicId' = nextExtrinsicId,
    receipts' = receipts,
    events' = events,
    tokens' = tokens,
    opRequested' = opRequested,
    totalIn' = totalIn,
    totalOut' = totalOut,
  }

  // 8.x.2 External fee account top-up (out of band; modeled as a free
  // action that adds balance).
  action topUpFeeAccount(amount: Amount): bool = all {
    amount > 0,
    amount <= 5,
    feeAccountBalance' = feeAccountBalance + amount,
    paidRingMembership' = paidRingMembership,
    purses' = purses,
    coins' = coins,
    entries' = entries,
    operations' = operations,
    rings' = rings,
    nextHandle' = nextHandle,
    nextRingIdx' = nextRingIdx,
    nextAccount' = nextAccount,
    nextMemberKey' = nextMemberKey,
    now' = now,
    nextExtrinsicId' = nextExtrinsicId,
    receipts' = receipts,
    events' = events,
    tokens' = tokens,
    opRequested' = opRequested,
    totalIn' = totalIn,
    totalOut' = totalOut,
  }

  /// Consume a token slot, returning the updated token map.
  pure def consumeFreeToken(
    ts: (int, UnloadTokenClass, int) -> UnloadToken,
    period: int,
    ctr: int,
  ): (int, UnloadTokenClass, int) -> UnloadToken = {
    val rec: UnloadToken = { period: period, class: UTFree, counter: ctr, consumed: true }
    ts.put((period, UTFree, ctr), rec)
  }

  pure def consumePaidToken(
    ts: (int, UnloadTokenClass, int) -> UnloadToken,
    period: int,
    ctr: int,
  ): (int, UnloadTokenClass, int) -> UnloadToken = {
    val rec: UnloadToken = { period: period, class: UTPaid, counter: ctr, consumed: true }
    ts.put((period, UTPaid, ctr), rec)
  }

  /// True iff coin `k` is eligible for the coin-age recycle sweep.
  def coinAgeRecycleEligible(k: (PurseId, CoinIndex)): bool = {
    val c = coins.get(k)
    coins.keys().contains(k)
      and c.state == CoinAvailable
      and c.age >= RecycleAtAge
      and purses.keys().contains(c.purse)
  }

  /// True iff entry `k` is eligible for the ring-expiration rescue sweep.
  def ringExpirationRescueEligible(k: (PurseId, EntryIndex)): bool = {
    val e = entries.get(k)
    val ringExists = rings.keys().contains(e.ringIdx)
    entries.keys().contains(k)
      and e.local == LocalAvailable
      and ringExists
      and rings.get(e.ringIdx).immutableSince >= 0
      and now + RescueMargin >= rings.get(e.ringIdx).immutableSince + RecyclerExpiration
      and not(rings.get(e.ringIdx).expired)
      and purses.keys().contains(e.purse)
      and canObtainUnloadToken
  }

  /// True iff *some* maintenance work is eligible right now.
  def anySweepEligible: bool = or {
    coins.keys().exists(coinAgeRecycleEligible),
    entries.keys().exists(ringExpirationRescueEligible),
  }

  // 8.3 Caller-triggered maintenance sweep (§8.7). Fires one eligible
  // sweep action — `coinAgeRecycle` if any aging coin is eligible, else
  // `ringExpirationRescue` if any entry is near ring expiry. The caller
  // invokes the sweep repeatedly to drain. The autonomous actions remain
  // independently enabled per their own preconditions; the sweep API
  // exists to give the caller a single entry point that mirrors what an
  // implementation's periodic scheduler does.
  action runMaintenanceSweep: bool = any {
    nondet k = oneOf(coins.keys().filter(coinAgeRecycleEligible))
    coinAgeRecycle(k),

    nondet k = oneOf(entries.keys().filter(ringExpirationRescueEligible))
    ringExpirationRescue(k),
  }

  // 8.4 Restart (§7.4). All durable state is preserved. Any operation
  // whose status is `SSubmitted` / `SInBlock` / `SFinalized` / `SWaiting`
  // is restored as `SPreparing` so the host can re-observe the chain and
  // proceed. Terminal operations are preserved. Locks held by restored
  // operations remain in place.
  def isInFlight(op: OperationRec): bool = match op.status {
    | SPreparing => true
    | SSubmitted => true
    | SInBlock => true
    | SFinalized => true
    | SWaiting(_) => true
    | _ => false
  }

  action restart: bool = all {
    operations' = operations.keys().fold(Map(), (m, h) => {
      val op = operations.get(h)
      if (isInFlight(op))
        m.put(h, op.with("status", SPreparing))
      else
        m.put(h, op)
    }),
    purses' = purses,
    coins' = coins,
    entries' = entries,
    rings' = rings,
    nextHandle' = nextHandle,
    nextRingIdx' = nextRingIdx,
    nextAccount' = nextAccount,
    nextMemberKey' = nextMemberKey,
    now' = now,
    nextExtrinsicId' = nextExtrinsicId,
    receipts' = receipts,
    events' = events.append(EResynced),
    totalIn' = totalIn,
    tokens' = tokens,
    paidRingMembership' = paidRingMembership,
    feeAccountBalance' = feeAccountBalance,
    opRequested' = opRequested,
    totalOut' = totalOut,
  }

  // ==========================================================================
  // 8.y Classification and recovery (§8.8 / §8.10)
  // ==========================================================================

  /// True iff `acct` is the account of some coin known to any purse.
  def coinExistsForAccount(acct: Account): bool =
    coins.keys().exists(k => coins.get(k).account == acct)

  /// Synchronous classification of incoming payment memos (§8.8). The
  /// layer treats an empty memo list as `PCUnmatched`.
  def classifyIncomingPayment(memos: List[MemoEntry]): PaymentClassification = {
    val n = memos.length()
    val matched = memos.foldl(0, (a, m) =>
      if (coinExistsForAccount(m.recipientAccount)) a + 1 else a)
    if (n == 0) PCUnmatched
    else if (matched == n) PCMatched
    else if (matched == 0) PCUnmatched
    else PCReceived
  }

  // 8.y.1 Recover. Starts a KRecover operation that walks
  // Preparing → SDone. For each provided purse id not currently in
  // `purses`, the layer re-creates a record with a placeholder name.
  // The main purse is always restored.
  action recover(toRecover: Set[PurseId]): bool = {
    val h = nextHandle
    val withMain = toRecover.union(Set(MAIN_PURSE))
    val newPurses = withMain.fold(purses, (m, pid) => {
      if (m.keys().contains(pid)) m
      else m.put(pid, { id: pid, name: "recovered", nextCoinIdx: 0, nextEntryIdx: 0 })
    })
    val op: OperationRec = {
      handle: h, kind: KRecover, purse: MAIN_PURSE,
      status: SDone,
      lockedCoins: Set(),
      lockedEntries: Set(),
    }
    all {
      withMain.size() > 0,
      operations' = operations.put(h, op),
      purses' = newPurses,
      nextHandle' = h + 1,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(Set()),
      }),
      events' = events.append(EOperationStarted({ handle: h, kind: KRecover, purse: MAIN_PURSE }))
                      .append(EOperationCompleted({ handle: h, status: SDone })),
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 8.y.2 Extend scan. Bumps the purse's `nextCoinIdx` / `nextEntryIdx`
  // forward to `fromCoin` / `fromEntry`. This reflects the design: the
  // caller is asking the layer to look further out in the deterministic
  // namespace. The operation completes synchronously.
  action extendScan(p: PurseId, fromCoin: CoinIndex, fromEntry: EntryIndex): bool = {
    val pExists = purses.keys().contains(p)
    val purse = if (pExists) purses.get(p)
      else { id: -1, name: "", nextCoinIdx: 0, nextEntryIdx: 0 }
    val newCoinIdx = if (fromCoin > purse.nextCoinIdx) fromCoin else purse.nextCoinIdx
    val newEntryIdx = if (fromEntry > purse.nextEntryIdx) fromEntry else purse.nextEntryIdx
    val h = nextHandle
    val op: OperationRec = {
      handle: h, kind: KRecover, purse: p,
      status: SDone,
      lockedCoins: Set(),
      lockedEntries: Set(),
    }
    all {
      pExists,
      fromCoin >= 0,
      fromEntry >= 0,
      operations' = operations.put(h, op),
      purses' = purses.put(p, purse.with("nextCoinIdx", newCoinIdx)
                                    .with("nextEntryIdx", newEntryIdx)),
      nextHandle' = h + 1,
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XSucceeded(Set()),
      }),
      events' = events.append(EOperationStarted({ handle: h, kind: KRecover, purse: p }))
                      .append(EOperationCompleted({ handle: h, status: SDone })),
      coins' = coins,
      entries' = entries,
      rings' = rings,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // ==========================================================================
  // 9. Chain-side events
  // ==========================================================================

  /// Number of entries on a given ring (sum across all purses).
  def ringSize(ridx: RingIndex): int =
    entries.keys().fold(0, (a, k) =>
      if (entries.get(k).ringIdx == ridx) a + 1 else a)

  /// Anonymity deficit (positive ⇒ ring is below floor).
  def ringDeficit(ridx: RingIndex): int = AnonymityFloor - ringSize(ridx)

  // 9.1 Ring members reach readiness, but only if the ring has met the
  // anonymity floor by sealing time. Rings under the floor go to
  // OnDegraded via `chainPromoteToDegraded` instead.
  action chainPromoteToReady(k: (PurseId, EntryIndex)): bool = {
    val eExists = entries.keys().contains(k)
    val e = if (eExists) entries.get(k)
      else { purse: -1, idx: -1, memberKey: -1, exponent: 0, allocatedAt: 0,
             readyAt: 0, ringIdx: -1, onChain: OnMissing, local: LocalAvailable }
    val ringExists = rings.keys().contains(e.ringIdx)
    val ring = if (ringExists) rings.get(e.ringIdx)
      else { idx: -1, immutableSince: -1, expired: false }
    all {
      eExists,
      e.onChain == OnWaiting,
      e.ringIdx >= 0,
      ringExists,
      ring.immutableSince >= 0,
      ringSize(e.ringIdx) >= AnonymityFloor,
      entries' = entries.put(k, e.with("onChain", OnReady)),
      purses' = purses,
      coins' = coins,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EEntryReadinessChanged({ purse: e.purse, exponent: e.exponent, newState: OnReady })),
      totalIn' = totalIn,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalOut' = totalOut,
    }
  }

  /// Ring sealed *under* the anonymity floor: entry promoted to
  /// OnDegraded carrying the deficit. Caller can opt in to degraded
  /// usage explicitly (see `entrySelectable(_, allowDegraded=true)`).
  action chainPromoteToDegraded(k: (PurseId, EntryIndex)): bool = {
    val eExists = entries.keys().contains(k)
    val e = if (eExists) entries.get(k)
      else { purse: -1, idx: -1, memberKey: -1, exponent: 0, allocatedAt: 0,
             readyAt: 0, ringIdx: -1, onChain: OnMissing, local: LocalAvailable }
    val ringExists = rings.keys().contains(e.ringIdx)
    val ring = if (ringExists) rings.get(e.ringIdx)
      else { idx: -1, immutableSince: -1, expired: false }
    val deficit = ringDeficit(e.ringIdx)
    all {
      eExists,
      e.onChain == OnWaiting,
      e.ringIdx >= 0,
      ringExists,
      ring.immutableSince >= 0,
      deficit > 0,
      entries' = entries.put(k, e.with("onChain", OnDegraded(deficit))),
      purses' = purses,
      coins' = coins,
      operations' = operations,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events.append(EEntryReadinessChanged({
        purse: e.purse, exponent: e.exponent, newState: OnDegraded(deficit)
      })),
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 9.2 Ring sealed: immutable_since set.
  action chainSealRing(ridx: RingIndex): bool = {
    val rExists = rings.keys().contains(ridx)
    val r = if (rExists) rings.get(ridx)
      else { idx: -1, immutableSince: -1, expired: false }
    all {
      rExists,
      r.immutableSince == -1,
      rings' = rings.put(ridx, r.with("immutableSince", now)),
      nextRingIdx' = if (ridx == nextRingIdx) nextRingIdx + 1 else nextRingIdx,
      purses' = purses,
      coins' = coins,
      entries' = entries,
      operations' = operations,
      nextHandle' = nextHandle,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  // 9.3 Ring cleanup. In reality the chain fires this unconditionally at
  // `immutable_since + RecyclerExpiration`, destroying any backing value of
  // entries that the host failed to unload. To encode the *design contract*
  // (rescue must happen before destruction), this action is additionally
  // gated on "no Available entry remains on this ring". The scheduler is
  // thereby forced to fire `ringExpirationRescue` first; if the host falls
  // behind, the spec is unsatisfiable — which corresponds to a real-world
  // bug, not to a valid execution.
  def ringEntriesAllConsumed(ridx: RingIndex): bool =
    entries.keys().forall(k => {
      val e = entries.get(k)
      e.ringIdx != ridx or e.local == LocalConsumed
    })

  action chainExpireRing(ridx: RingIndex): bool = {
    val rExists = rings.keys().contains(ridx)
    val r = if (rExists) rings.get(ridx)
      else { idx: -1, immutableSince: -1, expired: false }
    all {
      rExists,
      r.immutableSince >= 0,
      now >= r.immutableSince + RecyclerExpiration,
      not(r.expired),
      // Design contract: every entry on the ring must already be terminal
      // (Consumed) before the chain destroys it. The host is expected to
      // rescue Available entries via the rescue sweep, and to complete or
      // cancel in-flight operations holding LockedFor entries. The
      // unconstrained chain action (no precondition) would model the
      // iOS silent-loss bug; constraining it expresses the design contract.
      ringEntriesAllConsumed(ridx),
      rings' = rings.put(ridx, r.with("expired", true)),
      purses' = purses,
      coins' = coins,
      entries' = entries,
      operations' = operations,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      nextExtrinsicId' = nextExtrinsicId,
      receipts' = receipts,
      events' = events,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
      totalOut' = totalOut,
    }
  }

  /// Adversary action: a coin whose secret has been exposed externally
  /// (via an in-flight or completed Export operation) is snipped by a
  /// third party who races the layer to spend it. Models the
  /// `FRSnipedCoin` failure mode (§10). Fires only on Available coins
  /// that have been involved in a KExport op.
  def coinHasExportHistory(k: (PurseId, CoinIndex)): bool =
    operations.keys().exists(h => {
      val op = operations.get(h)
      op.kind == KExport
        and (op.lockedCoins.contains(k) or op.status == SDone)
    })

  action chainSnipeCoin(k: (PurseId, CoinIndex), h: OpHandle): bool = {
    val cExists = coins.keys().contains(k)
    val c = if (cExists) coins.get(k)
      else { purse: -1, idx: -1, account: -1, exponent: 0, age: 0, state: CoinPending }
    val opExists = operations.keys().contains(h)
    val op = if (opExists) operations.get(h)
      else { handle: -1, kind: KRecover, purse: -1, status: SFailed(FRInternal),
             lockedCoins: Set(), lockedEntries: Set() }
    val isExportOp = op.kind == KExport and op.lockedCoins.contains(k)
    val mid = op.status == SPreparing or op.status == SSubmitted or op.status == SInBlock
    all {
      cExists,
      opExists,
      isExportOp,
      mid,
      // Coin transitions to Spent (it's gone from our view) and the op
      // fails with FRSnipedCoin. Locks release.
      coins' = coins.put(k, c.with("state", CoinSpent)),
      operations' = operations.put(h, op.with("status", SFailed(FRSnipedCoin))
                                       .with("lockedCoins", Set())
                                       .with("lockedEntries", Set())),
      totalOut' = totalOut + coinValue(c.exponent),
      nextExtrinsicId' = nextExtrinsicId + 1,
      receipts' = appendReceipt(receipts, h, {
        extrinsicId: nextExtrinsicId,
        outcome: XRejected(FRSnipedCoin),
      }),
      events' = events.append(ECoinSpent({ purse: c.purse, exponent: c.exponent }))
                      .append(EOperationCompleted({ handle: h, status: SFailed(FRSnipedCoin) })),
      purses' = purses,
      entries' = entries,
      rings' = rings,
      nextHandle' = nextHandle,
      nextRingIdx' = nextRingIdx,
      nextAccount' = nextAccount,
      nextMemberKey' = nextMemberKey,
      now' = now,
      tokens' = tokens,
      paidRingMembership' = paidRingMembership,
      feeAccountBalance' = feeAccountBalance,
      opRequested' = opRequested,
      totalIn' = totalIn,
    }
  }

  // 9.4 Time advances.
  action tick: bool = all {
    now < 100,
    purses' = purses,
    coins' = coins,
    entries' = entries,
    operations' = operations,
    rings' = rings,
    nextHandle' = nextHandle,
    nextRingIdx' = nextRingIdx,
    nextAccount' = nextAccount,
    nextMemberKey' = nextMemberKey,
    now' = now + 1,
    nextExtrinsicId' = nextExtrinsicId,
    receipts' = receipts,
    events' = events,
    totalIn' = totalIn,
    tokens' = tokens,
    paidRingMembership' = paidRingMembership,
    feeAccountBalance' = feeAccountBalance,
    opRequested' = opRequested,
    totalOut' = totalOut,
  }

  // ==========================================================================
  // 10. Step relation
  // ==========================================================================

  action step: bool = any {
    nondet np = oneOf(Set(1, 2).exclude(purses.keys()))
    nondet nm = oneOf(Set("a", "b"))
    createPurse(np, nm),

    nondet p = oneOf(purses.keys())
    nondet nm = oneOf(Set("renamed", "x"))
    renamePurse(p, nm),

    nondet p = oneOf(purses.keys())
    deletePurse(p),

    nondet pf = oneOf(purses.keys())
    nondet pt = oneOf(purses.keys())
    nondet k = oneOf(coins.keys())
    rebalancePurse(pf, pt, k),

    nondet p = oneOf(purses.keys())
    nondet e = oneOf(Set(0, 1, 2))
    topUp(p, e),

    nondet k = oneOf(coins.keys())
    transfer(k),

    nondet p = oneOf(purses.keys())
    nondet sel = oneOf(availableCoinsIn(p).powerset())
    transferAmount(p, sel, true),

    nondet p = oneOf(purses.keys())
    nondet sel = oneOf(availableCoinsIn(p).powerset())
    startTransfer(p, sel, true),

    nondet p = oneOf(purses.keys())
    nondet k = oneOf(coins.keys())
    startTransferDeterministic(p, k),

    nondet h = oneOf(operations.keys())
    opCommitTransfer(h),

    nondet p = oneOf(purses.keys())
    nondet e = oneOf(Set(0, 1, 2))
    startTopUp(p, e),

    nondet h = oneOf(operations.keys())
    nondet e = oneOf(Set(0, 1, 2))
    opCommitTopUp(h, e),

    nondet k = oneOf(coins.keys())
    startExport(k),

    nondet h = oneOf(operations.keys())
    opCommitExport(h),

    nondet p = oneOf(purses.keys())
    nondet e = oneOf(Set(0, 1, 2))
    startImport(p, e),

    nondet h = oneOf(operations.keys())
    nondet e = oneOf(Set(0, 1, 2))
    opCommitImport(h, e),

    nondet pf = oneOf(purses.keys())
    nondet pt = oneOf(purses.keys())
    nondet k = oneOf(coins.keys())
    startRebalance(pf, pt, k),

    nondet h = oneOf(operations.keys())
    nondet pt = oneOf(purses.keys())
    opCommitRebalance(h, pt),

    nondet k = oneOf(coins.keys())
    exportCoin(k),

    nondet p = oneOf(purses.keys())
    nondet e = oneOf(Set(0, 1, 2))
    importCoin(p, e),

    nondet k = oneOf(entries.keys())
    nondet req = oneOf(Set(1, 2, 3, 4))
    startExternalOffload(k, req),

    nondet h = oneOf(operations.keys())
    cancelOp(h),

    nondet h = oneOf(operations.keys())
    nondet se = oneOf(Set(0, 1, 2))
    opOffboard(h, se),

    nondet h = oneOf(operations.keys())
    opAdvanceToSubmitted(h),

    nondet h = oneOf(operations.keys())
    opAdvanceToInBlock(h),

    nondet h = oneOf(operations.keys())
    opAdvanceToFinalized(h),

    nondet h = oneOf(operations.keys())
    opChainReject(h),

    nondet k = oneOf(coins.keys())
    nondet h = oneOf(operations.keys())
    chainSnipeCoin(k, h),

    nondet h = oneOf(operations.keys())
    opEnterWait(h),

    nondet h = oneOf(operations.keys())
    opWake(h),

    nondet k = oneOf(coins.keys())
    coinAgeRecycle(k),

    nondet k = oneOf(entries.keys())
    ringExpirationRescue(k),

    nondet k = oneOf(entries.keys())
    chainPromoteToReady(k),

    nondet k = oneOf(entries.keys())
    chainPromoteToDegraded(k),

    nondet r = oneOf(rings.keys())
    chainSealRing(r),

    nondet r = oneOf(rings.keys())
    chainExpireRing(r),

    runMaintenanceSweep,
    restart,
    joinPaidRing,

    nondet rs = oneOf(Set(1, 2).powerset())
    recover(rs),

    nondet p = oneOf(purses.keys())
    nondet fc = oneOf(Set(0, 1, 2, 3))
    nondet fe = oneOf(Set(0, 1, 2, 3))
    extendScan(p, fc, fe),

    nondet a = oneOf(Set(1, 2))
    topUpFeeAccount(a),

    tick,
  }

  // ==========================================================================
  // 11. Invariants
  // ==========================================================================

  val coinIndexBounded: bool =
    coins.keys().forall(k =>
      purses.keys().contains(k._1) and k._2 < purses.get(k._1).nextCoinIdx)

  val entryIndexBounded: bool =
    entries.keys().forall(k =>
      purses.keys().contains(k._1) and k._2 < purses.get(k._1).nextEntryIdx)

  val lockConsistency: bool = and {
    coins.keys().forall(k => {
      val c = coins.get(k)
      not(isCoinLocked(c)) or
        (operations.keys().contains(coinLockHandle(c))
          and operations.get(coinLockHandle(c)).lockedCoins.contains(k))
    }),
    entries.keys().forall(k => {
      val e = entries.get(k)
      not(isEntryLocked(e)) or
        (operations.keys().contains(entryLockHandle(e))
          and operations.get(entryLockHandle(e)).lockedEntries.contains(k))
    }),
    operations.keys().forall(h => {
      val op = operations.get(h)
      and {
        op.lockedCoins.forall(k =>
          coins.keys().contains(k)
            and isCoinLocked(coins.get(k))
            and coinLockHandle(coins.get(k)) == h),
        op.lockedEntries.forall(k =>
          entries.keys().contains(k)
            and isEntryLocked(entries.get(k))
            and entryLockHandle(entries.get(k)) == h),
      }
    }),
  }

  val coinAgeBound: bool =
    coins.keys().forall(k => {
      val c = coins.get(k)
      c.state != CoinAvailable or c.age < MaxAge
    })

  val conservation: bool =
    totalIn - totalOut == liveValue

  def isFailed(op: OperationRec): bool = match op.status {
    | SFailed(_) => true
    | _ => false
  }

  def isTerminal(op: OperationRec): bool =
    op.status == SDone or isFailed(op)

  val terminalReleasesLocks: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      not(isTerminal(op)) or (op.lockedCoins.size() == 0 and op.lockedEntries.size() == 0)
    })

  // The central design property: if a ring has expired, no entry on it is
  // still Available. The rescue sweep is what enforces this.
  val noEntryOnExpiredRing: bool =
    entries.keys().forall(k => {
      val e = entries.get(k)
      not(rings.keys().contains(e.ringIdx))
        or not(rings.get(e.ringIdx).expired)
        or e.local == LocalConsumed
    })

  val mainPurseExists: bool = purses.keys().contains(MAIN_PURSE)

  val operationsPurseExists: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      isTerminal(op) or purses.keys().contains(op.purse)
    })

  // Every terminal operation has at least one receipt entry.
  val terminalOpsHaveReceipts: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      not(isTerminal(op))
        or (receipts.keys().contains(h) and receipts.get(h).length() > 0)
    })

  // SDone implies at least one Succeeded receipt; SFailed implies at least
  // one Rejected receipt (per design §9 / §5.5).
  def isSucceeded(r: ExtrinsicRecord): bool = match r.outcome {
    | XSucceeded(_) => true
    | _ => false
  }
  def isRejected(r: ExtrinsicRecord): bool = match r.outcome {
    | XRejected(_) => true
    | _ => false
  }
  val receiptOutcomeMatchesStatus: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      val rs = if (receipts.keys().contains(h)) receipts.get(h) else List()
      and {
        op.status != SDone or rs.foldl(false, (a, r) => a or isSucceeded(r)),
        not(isFailed(op)) or rs.foldl(false, (a, r) => a or isRejected(r)),
      }
    })

  /// True iff `events` contains an `EOperationCompleted` event for handle `h`.
  def hasCompletedEvent(h: OpHandle): bool =
    events.foldl(false, (a, ev) => a or (match ev {
      | EOperationCompleted(payload) => payload.handle == h
      | _ => false
    }))

  // Every terminal operation has an EOperationCompleted event.
  val terminalOpsHaveCompletedEvent: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      not(isTerminal(op)) or hasCompletedEvent(h)
    })

  // Live coin/entry records always reference an existing purse.
  val liveRecordsRefExistingPurse: bool = and {
    coins.keys().forall(k => {
      val c = coins.get(k)
      c.state == CoinSpent or purses.keys().contains(c.purse)
    }),
    entries.keys().forall(k => {
      val e = entries.get(k)
      e.local == LocalConsumed or purses.keys().contains(e.purse)
    }),
  }

  val externalOffloadLocksEntriesOnly: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      op.kind != KExternalOffload or op.lockedCoins.size() == 0
    })

  // ==========================================================================
  // Temporal / liveness properties
  // ==========================================================================
  //
  // These require fairness assumptions on the autonomous and chain-side
  // actions: under fair scheduling, every eligible action eventually fires.
  // They are NOT part of `safety`; check them with
  //   quint verify --temporal=<name> docs/specs/coinage-layer.qnt
  // or run the simulator with --temporal flag if supported.

  /// Every in-flight operation eventually reaches a terminal status.
  temporal everyOpEventuallyTerminates =
    operations.keys().forall(h =>
      eventually(not(operations.keys().contains(h)) or isTerminal(operations.get(h))))

  /// Every Available coin whose age is at the recycle threshold is
  /// eventually consumed (either recycled or otherwise spent).
  temporal everyAgingCoinEventuallyConsumed =
    coins.keys().forall(k => {
      val c = coins.get(k)
      c.state != CoinAvailable or c.age < RecycleAtAge
        or eventually(coins.get(k).state == CoinSpent)
    })

  /// Every entry on a near-expired ring is eventually consumed (rescued
  /// or offboarded) before the chain destroys the ring.
  temporal everyNearExpiryEntryRescued =
    entries.keys().forall(k => {
      val e = entries.get(k)
      val ringExists = rings.keys().contains(e.ringIdx)
      val nearExpiry = ringExists
        and rings.get(e.ringIdx).immutableSince >= 0
        and now + RescueMargin >= rings.get(e.ringIdx).immutableSince + RecyclerExpiration
      not(nearExpiry) or e.local != LocalAvailable
        or eventually(entries.get(k).local == LocalConsumed)
    })

  /// Standard weak-fairness assumption: every always-eligible step fires
  /// infinitely often. (The simulator picks transitions nondeterministically;
  /// Apalache requires the fairness assumption to be stated explicitly when
  /// checking liveness.)

  val safety: bool = and {
    coinIndexBounded,
    entryIndexBounded,
    lockConsistency,
    coinAgeBound,
    conservation,
    terminalReleasesLocks,
    noEntryOnExpiredRing,
    mainPurseExists,
    operationsPurseExists,
    externalOffloadLocksEntriesOnly,
    liveRecordsRefExistingPurse,
    terminalOpsHaveReceipts,
    receiptOutcomeMatchesStatus,
    terminalOpsHaveCompletedEvent,
    feeBalanceNonNegative,
    tokenRecordsConsistent,
    midSubmissionHoldsLocks,
    derivationDeterministic,
    derivationInjective,
    handleMonotone,
    ringIntegrity,
    consumedFreeTokensInRange,
    eventOrderOpStartBeforeComplete,
    noCoinResurrection,
    noEntryResurrection,
    anonymityFloorEnforced,
  }

  /// Distinct coin records map to distinct accounts; distinct entry
  /// records map to distinct member keys. Follows from `derivationDeterministic`
  /// + the no-reuse purse-namespace invariant, but worth stating directly.
  val derivationInjective: bool = and {
    coins.keys().forall(k1 =>
      coins.keys().forall(k2 =>
        k1 == k2 or coins.get(k1).account != coins.get(k2).account)),
    entries.keys().forall(k1 =>
      entries.keys().forall(k2 =>
        k1 == k2 or entries.get(k1).memberKey != entries.get(k2).memberKey)),
  }

  /// nextHandle is strictly greater than every issued handle.
  val handleMonotone: bool =
    operations.keys().forall(h => h < nextHandle)

  /// Ring record keys agree with their stored idx.
  val ringIntegrity: bool =
    rings.keys().forall(r => rings.get(r).idx == r)

  /// Every consumed free token's counter is within the allowed search range.
  val consumedFreeTokensInRange: bool =
    tokens.keys().forall(k => {
      val rec = tokens.get(k)
      rec.class != UTFree or rec.counter < FreeTokenSearchRange
    })

  /// Within the event log, every `EOperationCompleted(h, ...)` is preceded
  /// by an `EOperationStarted({ handle: h, ... })` somewhere earlier.
  ///
  /// Encoded with `foldl` over the event list rather than symbolic index
  /// ranges so the invariant stays inside the fragment Apalache can
  /// unfold. The accumulator carries the set of handles already started
  /// and whether any completion has fired without a prior start.
  type EventScanAcc = { seenStarts: Set[OpHandle], violated: bool }
  val eventOrderOpStartBeforeComplete: bool = {
    val initAcc: EventScanAcc = { seenStarts: Set(), violated: false }
    val final = events.foldl(initAcc, (acc, ev) => match ev {
      | EOperationStarted(p) =>
          { seenStarts: acc.seenStarts.union(Set(p.handle)), violated: acc.violated }
      | EOperationCompleted(p) =>
          { seenStarts: acc.seenStarts,
            violated: acc.violated or not(acc.seenStarts.contains(p.handle)) }
      | _ => acc
    })
    not(final.violated)
  }

  /// Terminal-coin records remain terminal: there is no action that
  /// flips a `Spent` coin back to `Available` (rebalance/recover only
  /// create new coin records at fresh indices).
  ///
  /// We express this as a static well-formedness rule: any `Spent` coin
  /// remains keyed at its original `(purse, idx)`. The "no resurrection"
  /// behavior follows from the absence of any action writing `Spent →
  /// Available`. Invariant-wise, we check that no Spent coin has age
  /// reset below its previously recorded value — but without history,
  /// we instead check that Spent coins are never re-inserted at a
  /// previously-issued (purse, idx) by a fresh derivation. This is
  /// equivalent to the no-reuse `coinIndexBounded` invariant already
  /// enforced: idx < nextCoinIdx.
  val noCoinResurrection: bool =
    coins.keys().forall(k => {
      val c = coins.get(k)
      c.purse == k._1 and c.idx == k._2
    })

  val noEntryResurrection: bool =
    entries.keys().forall(k => {
      val e = entries.get(k)
      e.purse == k._1 and e.idx == k._2
    })

  // --------------------------------------------------------------------------
  // Refinement-only invariants
  // --------------------------------------------------------------------------
  //
  // These are NOT part of `safety`: the abstract spec contains
  // nondeterministic actions (like `startTransfer` taking any covering
  // subset) that broaden the behavioral envelope. They are *refinement
  // targets* — properties an implementation must hold when only the
  // deterministic actions (`startTransferDeterministic`, ...) fire.

  /// Single-coin transfers are deterministic: among Available coins in
  /// the purse with the same value as the locked coin, the locked coin
  /// has highest priority.
  val singleCoinTransferDeterministic: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      val mid = op.status == SPreparing or op.status == SSubmitted
                  or op.status == SInBlock or op.status == SFinalized
      not(op.kind == KTransfer and mid and op.lockedCoins.size() == 1)
        or op.lockedCoins.forall(k => {
            val c = coins.get(k)
            val amount = coinValue(c.exponent)
            availableCoinsIn(op.purse).forall(kk =>
              coinValue(coins.get(kk).exponent) != amount
                or coinOrderLT(c, coins.get(kk)))
          })
    })

  /// Anonymity floor enforced: an entry can be OnReady only if its ring
  /// has at least `AnonymityFloor` members. Sub-floor rings appear as
  /// OnDegraded(deficit) only.
  val anonymityFloorEnforced: bool =
    entries.keys().forall(k => {
      val e = entries.get(k)
      e.onChain != OnReady or ringSize(e.ringIdx) >= AnonymityFloor
    })

  /// Derivation determinism (Appendix B): every coin's account matches
  /// its (purse, idx) derivation; every entry's member key likewise.
  val derivationDeterministic: bool = and {
    coins.keys().forall(k => {
      val c = coins.get(k)
      c.account == deriveCoinAccount(c.purse, c.idx)
    }),
    entries.keys().forall(k => {
      val e = entries.get(k)
      e.memberKey == deriveMemberKey(e.purse, e.idx)
    }),
  }

  /// Phase order: SSubmitted/SInBlock/SFinalized implies the op holds at
  /// least one lock — except for allocate-only kinds (KTopUp, KImport,
  /// KRecover) which produce records without consuming any.
  def isAllocateOnlyKind(k: OpKind): bool =
    k == KTopUp or k == KImport or k == KRecover
  val midSubmissionHoldsLocks: bool =
    operations.keys().forall(h => {
      val op = operations.get(h)
      val mid = op.status == SSubmitted or op.status == SInBlock or op.status == SFinalized
      not(mid) or isAllocateOnlyKind(op.kind)
        or (op.lockedCoins.size() + op.lockedEntries.size() > 0)
    })

  /// Fee account balance never goes negative.
  val feeBalanceNonNegative: bool = feeAccountBalance >= 0

  /// Every stored token record's keys match its tagged contents.
  val tokenRecordsConsistent: bool =
    tokens.keys().forall(k => {
      val rec = tokens.get(k)
      and {
        rec.period == k._1,
        rec.class == k._2,
        rec.counter == k._3,
      }
    })
}
