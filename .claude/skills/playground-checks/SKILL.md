---
name: playground-checks
description: Static verification of the playground (Next.js build + ESLint). Use after changing playground/ sources or after refreshing the @parity/truapi snapshot.
---

# Playground build + lint

Mirrors step 5 of `docs/local-e2e-testing.md`.

```bash
cd playground
yarn build
yarn lint
```

`yarn build` runs the Next.js static export and a strict `tsc` pass over
the playground sources. `yarn lint` is ESLint.

Expected:

- `yarn build` finishes with `✓ Generating static pages` and writes to `out/`.
- `yarn lint` prints `No ESLint warnings or errors`.

## Failure modes

- `Type error: ...` in `src/lib/host-api-bridge.ts` or
  `src/lib/transport.ts` → the bridge is calling the generated client
  with the wrong shape. Fix at the bridge call site, **never** in the
  generated files. If it persists after a bridge fix, the snapshot is
  stale — run the `refresh-playground-snapshot` skill.
- `Can't resolve '@parity/truapi'` → snapshot missing entirely; same
  fix.
- ESLint complains about unused vars in the generated bridge map →
  lint config in `playground/eslint.config.mjs` should already exempt
  generated paths; do not edit a generated file to silence it.
