---
name: ts-client-checks
description: Build and smoke-test the @parity/truapi TypeScript package (tsc + Vitest). Use after regenerating the client or after touching js/packages/truapi/.
---

# `@parity/truapi` build + smoke tests

Mirrors step 3 of `docs/local-e2e-testing.md`.

```bash
cd js/packages/truapi
npm run build
npm test
```

`npm test` runs the [Vitest](https://vitest.dev/) suite (`src/**/*.test.ts`),
which loads the source `.ts` files directly (no build step required).

Expected:

- `tsc` (the `build` step) exits cleanly with no diagnostics.
- Vitest reports `Test Files  N passed` / `Tests  M passed` with no failures.
- `src/wire-table.test.ts` emits one `round-trips <method>.<kind>` case per
  generated frame id. The case count tracks `WIRE_TABLE`: adding a method
  grows it by 2 (request + response) or 4 (subscribe).

## Failure modes

- `tsc` errors here usually mean codegen was skipped or out of sync.
  Re-run the `regen-codegen` skill, then retry.
- A `src/wire-equality.test.ts` failure (golden hex mismatch) is a
  wire-breaking change. That is a protocol decision, not a regression to
  "fix" by tweaking the test.
- A `src/wire-table.test.ts` count drop means a versioned wrapper variant is
  missing, not extra. V0.2-only methods (e.g. `host_get_user_id`,
  `host_chat_create_simple_group`, all `EntropyDerivation`, all
  `Payment`) intentionally lack a V1 variant.
