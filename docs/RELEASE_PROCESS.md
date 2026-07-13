# Releasing npm packages

The `@parity/truapi` and `@parity/truapi-host` npm packages are published by
[`paritytech/npm_publish_automation`](https://github.com/paritytech/npm_publish_automation).
We never run `npm publish` locally or from a personal account; the
`Release` workflow in `.github/workflows/release.yml` packs the packages
and dispatches the automation.

Releases happen via a dedicated **release PR**. Nothing publishes
automatically on a normal feature merge — only PRs whose title (and
therefore squashed commit subject) starts with `release:` trigger a
publish, and only when they bump the package version.

## How to release

### 1. Cut the protocol version

Run `scripts/cut-version.sh` to crystallize wire types, take an explorer
snapshot, and generate the root `CHANGELOG.md`:

```bash
scripts/cut-version.sh            # crystallize next/, snapshot, changelog
scripts/cut-version.sh --dry-run  # preview without making changes
```

### 2. Bump the package version

```bash
npm run changeset            # interactive: pick patch / minor / major + a short summary
npm run version-packages     # consumes the changeset, bumps package.json + writes CHANGELOG.md
```

The first command writes a markdown file under `.changeset/`; the second
consumes it, bumps the selected package `package.json`, appends the package
`CHANGELOG.md`, deletes the changeset file, and then runs
`scripts/sync-cargo-version.mjs` to keep `rust/crates/truapi/Cargo.toml`
aligned with `js/packages/truapi/package.json`. A protocol release should
therefore include the `@parity/truapi` package, its changelog, and the Cargo
version. A host-runtime-only release can bump `@parity/truapi-host` without
changing the Rust crate version.

### 3. Open a release PR

Commit the resulting diff and open a PR using the **release** template:

```
https://github.com/paritytech/truapi/compare/main...<your-branch>?template=release.md
```

The PR title must start with `release:`. Convention:

```
release: @parity/truapi 0.1.1
release: @parity/truapi-host 0.1.1
```

### 4. Get the PR reviewed and merged

Merge via squash merge (the repo's default). The squash commit subject
defaults to the PR title, so the `release:` prefix carries over to
`main`. **Don't rewrite the squash subject in GitHub's merge dialog** —
the workflow checks the commit subject, and dropping the `release:`
prefix will silently skip the publish. If that does happen, open a
follow-up `release:` PR with any trivial change (a CHANGELOG note tweak,
say); the tag-already-exists guard makes re-runs safe.

### 5. Watch the publish

On merge, CI runs as usual. When CI passes, the `Release` workflow:

1. Confirms the commit subject starts with `release:`.
2. Reads package versions from `js/packages/truapi/package.json` and
   `js/packages/truapi-host/package.json`.
3. Checks for `@parity/truapi@<version>` and
   `@parity/truapi-host@<version>` tags. Packages whose tag already exists
   are skipped, so re-runs are idempotent.
4. Builds generated sources and the host WASM bundle, creates and pushes tags
   for unpublished packages, packs their tarballs, and dispatches to
   `npm_publish_automation`.

You can watch the dispatched run under
[`paritytech/npm_publish_automation` Actions](https://github.com/paritytech/npm_publish_automation/actions).

## Safety properties

- A feature PR that accidentally bumps `package.json` will **not**
  trigger a publish — only `release:` PRs do.
- A `release:` PR that forgets to bump package versions will be skipped at
  the tag-already-exists check, not silently re-publish over an
  existing version.
- A `release:` PR with mismatched `js/packages/truapi/package.json` and
  `rust/crates/truapi/Cargo.toml` versions is blocked at PR time by the
  `Release version check` workflow.
- The whole flow uses the default `GITHUB_TOKEN`. No GitHub App, no bot
  identity, no separate secrets to manage other than the org-level
  `NPM_PUBLISH_AUTOMATION_TOKEN` that the automation itself relies on.
