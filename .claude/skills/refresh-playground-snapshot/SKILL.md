---
name: refresh-playground-snapshot
description: Force-refresh the playground's frozen snapshot of @parity/truapi after the package has been rebuilt. Use whenever js/packages/truapi/ changes (codegen, transport, package.json).
---

# Refresh the playground's `@parity/truapi` snapshot

Mirrors step 4 of `docs/local-e2e-testing.md`.

yarn 1.x copies `file:` deps at install time, so
`playground/node_modules/@parity/truapi` is a _snapshot_ of the package
state when `yarn install` last ran. Any change to `js/packages/truapi/`
requires a full reinstall:

```bash
cd playground
rm -rf node_modules
yarn install
```

A `rm -rf node_modules/@parity` followed by `yarn install` is **not**
sufficient when yarn already considers the lockfile satisfied — it will
say `success Already up-to-date` and leave the directory missing.
Always nuke the whole `node_modules` (it is a few seconds to repopulate)
when the snapshot is stale.

## How to tell the snapshot is stale

```bash
diff -q js/packages/truapi/dist/generated/client.d.ts \
        playground/node_modules/@parity/truapi/dist/generated/client.d.ts
```

A non-zero exit means the snapshot is behind the source build.

## Failure modes

- yarn says `Already up-to-date` and the directory is still missing →
  partial nuke. Repeat with the full `rm -rf node_modules`.
- `Can't resolve '@parity/truapi'` from webpack at build time → same
  cause as above.
