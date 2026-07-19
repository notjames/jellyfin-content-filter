import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["web/dist/**", "desktop/dist/**", "playwright-report/**", "test-results/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["web/**/*.ts", "desktop/**/*.ts", "tests/**/*.ts", "vite.config.ts", "vite.desktop.config.ts"],
    languageOptions: { globals: { window: "readonly", document: "readonly", console: "readonly", MutationObserver: "readonly", HTMLVideoElement: "readonly", localStorage: "readonly", location: "readonly", fetch: "readonly", URL: "readonly", Headers: "readonly", Request: "readonly", XMLHttpRequest: "readonly", CustomEvent: "readonly", Event: "readonly", setTimeout: "readonly", clearTimeout: "readonly" } },
    rules: { "@typescript-eslint/no-this-alias": "off" }
  },
  {
    files: ["packaging/**/*.mjs", "tests/**/*.mjs"],
    languageOptions: { globals: { process: "readonly", console: "readonly", URL: "readonly" } }
  }
);
