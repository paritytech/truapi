import { copyFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

function spaFallback(): Plugin {
  return {
    name: "spa-fallback",
    closeBundle() {
      const dist = resolve(import.meta.dirname, "dist");
      const index = resolve(dist, "index.html");
      if (existsSync(index)) {
        copyFileSync(index, resolve(dist, "404.html"));
      }
    },
  };
}

export default defineConfig({
  base: "/truapi/",
  plugins: [react(), tailwindcss(), spaFallback()],
});
