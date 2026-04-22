---
title: "Statement Store Host API v0.2"
type: rfc
status: draft
owner: "@johnthecat"
pr:
---

# RFC 0008 — Statement Store Host API v0.2

## Summary

Two changes to the Statement Store Host API that unlock the full feature set of the underlying Substrate statement store: expressive topic filtering for subscriptions and queries (AND / OR semantics), and paged subscription delivery that exposes the historical-dump / live-stream boundary.

## Motivation

**Topic filtering.** The current `remote_statement_store_subscribe` start payload is a `Vector(Topic)` interpreted as MatchAll (AND): every listed topic must be present in a statement for it to be delivered. The underlying Substrate `TopicFilter` type also supports MatchAny (OR), but this is unexposed. Without OR semantics a product monitoring multiple independent channels must open one subscription per channel, multiplying connection overhead and complicating fan-in logic.

**Paged delivery.** The Substrate node delivers existing statements to a subscriber in pages before switching to incremental live updates. The current Host API collapses this into a flat `Vector(SignedStatement)` with no page boundary signal. Products cannot distinguish "receiving historical statements" from "receiving new live statements", making it impossible to show a meaningful loaded/synced state. The host is also forced to buffer the entire initial dump or deliver it in semantically meaningless chunks.

## Detailed Design

### API changes

#### 1. TopicFilter type

A new SCALE enum used as the start payload for subscribe (V2):

```rust
enum TopicFilter {
  MatchAll(Vec<Topic>),   // AND: statement must contain every listed topic
  MatchAny(Vec<Topic>),   // OR:  statement must contain at least one listed topic
}
```

The `Any` (match-all) variant is intentionally omitted — receiving the full statement stream is too broad for any realistic product use case. Topic count limits (MatchAll ≤ 4, MatchAny ≤ 128 per Substrate) are left to the host to enforce; the protocol uses unbounded `Vec<Topic>`.

#### 2. Subscribe

The start payload changes from a flat topic list to `TopicFilter`, and the receive payload changes from a flat statement list to a page struct:

```
StatementStoreSubscribeV1_start   = TopicFilter
StatementStoreSubscribeV1_receive = SignedStatementsPage
```

Where:

```rust
struct SignedStatementsPage {
    statements: Vec<SignedStatement>,
    /// false — intermediate page of the initial historical dump; more pages follow.
    /// true  — initial dump is complete; product may render a "synced" state.
    ///         All subsequent pages are also isComplete = true and carry only new statements.
    isComplete: bool,
}
```

The host must preserve delivery order: all `isComplete = false` pages precede the first `isComplete = true` page; no `isComplete = false` page may be emitted afterwards.

### Data model changes

New SCALE codec definitions in `packages/host-api/src/protocol/v1/statementStore.ts`:

```typescript
// Subscribe

export const TopicFilter = Enum({
  MatchAll: Vector(Topic),
  MatchAny: Vector(Topic),
});

export const SignedStatementsPage = Struct({
  statements: Vector(SignedStatement),
  isComplete: bool,
});

export const StatementStoreSubscribeV1_start = TopicFilter;
export const StatementStoreSubscribeV1_receive = SignedStatementsPage;
```

The `StatementStoreAdapter` interface in `packages/statement-store/src/adapter/types.ts` is updated:

```typescript
type TopicFilter =
  | { matchAll: Uint8Array[] }
  | { matchAny: Uint8Array[] };

type StatementsPage = {
  statements: Statement[];
  isComplete: boolean;
};

type StatementStoreAdapter = {
  queryStatements(filter: TopicFilter, destination?: Uint8Array): ResultAsync<Statement[], Error>;
  subscribeStatements(filter: TopicFilter, callback: (page: StatementsPage) => unknown): VoidFunction;
  submitStatement(statement: SignedStatement): ResultAsync<void, Error>; // error variants unchanged
};
```

`queryStatements` accepts a `TopicFilter` but returns a flat array — pagination only applies to the streaming subscription.

### Migration strategy

## Drawbacks

- **Breaking subscribe and query change** — all products and hosts must coordinate the upgrade.
- **isComplete handling burden** — products that render on receipt work naturally; those that wait for the full dataset must buffer until the first `isComplete = true` page.
