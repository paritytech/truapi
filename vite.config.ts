import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { copyFileSync } from 'fs'
import { resolve } from 'path'
import type { Plugin } from 'vite'

// Copies index.html to 404.html after build so GitHub Pages
// serves the SPA shell for all routes instead of a real 404.
function spa404Plugin(): Plugin {
  return {
    name: 'spa-404',
    closeBundle() {
      const dist = resolve(__dirname, 'dist')
      copyFileSync(resolve(dist, 'index.html'), resolve(dist, '404.html'))
    },
  }
}

export default defineConfig({
  base: '/truapi/',
  plugins: [react(), tailwindcss(), spa404Plugin()],
  server: {
    // Redirect legacy base paths to /truapi/ during development.
    // In production, the 404.html SPA fallback handles this instead.
    proxy: {
      '/truapi-explorer': {
        target: 'http://localhost:5173',
        rewrite: (path) => path.replace(/^\/truapi-explorer/, '/truapi'),
      },
      '/host-api-explorer': {
        target: 'http://localhost:5173',
        rewrite: (path) => path.replace(/^\/host-api-explorer/, '/truapi'),
      },
    },
  },
})
