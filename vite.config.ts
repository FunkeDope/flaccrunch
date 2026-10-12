/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import type { AtRule, Plugin as PostcssPlugin } from "postcss";

const host = process.env.TAURI_DEV_HOST;

/**
 * 98.css styles bare elements (button, input, fieldset, table...). It is the
 * Windows 95 theme, so every one of its selectors is scoped to
 * [data-theme="win95"] at build time; the other themes start from a clean slate.
 */
function scope98Css(): PostcssPlugin {
  return {
    postcssPlugin: "flaccrunch-scope-98css",
    Once(root) {
      root.walkRules((rule) => {
        const file = rule.source?.input?.file ?? "";
        if (!/[\\/]98\.css[\\/]/.test(file)) return;
        const parent = rule.parent as AtRule | undefined;
        if (parent?.type === "atrule" && /keyframes$/i.test(parent.name)) return;
        rule.selectors = rule.selectors.map((selector) => {
          const s = selector.trim();
          if (s === "html" || s === ":root") return `html[data-theme="win95"]`;
          return `[data-theme="win95"] ${s}`;
        });
      });
    },
  };
}

export default defineConfig({
  plugins: [react()],
  css: {
    postcss: {
      plugins: [scope98Css()],
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    globals: true,
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
