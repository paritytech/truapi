# Third-party notices

This project (outbound licence: MIT) depends on third-party software listed below.
All bundled dependencies are under permissive licences compatible with MIT. The
summaries are generated with `cargo deny check licenses` (Rust) and
`license-checker-rseidelsohn` (npm); regenerate after dependency changes.

## Rust crates (`truapi`, `truapi-codegen`, `truapi-macros`)

73 transitive dependencies, all permissive:

| Licence | Notes |
|---------|-------|
| `MIT OR Apache-2.0`, `MIT`, `Apache-2.0` | Permissive, MIT-compatible |
| `Zlib` | Permissive |
| `Unicode-3.0` | Permissive (Unicode data tables) |
| `Unlicense OR MIT` | Permissive (MIT selected) |

`cargo deny check licenses` passes against the allowlist in `deny.toml`. No copyleft
(GPL/LGPL/AGPL/MPL) dependencies are present.

Regenerate:

```bash
cargo deny check licenses
```

## Published npm package (`@parity/truapi`)

The published client has no third-party runtime dependencies bundled into its
distribution beyond peer/dev tooling. Its own licence is MIT.

Regenerate:

```bash
( cd js/packages/truapi && npx license-checker-rseidelsohn --production --summary )
```

## Applications (`playground`, `explorer`)

These are not published as libraries; they are built and deployed as static sites.
Their dependency trees are overwhelmingly permissive (MIT, Apache-2.0, ISC, BSD,
0BSD, CC0-1.0, MPL-2.0). One weak-copyleft dependency is present:

- `@img/sharp-libvips-*` — **LGPL-3.0-or-later**. Pulled in transitively by Next.js
  image optimization. It is dynamically loaded and not modified or statically linked;
  the LGPL notice is preserved here. It is not part of the published `@parity/truapi`
  library.

The `UNLICENSED` entries reported by the licence checker are the repository's own
private, unpublished workspace packages (`truapi-playground`, `truapi-explorer`), not
third-party code.

Regenerate:

```bash
( cd playground && npx license-checker-rseidelsohn --summary )
( cd explorer   && npx license-checker-rseidelsohn --summary )
```
