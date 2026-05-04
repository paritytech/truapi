import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import './index.css'
import App from './App.tsx'

// Rewrite legacy paths so bookmarks keep working after renames.
const CURRENT_BASE = '/truapi'
const LEGACY_BASES = ['/host-api-explorer', '/truapi-explorer']
for (const legacy of LEGACY_BASES) {
  if (
    window.location.pathname === legacy ||
    window.location.pathname.startsWith(`${legacy}/`)
  ) {
    const rewritten =
      CURRENT_BASE +
      window.location.pathname.slice(legacy.length) +
      window.location.search +
      window.location.hash
    window.location.replace(rewritten)
    break
  }
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter basename={CURRENT_BASE}>
      <App />
    </BrowserRouter>
  </StrictMode>,
)
