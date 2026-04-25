# TrUAPI Protocol Explorer

Interactive reference documentation for the TrUAPI (Triangle User-Agent Programming Interface) Protocol, the protocol that mediates all communication between a host application and products running in sandboxes.

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

- Webapp: `https://paritytech.github.io/truapi-explorer/`
- Rust docs: `https://paritytech.github.io/truapi-explorer/rustdoc/truapi_spec/`

The workflow is defined in `.github/workflows/deploy.yml`.

## MCP server

The site also publishes a [StaticMCP](https://staticmcp.com/) endpoint so AI assistants can query the protocol reference directly. The static JSON files are generated during the GitHub Pages deploy and served from `https://paritytech.github.io/truapi-explorer/mcp/`.

### Add to Claude Code

```bash
claude mcp add truapi-explorer -- npx -y staticmcp-bridge https://paritytech.github.io/truapi-explorer/mcp
```

### Available tools

| Tool | Args | Returns |
|------|------|---------|
| `list_versions` | `scope: "all"` | Available protocol versions |
| `list_groups` | `version` | Method groups for a version |
| `list_methods` | `version` | All methods for a version |
| `list_types` | `version` | All data types for a version |
| `get_group` | `version`, `id` | Group details with expanded methods |
| `get_method` | `version`, `name` | Method definition with examples |
| `get_type` | `version`, `name` | Data type definition |
| `list_docs` | `kind: "rfcs" \| "features" \| "changelog"` | Documents of that kind |
| `get_doc` | `kind`, `slug` | Full markdown body |

`version` is `v01` or `v02`.

### Example queries

Ask Claude things like:

- *"What v0.2 methods are in the payment group?"* → calls `list_methods` then filters, or `get_group({version:"v02", id:"payment"})`.
- *"Show me the host_navigate_to spec."* → `get_method({version:"v02", name:"host_navigate_to"})`.
- *"What changed between v0.1 and v0.2?"* → `get_doc({kind:"changelog", slug:"v02-changes"})`.
- *"List the accepted RFCs."* → `list_docs({kind:"rfcs"})`.

Resources are also addressable directly by URI, e.g. `truapi://v02/methods/host_navigate_to` or `docs://rfcs/0006-payments`.

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
