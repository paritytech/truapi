# Contributing

## Reporting Issues

If you have found what you think is a bug,
please [file an issue](https://github.com/paritytech/host-api/issues/new/choose).

## Suggesting New Features

Feature proposals live as markdown files in `docs/features/`. To propose a new feature:

1. Create a branch and add a new file to `docs/features/` (e.g., `docs/features/my-feature.md`)
2. Include YAML frontmatter (`title`, `type: feature`, `status: draft`, `author`, `pr`)
3. Describe the feature: summary, use cases, and proposed solution
4. Update `docs/features/_index.md` with a link to your file
5. Open a PR using the **feature** template (`?template=feature.md`) and add the `feature-request` and `proposal` labels

## RFCs

For larger changes that need cross-team discussion, use the RFC process:

1. Create a branch and add a new file to `docs/rfcs/` using the next available number (e.g., `docs/rfcs/0002-my-proposal.md`)
2. Use `docs/rfcs/0001-template.md` as a reference for the expected structure and frontmatter
3. Update `docs/rfcs/_index.md` with a link to your RFC
4. Open a PR using the **rfc** template (`?template=rfc.md`) and add the `rfc` and `proposal` labels
5. The PR will be auto-added to the project board for tracking and review

If you use Claude Code, the [`rfc`](.claude/skills/rfc/SKILL.md) skill is highly recommended for drafting RFCs — invoke it with `/rfc` to turn your notes into a well-structured document that follows the template above.

## Design Documents

Canonical design documentation lives in `docs/design/`. To propose updates or add new design docs:

1. Edit or add a file in `docs/design/`
2. Include YAML frontmatter (`title`, `type: design`, `status`, `author`, `created`, `pr`)
3. Open a PR using the **design** template (`?template=design.md`) and add the `design-doc` label

## Development

If you have been assigned to fix an issue or develop a new feature, please follow these steps to get started:

- Fork this repository.
- Install dependencies

  ```shell
  npm install
  ```

  - We use [nvm](https://github.com/nvm-sh/nvm) to manage node versions - please make sure to use the version mentioned
    in `.nvmrc`

    ```shell
    nvm use
    ```

- Build all packages.

  ```shell
  npm run build
  ```

- Run development server.

  ```shell
  npm run build:watch
  ```

- Implement your changes and tests in files in the `packages/` and `__tests__` directories.
- Document your changes in the appropriate doc page.
- Git stage your required changes and commit (see below commit guidelines).
- Submit PR for review.

## Pull requests

Maintainers merge pull requests by squashing all commits and editing the commit message if necessary using the GitHub
user interface.

Use an appropriate commit type. Be especially careful with breaking changes.
