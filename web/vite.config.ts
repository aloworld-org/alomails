import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

// Product selection (ADR 0019): the whole app is defined by one product
// surface. `@product` resolves to it — the full workspace by default, or the
// mail-only surface when built with ALO_PRODUCT=mail (as alomails does).
const product = "mail";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@product": fileURLToPath(new URL(`./src/product/${product}.tsx`, import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
  },
});
