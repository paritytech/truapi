import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";

export default defineConfig({
  base: process.env.EXPLORER_BASE_PATH ?? "/explorer/",
  plugins: [react(), tailwind()],
});
