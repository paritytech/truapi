import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// This is a workspace package, so `@parity/truapi` and `@parity/truapi-host-wasm`
// resolve through node_modules (the npm-workspace symlinks) via their package
// `exports` — there is no machine-specific path here. Both packages must be
// built first (their `dist/` present, including the WASM bundle); see README.
export default defineConfig({
  plugins: [react()],
  worker: { format: "es" },
  // Pre-built workspace packages ship real ESM + a Worker + WASM; let Vite load
  // them as-is instead of pre-bundling them.
  optimizeDeps: { exclude: ["@parity/truapi-host-wasm", "@parity/truapi"] },
  server: { port: 4319, strictPort: true },
  preview: { port: 4319, strictPort: true },
});
