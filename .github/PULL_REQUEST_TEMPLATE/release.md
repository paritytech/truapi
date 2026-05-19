## Release: <!-- e.g. @parity/truapi 0.1.1 -->

> [!IMPORTANT]
> The PR title must start with `release:` for the publish workflow to fire.
> Example: `release: @parity/truapi 0.1.1`.
> Don't rewrite the squash commit subject in the merge dialog — the
> `release:` prefix has to land on `main`.

### Summary

<!-- One-paragraph summary of what's shipping in this version -->

### Checklist

- [ ] Ran `npm run changeset` and selected the bump type (patch / minor / major)
- [ ] Ran `npm run version-packages` to consume the changeset
- [ ] `js/packages/truapi/package.json` version is bumped
- [ ] `js/packages/truapi/CHANGELOG.md` has the new entry
- [ ] `rust/crates/truapi/Cargo.toml` version matches `js/packages/truapi/package.json`
- [ ] No leftover files under `.changeset/` (other than `config.json`)

