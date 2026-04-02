# TrUAPI Protocol Explorer

Interactive reference documentation for the TrUAPI (Triangle User-Agent Programming Interface) Protocol — the protocol that mediates all communication between a host application and products running in sandboxes.

Built with React, TypeScript, and Tailwind CSS.

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

## Deployment

This project is configured for automatic deployment to GitHub Pages via GitHub Actions.

**Setup:**

1. Push this repository to GitHub
2. Go to **Settings > Pages**
3. Under **Source**, select **GitHub Actions**
4. The next push to `main` will trigger a deployment

The workflow is defined in `.github/workflows/deploy.yml`.

## Project structure

```
src/
  data/types.ts        # Protocol data: methods, types, groups
  components/          # Reusable UI components
    Sidebar.tsx        # Navigation sidebar with method groups
    CodeBlock.tsx      # Syntax-highlighted code blocks
    PatternBadge.tsx   # Request/Response, Subscription badges
    TypeLink.tsx       # Clickable type references
  pages/
    OverviewPage.tsx   # Landing page with architecture overview
    MethodPage.tsx     # Individual method documentation
    TypesPage.tsx      # Data type browser
    TypeDetailPage.tsx # Individual type documentation
```

## License

MIT
