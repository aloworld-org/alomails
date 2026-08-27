// The installability contract, held by tests: every product surface has a
// manifest that satisfies Chromium's install criteria (name, start_url,
// standalone display, a 192px and a 512px PNG icon plus a maskable one), every
// icon the manifests reference exists on disk at its declared pixel size, and
// index.html loads the head installer before the app. Chromium's criteria are
// structural, so this suite is the automatable half of "passing Chromium
// installability"; the click-the-install-button half is the owner's.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { installPwaHead } from "./install";
import { pwaAssetsFor } from "./productPwa";

const BRANDS = ["alomails", "alo workplace", "alodrives"] as const;

// Plain path joins, not `new URL(..., import.meta.url)`: Vite statically
// rewrites the latter as an asset reference, which resolves to `undefined`
// for files outside the module graph (everything under `public/`).
const HERE = dirname(fileURLToPath(import.meta.url));

/** Resolve a `/`-rooted public asset path to the file in `web/public`. */
function publicFile(href: string): string {
  return join(HERE, "../../public", href);
}

function pngSize(path: string): { width: number; height: number } {
  const bytes = readFileSync(path);
  // PNG signature (8 bytes) + IHDR length/type (8 bytes) precede the fixed
  // big-endian width/height pair.
  expect(bytes.subarray(1, 4).toString("ascii")).toBe("PNG");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

interface ManifestIcon {
  src: string;
  sizes: string;
  type: string;
  purpose: string;
}

interface Manifest {
  name: string;
  short_name: string;
  start_url: string;
  scope: string;
  display: string;
  background_color: string;
  theme_color: string;
  icons: ManifestIcon[];
}

describe("product manifests", () => {
  it("each brand resolves to its own manifest; unknown titles fall back to the workspace", () => {
    const manifests = BRANDS.map((b) => pwaAssetsFor(b).manifest);
    expect(new Set(manifests).size).toBe(BRANDS.length);
    expect(pwaAssetsFor("something else").manifest).toBe(
      pwaAssetsFor("alo workplace").manifest,
    );
  });

  for (const brand of BRANDS) {
    describe(brand, () => {
      const assets = pwaAssetsFor(brand);
      const manifest = JSON.parse(
        readFileSync(publicFile(assets.manifest), "utf8"),
      ) as Manifest;

      it("carries the brand name and Chromium's required members", () => {
        expect(manifest.name).toBe(brand);
        expect(manifest.short_name.length).toBeGreaterThan(0);
        expect(manifest.start_url).toBe("/");
        expect(manifest.scope).toBe("/");
        expect(manifest.display).toBe("standalone");
        expect(manifest.background_color).toMatch(/^#[0-9a-f]{6}$/);
        expect(manifest.theme_color).toMatch(/^#[0-9a-f]{6}$/);
      });

      it("declares the 192 + 512 raster icons and a maskable one", () => {
        const png = manifest.icons.filter((i) => i.type === "image/png");
        const any = png.filter((i) => i.purpose === "any").map((i) => i.sizes);
        expect(any).toContain("192x192");
        expect(any).toContain("512x512");
        expect(
          png.some((i) => i.purpose === "maskable" && i.sizes === "512x512"),
        ).toBe(true);
      });

      it("every referenced icon exists at its declared size", () => {
        for (const icon of manifest.icons) {
          const file = publicFile(icon.src);
          if (icon.type === "image/png") {
            const declared = Number(icon.sizes.split("x")[0]);
            expect(pngSize(file)).toEqual({ width: declared, height: declared });
          } else {
            expect(readFileSync(file, "utf8")).toContain("<svg");
          }
        }
      });

      it("the head assets exist too", () => {
        expect(readFileSync(publicFile(assets.iconSvg), "utf8")).toContain("<svg");
        expect(pngSize(publicFile(assets.iconPng)).width).toBe(192);
      });
    });
  }
});

describe("installPwaHead", () => {
  function freshDocument(title: string): Document {
    return document.implementation.createHTMLDocument(title);
  }

  it("links the stamped brand's manifest, icons, and theme color", () => {
    const doc = freshDocument("alomails");
    installPwaHead(doc);
    expect(
      doc.querySelector('link[rel="manifest"]')?.getAttribute("href"),
    ).toBe("/manifest-mail.webmanifest");
    expect(
      doc.querySelector('link[rel="icon"][type="image/svg+xml"]')?.getAttribute("href"),
    ).toBe("/icons/mail.svg");
    expect(
      doc.querySelector('link[rel="apple-touch-icon"]')?.getAttribute("href"),
    ).toBe("/icons/mail-192.png");
    expect(
      doc.querySelector('meta[name="theme-color"]')?.getAttribute("content"),
    ).toBe("#f4f1ec");
  });

  it("an unknown title gets the workspace manifest", () => {
    const doc = freshDocument("dev tab");
    installPwaHead(doc);
    expect(
      doc.querySelector('link[rel="manifest"]')?.getAttribute("href"),
    ).toBe("/manifest-workplace.webmanifest");
  });

  it("is idempotent", () => {
    const doc = freshDocument("alo workplace");
    installPwaHead(doc);
    installPwaHead(doc);
    expect(doc.querySelectorAll('link[rel="manifest"]')).toHaveLength(1);
    expect(doc.querySelectorAll('meta[name="theme-color"]')).toHaveLength(1);
  });
});

describe("index.html", () => {
  it("loads the PWA installer before the app entry", () => {
    const html = readFileSync(join(HERE, "../../index.html"), "utf8");
    const pwa = html.indexOf("/src/pwa/install.ts");
    const app = html.indexOf("/src/main.tsx");
    expect(pwa).toBeGreaterThan(-1);
    expect(app).toBeGreaterThan(-1);
    expect(pwa).toBeLessThan(app);
  });
});
