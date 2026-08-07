---
title: "Product Manifest — Host Implementation Guide"
type: design
---

# Product Manifest — Host Implementation Guide

This document complements [RFC — Product Manifest Format](../rfcs/product-manifest.md) with concrete data structures and a step-by-step guide for host implementors. The RFC is the normative source; this page is a quick-reference.

## Data Structures

The manifest system uses two layers of structured data: a **root manifest** at the product's dotNS base name and one **executable manifest** per modality subname.

### Root Manifest

```typescript
type RootManifest = {
  $v: 1;
  displayName: string;
  description: string;
  icon: Icon;
};

type Icon = {
  cid: string;            // Bulletin-chain CID
  format: "jpeg" | "png";
};
```

### Executable Manifest

```typescript
type ExecutableManifest = AppManifest | WidgetManifest | WorkerManifest;

type CommonExecutableFields = {
  $v: 1;
  appVersion: SemVer;
};

type AppManifest = CommonExecutableFields & {
  kind: "app";
};

type WidgetManifest = CommonExecutableFields & {
  kind: "widget";
  description?: string;
  dimensions: {
    height: number[];    // supported grid-step heights
    width?: number;      // defaults to 1
  };
};

type WorkerManifest = CommonExecutableFields & {
  kind: "worker";
  entrypoint: string;
  includes: Record<"chat" | "pocket", boolean>;
};

type SemVer = [major: number, minor: number, patch: number, build?: string];
```

### Subname Convention

| Subname                     | Text-record key | Carries                    |
|-----------------------------|-----------------|----------------------------|
| `<product_id>.dot`          | `manifest`      | Root manifest              |
| `app.<product_id>.dot`      | `executable`    | App executable manifest    |
| `widget.<product_id>.dot`   | `executable`    | Widget executable manifest |
| `worker.<product_id>.dot`   | `executable`    | Worker executable manifest |

Absence of a subname means the product does not provide that executable.

## Resolution Flow

A host resolves a product from its dotNS base name `B` in eight steps:

```
1. node = namehash(B)
2. resolver = IDotnsRegistry.resolver(node)
   └─ address(0) → product does not exist; stop
3. json = IDotnsContentResolver.text(node, "manifest")
   └─ empty → product does not exist; stop
4. Parse JSON, validate $v and RootManifest schema
   └─ failure → malformed; surface diagnostic
5. (optional) author = IDotnsRegistry.owner(node)
6. For each executable type the host can render:
   subnode = namehash("<type>.<product_id>.dot")
   repeat steps 2–4 with text(subnode, "executable")
7. (optional) Verify owner(subnode) == owner(node)
8. Fetch bytes: GET <gateway>/ipfs/<cid>
   verify fetched bytes match the CID
```

### dotNS Dry-Run Origin

All reads use a `ReviveApi.call(origin, ...)` dry-run RPC. The origin MUST be the deterministic Revive system account, not a real keypair:

```rust
fn pallet_account(pallet_id: &[u8; 8]) -> [u8; 32] {
    let mut account = [0u8; 32];
    account[..4].copy_from_slice(b"modl");
    account[4..12].copy_from_slice(pallet_id);
    account
}

let origin = pallet_account(b"py/reviv");
// 0x6d6f646c70792f7265766976000000...0000
```

This account need not exist or hold a balance.

## Bulletin Constants

Fixed by the Bulletin chain protocol.

| Constant       | Value              | Meaning                               |
|----------------|--------------------|---------------------------------------|
| CID version    | `1`                | CIDv1                                 |
| Multicodec     | `0x55` (`raw`)     | Stored bytes addressed as raw payload |
| Multihash code | `0x12` (`sha-256`) | Hash algorithm for the CID            |
| Digest length  | `32` bytes         | SHA-256 output size                   |

A Bulletin CID is `CIDv1(raw, sha256(data))`.

## Error Handling

| Condition                                | Host action                              |
|------------------------------------------|------------------------------------------|
| No resolver / empty root manifest        | Product does not exist                   |
| Unknown `$v`                             | Undiscoverable; skip, surface diagnostic |
| Malformed JSON / schema validation fail  | Do not launch; surface diagnostic        |
| Unknown `icon.format` or icon CID fails  | Render placeholder; product launchable   |
| Missing executable subname               | Product does not provide that executable |
| `kind` does not match subname label      | Skip that executable                     |
| Executable CID unreachable / mismatch    | Refuse to launch that executable         |
| Subname owner differs (strict provenance)| Skip that executable                     |

## Caching

- **Manifests**: cache by base name. Detect re-publish by re-reading the text record or comparing `appVersion`.
- **Icon and executable bytes**: cacheable indefinitely by CID (content-addressed; same CID = same bytes).
- dotNS provides no push notifications; hosts must poll.
