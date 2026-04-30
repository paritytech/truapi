# TrUAPI

TrUAPI (Triangle User-Agent Programming Interface) Protocol — the protocol that mediates all communication between a host application and products running in sandboxes.

This repository is the single source of truth for the TrUAPI protocol, containing Rust trait definitions, an interactive protocol explorer, and documentation.

The explorer covers two protocol versions:

- *v0.1* -- the initial protocol version.
- *v0.2* -- the current protocol version with new capabilities. See [v02-changes.md](v02-changes.md) for a detailed description of all changes and their rationale.

A version switcher in the sidebar lets you browse each version independently.

## Running locally

```bash
# Install dependencies
npm install

# Start the development server
npm run dev
```

Open [http://localhost:5173](http://localhost:5173) in your browser.

## Building for production

```bash
npm run build
```

The built files will be in the `dist/` directory. You can preview the production build with:

```bash
npm run preview
```

## Rust crate docs

The `truapi-spec/` directory contains the Rust crate with trait definitions and types for both protocol versions (modules `v01` and `v02`). To build the docs locally:

```bash
cargo doc --no-deps --manifest-path truapi-spec/Cargo.toml --open
```

## Deployment

This project is configured for automatic deployment to GitHub Pages via GitHub Actions. The workflow builds both the webapp and the Rust crate docs, then deploys them together.

Setup:

1. Push this repository to GitHub
2. Go to Settings > Pages
3. Under Source, select GitHub Actions
4. The next push to `main` will trigger a deployment

After deployment:

- Webapp: `https://paritytech.github.io/truapi/`
- Rust docs: `https://paritytech.github.io/truapi/rustdoc/truapi_spec/`

The workflow is defined in `.github/workflows/deploy.yml`.

## Project structure

```
truapi-spec/              # Rust crate with trait and type definitions
  src/
    lib.rs                # Re-exports v01 and v02 modules
    v01/mod.rs            # Protocol v0.1 trait and types
    v02/mod.rs            # Protocol v0.2 trait and types
src/
  data/
    v01/types.ts          # Webapp data for protocol v0.1
    v02/types.ts          # Webapp data for protocol v0.2
    registry.ts           # Version registry mapping slug to data
  contexts/
    VersionContext.tsx     # React context providing versioned data
  components/             # Reusable UI components
    Sidebar.tsx           # Navigation sidebar with method groups and version switcher
    CodeBlock.tsx         # Syntax-highlighted code blocks
    PatternBadge.tsx      # Request/Response, Subscription badges
    TypeLink.tsx          # Clickable type references
  pages/
    OverviewPage.tsx      # Landing page with architecture overview
    MethodPage.tsx        # Individual method documentation
    TypesPage.tsx         # Data type browser
    TypeDetailPage.tsx    # Individual type documentation
```

## License

MIT
