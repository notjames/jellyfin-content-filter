import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "web/dist",
    emptyOutDir: true,
    lib: {
      entry: "web/src/index.ts",
      name: "JellyfinContentFilter",
      formats: ["iife"],
      fileName: () => "jellyfin-content-filter.js"
    },
    minify: false,
    sourcemap: true
  },
  test: {
    include: ["web/test/**/*.test.ts", "desktop/test/**/*.test.ts"],
    environment: "happy-dom"
  }
});
