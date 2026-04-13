import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import './index.css'
import App from './App.tsx'

// The repo was renamed from `host-api-explorer` to `truapi-explorer`.
// Rewrite any incoming legacy path so bookmarks to the old URL keep working.
const LEGACY_BASE = '/host-api-explorer'
const NEW_BASE = '/truapi-explorer'
if (
  window.location.pathname === LEGACY_BASE ||
  window.location.pathname.startsWith(`${LEGACY_BASE}/`)
) {
  const rewritten =
    NEW_BASE +
    window.location.pathname.slice(LEGACY_BASE.length) +
    window.location.search +
    window.location.hash
  window.location.replace(rewritten)
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter basename={NEW_BASE}>
      <App />
    </BrowserRouter>
  </StrictMode>,
)
