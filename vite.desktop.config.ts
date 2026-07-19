import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "desktop/dist",
    emptyOutDir: true,
    lib: {
      entry: "desktop/src/index.ts",
      name: "JellyfinContentFilterDesktop",
      formats: ["iife"],
      fileName: () => "jellyfin-content-filter-desktop.js"
    },
    minify: false,
    sourcemap: true
  }
});
