---
name: ts-client-checks
description: Build and smoke-test the @parity/truapi TypeScript package (tsc, wire-equality, wire-table-loop). Use after regenerating the client or after touching js/packages/truapi/.
---

# `@parity/truapi` build + smoke tests

Mirrors step 3 of `docs/local-e2e-testing.md`.

```bash
cd js/packages/truapi
npm run build
npm test
```

Expected:

- `tsc` exits cleanly with no diagnostics.
- `wire-equality.test.mjs`: `all 6 wire-equality tests passed`.
- `wire-table-loop.test.mjs`:
  `programmatic wire-table loop: <N> (id, tag) pairs round-tripped`.
  `<N>` should match the size of `WIRE_TABLE`. Adding a method grows it
  by 2 (request + response) or 4 (subscribe).

## Failure modes

- `tsc` errors here usually mean codegen was skipped or out of sync.
  Re-run the `regen-codegen` skill, then retry.
- A wire-equality failure (golden hex mismatch) is a wire-breaking
  change. That is a protocol decision, not a regression to "fix" by
  tweaking the test.
- A wire-table-loop count mismatch means a versioned wrapper variant is
  missing, not extra. V0.2-only methods (e.g. `host_get_user_id`,
  `host_chat_create_simple_group`, all `EntropyDerivation`, all
  `Payment`) intentionally lack a V1 variant.
