# Releasing `@parity/truapi`

The `@parity/truapi` npm package is published by
[`paritytech/npm_publish_automation`](https://github.com/paritytech/npm_publish_automation).
We never publish from a personal account or run `npm publish` locally; the
`Release` workflow in `.github/workflows/release.yml` packs the package and
dispatches the automation.

Versions are managed with [changesets](https://github.com/changesets/changesets).
Releasing follows two steps:

## 1. Author a changeset in your PR

Any PR that should produce a published version must include a changeset.
From the repo root:

```bash
npx changeset
```

The CLI asks which packages changed and whether the bump is `patch`, `minor`,
or `major`, then writes a markdown file under `.changeset/`. Commit that file
as part of your PR. Multiple changesets across multiple PRs accumulate and are
consumed together at release time.

PRs that don't ship user-visible changes (CI tweaks, docs, refactors with no
behavior change) don't need a changeset.

## 2. Merge to `main`

When your PR lands on `main`, CI runs as usual. On CI success, the `Release`
workflow runs and:

1. Skips if the last commit was the release bot (avoids loops).
2. Skips if there are no pending changesets.
3. Otherwise: runs `changeset version` to consume the pending changesets and
   bump `js/packages/truapi/package.json`, regenerates the codegen, rebuilds,
   commits the bump as `truapi-release-bot[bot]`, and tags
   `@parity/truapi@<version>`.
4. Packs the tarball and dispatches it to `npm_publish_automation`, which
   performs the actual `npm publish`.

The bot's own version-bump commit triggers CI again; the second `Release`
run hits the skip-if-bot guard and does nothing.
