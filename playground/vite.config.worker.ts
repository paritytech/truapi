import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
  root: "worker",
  build: {
    outDir: resolve("out/worker"),
    emptyOutDir: true,
    lib: {
      entry: "index.ts",
      formats: ["es"],
      fileName: "index",
    },
  },
});
