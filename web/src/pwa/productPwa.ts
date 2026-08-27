// Which PWA assets belong to the running product surface (ADR 0019). The
// product is chosen at build time (the `@product` alias), and the
// `alo-product-title` vite plugin stamps the marketed brand name into
// `<title>` at build time — before any script runs. That stamped title is the
// one product identity a head script can read without importing the whole
// surface (which would pull every module into the bootstrap chunk), so it is
// the key here. Keep the keys in step with `productTitle` in vite.config.ts.
export interface PwaAssets {
  /** The product's web app manifest, served from `public/`. */
  manifest: string;
  /** Vector brand icon — favicon-quality at any size. */
  iconSvg: string;
  /** 192px raster icon, for apple-touch-icon and legacy favicon use. */
  iconPng: string;
  /** Browser-chrome theme color — the app's warm porcelain canvas, which all
   *  three products render, so it is the same for all of them. */
  themeColor: string;
}

const CANVAS = "#f4f1ec";

const workplace: PwaAssets = {
  manifest: "/manifest-workplace.webmanifest",
  iconSvg: "/icons/workplace.svg",
  iconPng: "/icons/workplace-192.png",
  themeColor: CANVAS,
};

const byBrand: Record<string, PwaAssets> = {
  alomails: {
    manifest: "/manifest-mail.webmanifest",
    iconSvg: "/icons/mail.svg",
    iconPng: "/icons/mail-192.png",
    themeColor: CANVAS,
  },
  "alo workplace": workplace,
  alodrives: {
    manifest: "/manifest-drive.webmanifest",
    iconSvg: "/icons/drive.svg",
    iconPng: "/icons/drive-192.png",
    themeColor: CANVAS,
  },
};

/**
 * Resolve the PWA assets for the build-stamped brand title. An unknown title
 * falls back to the workspace — the same default vite.config.ts applies when
 * `ALO_PRODUCT` is unset.
 */
export function pwaAssetsFor(brandTitle: string): PwaAssets {
  return byBrand[brandTitle] ?? workplace;
}
