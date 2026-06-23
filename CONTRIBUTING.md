# Contributing

## Reporting Issues

If you have found what you think is a bug,
please [file an issue](https://github.com/paritytech/truapi/issues/new/choose).

## Suggesting New Features

Feature proposals live as markdown files in `docs/features/`. To propose a new feature:

1. Create a branch and add a new file to `docs/features/` (e.g., `docs/features/my-feature.md`)
2. Include YAML frontmatter (`title`, `type: feature`, `status: draft`, `author`, `pr`)
3. Describe the feature: summary, use cases, and proposed solution
4. Update `docs/features/_index.md` with a link to your file
5. Open a PR using the **feature** template (`?template=feature.md`) and add the `feature-request` and `proposal` labels

## RFCs

For larger changes that need cross-team discussion, use the RFC process:

1. Create a branch and add a new file to `docs/rfcs/<slug>.md` (e.g., `docs/rfcs/my-proposal.md`) — do **not** assign a number
2. Use `docs/rfcs/0001-template.md` as a reference for the expected structure and frontmatter
3. Open a PR using the **rfc** template (`?template=rfc.md`) and add the `rfc` label
4. The PR will be auto-added to the project board for tracking and review
5. When the PR is approved and merged, CI automatically assigns the next sequential number, renames the file, and appends it to `docs/rfcs/_index.md`

**Important:** RFC PRs must include corresponding changes to the TrUAPI Rust
interfaces in `rust/crates/truapi/`. A CI check (`check-rfc.yml`) enforces
this — PRs that touch `docs/rfcs/` without also modifying `rust/crates/truapi/`
will fail. This ensures every RFC ships with a concrete API change, not just
prose.

If you use Claude Code, the [`rfc`](.claude/skills/rfc/SKILL.md) skill is highly recommended for drafting RFCs — invoke it with `/rfc` to turn your notes into a well-structured document that follows the template above.

## Design Documents

Canonical design documentation lives in `docs/design/`. To propose updates or add new design docs:

1. Edit or add a file in `docs/design/`
2. Include YAML frontmatter (`title`, `type: design`, `status`, `author`, `created`, `pr`)
3. Open a PR using the **design** template (`?template=design.md`) and add the `design-doc` label

## Development

### Prerequisites

- Rust toolchain (stable + nightly for `cargo fmt`)
- Node.js and npm (for the TypeScript client)
- Yarn 1.x (for the playground)

### Repository layout

```
rust/crates/
  truapi/              Rust trait + type definitions (source of truth)
  truapi-codegen/      rustdoc JSON → TypeScript client generator
  truapi-macros/       #[wire(id = N)] proc-macro
js/packages/
  truapi/              @parity/truapi TypeScript package (generated TS is auto-generated and git-ignored)
playground/            Next.js interactive playground
hosts/dotli/           dotli host (git submodule)
scripts/codegen.sh     regenerate the TS client from the Rust crate
```

Common tasks are wrapped in the top-level `Makefile`. Run `make help` to see
the full list of targets.

### Getting started

```bash
make setup    # submodules + JS dependencies
make build    # Rust workspace + TypeScript client
```

### Making changes to the API

The Rust crate in `rust/crates/truapi/` is the single source of truth for the
TrUAPI protocol. When you modify traits or types there:

```bash
make codegen      # regenerate the TS client and refresh the playground snapshot
make playground   # rebuild the playground against the refreshed snapshot
```

### Verification

```bash
make test     # Rust + TypeScript client tests
make check    # full suite: build, fmt, clippy, test, TS tests, playground build + lint
```

## Pull requests

Maintainers merge pull requests by squashing all commits and editing the commit message if necessary using the GitHub
user interface.

Use an appropriate commit type. Be especially careful with breaking changes.

## Releasing

See [`docs/RELEASE_PROCESS.md`](docs/RELEASE_PROCESS.md) for the `@parity/truapi` npm publishing flow.
