---
name: truapi-definition-of-done
description: The full local end-to-end checklist before declaring a TrUAPI change done. Chains the layered skills in order. Invoke when the user says "is this ready", "definition of done", or asks to verify a Rust→codegen→TS→playground change end-to-end.
---

# Definition of done

A change is end-to-end-verified locally when **all** of these pass.
Run them in order — each layer assumes the layer below it builds clean.

```
Rust crates  →  codegen  →  @parity/truapi  →  playground  →  dotli iframe
```

## Pre-flight (once per session)

```bash
git submodule update --init --recursive
( cd js/packages/truapi && npm install )
( cd playground && yarn install --frozen-lockfile )
( cd hosts/dotli && bun install )
```

`bun: command not found` → install Bun
(`curl -fsSL https://bun.sh/install | bash`).

## The chain

- [ ] **Rust workspace** — invoke the `rust-checks` skill. All four
      cargo commands clean.
- [ ] **Codegen** — only if Rust trait surface changed. Invoke the
      `regen-codegen` skill, then commit
      `js/packages/truapi/src/{generated,playground,explorer}/`.
- [ ] **`@parity/truapi`** — invoke the `ts-client-checks` skill.
      `npm run build && npm test` clean.
- [ ] **Playground snapshot** — only if codegen ran or
      `js/packages/truapi/` changed. Invoke the
      `refresh-playground-snapshot` skill.
- [ ] **Playground statics** — invoke the `playground-checks` skill.
      `yarn build && yarn lint` clean.
- [ ] **End-to-end** — invoke the `e2e-dotli` skill. Either
      `cd playground && yarn e2e` (preferred) or the manual browser
      flow.

If any layer fails, fix it and rerun **that layer plus every layer
above it**. Skipping a layer because "I only changed X" is the most
common cause of the codegen ↔ snapshot mismatch.

## CI parity

GitHub Actions in `.github/workflows/ci.yml` runs the same chain on
every PR. A green CI run is sufficient evidence for the static layers
(rust, codegen-drift, ts-client, playground); the e2e job runs the
Playwright suite from the `e2e-dotli` skill against a freshly built
dotli host.
