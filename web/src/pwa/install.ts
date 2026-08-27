// Makes the app installable: injects the product's web app manifest, icons,
// and theme color into <head>, and registers the offline-shell service
// worker. Loaded from index.html as its own module entry
// (before main.tsx), so the whole PWA surface lives in index.html + public/ +
// src/pwa/ and the app shell never has to know about it. Runs before React
// touches the document, while <title> still holds the build-stamped brand.
import { pwaAssetsFor } from "./productPwa";
import { registerOfflineShell } from "./registerSw";

/** Inject the PWA head tags for the document's stamped brand. Idempotent —
 *  a document that already links a manifest is left alone. */
export function installPwaHead(doc: Document): void {
  if (doc.querySelector('link[rel="manifest"]')) return;
  const assets = pwaAssetsFor(doc.title);

  const link = (rel: string, href: string, type?: string, sizes?: string) => {
    const el = doc.createElement("link");
    el.rel = rel;
    el.href = href;
    if (type) el.type = type;
    if (sizes) el.setAttribute("sizes", sizes);
    doc.head.appendChild(el);
  };
  link("manifest", assets.manifest);
  link("icon", assets.iconSvg, "image/svg+xml");
  link("icon", assets.iconPng, "image/png", "192x192");
  link("apple-touch-icon", assets.iconPng);

  const theme = doc.createElement("meta");
  theme.name = "theme-color";
  theme.content = assets.themeColor;
  doc.head.appendChild(theme);
}

installPwaHead(document);
registerOfflineShell(window, import.meta.env.PROD);
