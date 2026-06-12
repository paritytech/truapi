# I - Nested dApps note

> Part of the [host-contract & core-impl spec](<index.md>). This is a non-blocking design note, not a v1
> parity gate.

Current dotli has a nested bridge detector in `~/github/dotli/packages/ui/src/container.ts`
(`setupNestedBridgeDetector`). When a child iframe posts host-container bytes to `window.top`, dotli
creates another JS `Container` for that source window.

Observed current behavior:

- The nested bridge gets a synthetic debug/container id like `<label>:nested-<n>`.
- Handler wiring still receives the parent `label`, so account derivation, signing, aliases, entropy,
  resource allocation, and permission prompts all use the parent product identity.
- Local storage is the exception: nested bridges get a separate prefix,
  `dotli:<label>:nested-<n>:`.

## V1 Rust Migration Decision

Nested dApps should use the shared Rust core. Do not create separate nested Rust runtimes, sessions,
product identities, or storage namespaces as part of the `@novasamatech` removal milestone.

If dotli keeps nested message forwarding during the migration, route nested messages into the same
top-level product core/provider context. This preserves the important security property that nested
content cannot silently become a different product identity, and avoids treating current JS storage-prefix
behavior as a protocol contract.

## Why Track It Separately

The nested bridge may still be useful later:

- product marketplaces embedding other products;
- widgets that expect `window.top` host API access;
- migration compatibility for existing products that already embed dApps;
- future per-embedded-product permission and storage isolation.

Those are broader product/runtime-policy questions, not prerequisites for current dotli feature parity.
A future design can define an explicit nested-product identity model, including storage ownership,
permission prompts, origin checks, and whether nested products get their own wallet-derived account path.
