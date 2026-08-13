import react from "@vitejs/plugin-react";
// Tailwind is how styles are written (ADR 0046). Its theme is generated from
// ds/tokens.css by scripts/gen-tailwind-theme.mjs, so utilities and custom
// properties are the same values spelled two ways.
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vitest/config";
import type { ProxyOptions } from "vite";
import { fileURLToPath } from "node:url";

// Product selection (ADR 0019): the whole app is defined by one product
// surface. `@product` resolves to it — the full workspace by default, the
// mail-only surface when built with ALO_PRODUCT=mail (alomails), or the
// Drive-only surface with ALO_PRODUCT=drive (alodrives).
const ALO_PRODUCT = process.env.ALO_PRODUCT;
const product =
  ALO_PRODUCT === "mail" ? "mail" : ALO_PRODUCT === "drive" ? "drive" : "workplace";

// Browser-tab brand name per product. A proper-noun brand, not translatable
// copy — so it lives here (like the `alo` wordmark) rather than in i18n, and
// is stamped into index.html at build time so the tab is correct before JS
// loads. Keep in step with the marketed product name.
const productTitle: Record<typeof product, string> = {
  workplace: "alo workplace",
  mail: "alomails",
  drive: "alodrives",
};

// Local dev backend. `npm run dev` serves the UI from Vite but the app calls its
// API same-origin, so in dev we proxy the API (and Collabora) path prefixes to a
// real alo server — the live server by default, overridable with VITE_DEV_API
// (e.g. a local jmap on http://localhost:8080). Auth is bearer-token in
// localStorage (no cookies), so changeOrigin is all that's needed.
const DEV_API = process.env.VITE_DEV_API ?? "https://mail.alomails.com";

// Several API prefixes (/drive, /tasks, /admin, …) are ALSO client-side route
// paths. A browser page-load of one (refresh on /drive) sends `Accept: text/html`
// and must be served by Vite's own dev shell — proxying it would return the
// live server's PRODUCTION index.html, whose hashed asset URLs the dev server
// doesn't have → a blank page. So on an HTML navigation we bypass the proxy and
// let Vite serve index.html (the SPA); real API/XHR calls (Accept: */* etc.)
// proxy through as normal.
const spaBypass = (req: { headers: Record<string, string | string[] | undefined> }) => {
  const accept = req.headers.accept;
  if (typeof accept === "string" && accept.includes("text/html")) return "/index.html";
  return undefined;
};

// What the dev server forwards to the API instead of serving the app.
//
// `/api` covers every module: the whole API is mounted there as well as at its
// own paths, and the clients call it there. The module prefixes are gone from
// this list on purpose — /billing, /chat, /hr and the rest are *page* paths in
// the router, and forwarding them meant a hard refresh on /billing/invoices/123
// answered JSON to a browser that wanted the app.
//
// What remains beside `/api` are addresses other software was told: the OIDC
// endpoints, JMAP and its discovery document, CalDAV, mail autoconfig, and
// Collabora's callback. None of them collides with a page.
const API_PATHS = [
  "/api",
  // `/auth/token`, not `/auth`: `/auth/callback` is a *page* — the OIDC
  // redirect target — and proxying the whole prefix sent the browser's
  // redirect to the API, which 404s. The production Caddyfile always listed
  // the single route; only this list was too broad.
  "/jmap", "/oauth", "/auth/token", "/.well-known",
  "/dav", "/autodiscover", "/Autodiscover", "/wopi",
  // A different service behind the same proxy (ADR 0012), so it keeps its own
  // prefix and must not be folded under `/api`.
  "/control",
];
// Collabora paths are loaded by the editor itself (and its server), never a
// user page navigation — proxy them straight through with no SPA bypass.
const COLLABORA_PATHS = ["/wopi", "/hosting", "/browser", "/cool", "/lool"];

// TLS verification of the proxy target is ON by default. Behind a corporate
// TLS-inspecting proxy (whose root CA Node doesn't trust), the handshake to the
// live server fails with "unable to verify the first certificate" and every API
// call 500s. Set VITE_DEV_INSECURE_TLS=1 to skip verification for the dev proxy
// only — never a production concern, this file configures the dev server alone.
const DEV_INSECURE_TLS = process.env.VITE_DEV_INSECURE_TLS === "1";
const base = { target: DEV_API, changeOrigin: true, secure: !DEV_INSECURE_TLS, ws: true };

// The Office editor loads entirely from the real backend host (see OFFICE_HOST /
// OfficeEditor), so the only Collabora path the app itself fetches is the WOPI
// discovery — proxied here same-origin to avoid CORS. Collabora allows the dev
// origin to frame it via its own `frame_ancestors` config (compose), so no CSP
// rewriting is needed.
const devProxy: Record<string, ProxyOptions> = {
  ...Object.fromEntries(API_PATHS.map((p) => [p, { ...base, bypass: spaBypass }])),
  ...Object.fromEntries(COLLABORA_PATHS.map((p) => [p, { ...base }])),
};

export default defineConfig(({ command }) => ({
  plugins: [
    react(),
    tailwindcss(),
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
  // The host the Office (Collabora) editor loads and saves through. In a real
  // deployment the app is served same-origin as the backend, so this is empty
  // and the editor uses the page origin. In local dev the app is on localhost
  // but the backend + Collabora are the remote server — which cannot reach the
  // developer's localhost for the WOPI file — so the editor must target the real
  // backend host directly. `serve` (dev) injects it; `build` leaves it empty.
  define: {
    __ALO_OFFICE_HOST__: JSON.stringify(command === "serve" ? DEV_API : ""),
  },
  resolve: {
    alias: {
      "@product": fileURLToPath(new URL(`./src/product/${product}.tsx`, import.meta.url)),
    },
    // Univer and BlockNote both declare React as a peer dependency. During
    // repeated local restarts Vite can otherwise retain optimized chunks that
    // resolve those peers through different module identities, producing an
    // intermittent "Invalid hook call" before AuthProvider can render.
    dedupe: ["react", "react-dom"],
  },
  optimizeDeps: {
    include: ["react", "react-dom", "react/jsx-runtime", "react/jsx-dev-runtime"],
  },
  server: {
    proxy: devProxy,
  },
  test: {
    environment: "jsdom",
  },
}));
