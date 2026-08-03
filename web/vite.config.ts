import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

// Product selection (ADR 0019): the whole app is defined by one product
// surface. `@product` resolves to it — the full workspace by default, or the
// mail-only surface when built with ALO_PRODUCT=mail (as alomails does).
const product = "mail";

// Browser-tab brand name per product. A proper-noun brand, not translatable
// copy — so it lives here (like the `alo` wordmark) rather than in i18n, and
// is stamped into index.html at build time so the tab is correct before JS
// loads. Keep in step with the marketed product name.
const productTitle: Record<typeof product, string> = {
  workplace: "alo workplace",
  mail: "alomails",
};

export default defineConfig({
  plugins: [
    react(),
    {
      name: "alo-product-title",
      transformIndexHtml(html) {
        return html.replace(
          /<title>[^<]*<\/title>/,
          `<title>${productTitle[product]}</title>`,
        );
      },
    },
  ],
  resolve: {
    alias: {
      "@product": fileURLToPath(new URL(`./src/product/${product}.tsx`, import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
  },
});
