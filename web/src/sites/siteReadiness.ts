import type { SiteDetail, SitePage, SiteTranslationReadiness } from "./types";

const ACTION_SECTIONS = new Set(["cta", "contact_form", "booking", "tickets", "shop"]);

export interface SiteElementCounts {
  navigation: number;
  hero: number;
  content: number;
  action: number;
}

export interface SiteReadinessResult {
  overall: number;
  foundation: number;
  content: number;
  seo: number;
  accessibility: number;
  branding: number;
  localization: number;
  launch: number;
  elements: SiteElementCounts;
  quality: {
    pages: number;
    seoTitles: number;
    metaDescriptions: number;
    images: number;
    imagesWithAlt: number;
    logo: boolean;
    favicon: boolean;
  };
}

export function calculateSiteReadiness(
  site: SiteDetail,
  pages: SitePage[],
  host: string | null,
  readiness: SiteTranslationReadiness | null,
): SiteReadinessResult {
  const kinds = pages.flatMap((page) => page.sectionKinds ?? []);
  const elements = kinds.reduce<SiteElementCounts>(
    (counts, kind) => {
      if (kind === "nav") counts.navigation += 1;
      else if (kind === "hero") counts.hero += 1;
      else if (ACTION_SECTIONS.has(kind)) counts.action += 1;
      else counts.content += 1;
      return counts;
    },
    { navigation: 0, hero: 0, content: 0, action: 0 },
  );

  const foundation = (host === null ? 0 : 50) + (pages.length === 0 ? 0 : 50);
  const content = Math.min(
    100,
    (elements.hero > 0 ? 25 : 0) +
      Math.min(60, elements.content * 20) +
      (elements.action > 0 ? 15 : 0),
  );
  const localization = translationReadiness(readiness);
  const seoTitles = pages.filter((page) => (page.seoTitle ?? "").trim() !== "").length;
  const metaDescriptions = pages.filter(
    (page) => (page.seoDescription ?? "").trim() !== "",
  ).length;
  const imageCount = pages.reduce((sum, page) => sum + (page.imageCount ?? 0), 0);
  const imagesWithAlt = pages.reduce((sum, page) => sum + (page.imageAltCount ?? 0), 0);
  const theme = site.theme ?? {};
  const pageDivisor = Math.max(1, pages.length);
  const seo = pages.length === 0
    ? 0
    : Math.round(
        25 + (seoTitles / pageDivisor) * 30 + (metaDescriptions / pageDivisor) * 45,
      );
  const accessibility = imageCount === 0 ? 100 : Math.round((imagesWithAlt / imageCount) * 100);
  const branding = (theme.logo ? 50 : 0) + (theme.favicon ? 50 : 0);
  const launch = site.status === "live" ? 100 : 0;
  const overall = Math.round(
    foundation * 0.15 +
      content * 0.3 +
      seo * 0.15 +
      accessibility * 0.1 +
      branding * 0.1 +
      localization * 0.1 +
      launch * 0.1,
  );

  return {
    overall,
    foundation,
    content,
    seo,
    accessibility,
    branding,
    localization,
    launch,
    elements,
    quality: {
      pages: pages.length,
      seoTitles,
      metaDescriptions,
      images: imageCount,
      imagesWithAlt,
      logo: Boolean(theme.logo),
      favicon: Boolean(theme.favicon),
    },
  };
}

function translationReadiness(readiness: SiteTranslationReadiness | null): number {
  if (readiness === null || readiness.totalPages === 0 || readiness.languages.length === 0) {
    return 0;
  }
  const possible = readiness.totalPages * readiness.languages.length;
  const translated = readiness.languages.reduce(
    (sum, language) => sum + Math.min(readiness.totalPages, language.translatedPages),
    0,
  );
  return Math.round((translated / possible) * 100);
}
