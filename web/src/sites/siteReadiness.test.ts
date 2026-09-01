import { describe, expect, test } from "vitest";

import { calculateSiteReadiness } from "./siteReadiness";
import type { SiteDetail, SitePage, SiteTranslationReadiness } from "./types";

const site = { status: "draft", theme: {} } as SiteDetail;
const readiness: SiteTranslationReadiness = {
  defaultLocale: "en",
  totalPages: 1,
  languages: [{ locale: "en", translatedPages: 1, ready: true }],
};

function page(sectionKinds: string[]): SitePage {
  return {
    id: "page-1",
    slug: "",
    title: "Home",
    home: true,
    seoTitle: null,
    seoDescription: null,
    sectionKinds,
  };
}

describe("site launch readiness", () => {
  test("does not treat a navigation-only page as meaningful content", () => {
    const result = calculateSiteReadiness(site, [page(["nav"])], "example.alosites.com", readiness);

    expect(result.content).toBe(0);
    expect(result.overall).toBe(39);
    expect(result.elements).toEqual({ navigation: 1, hero: 0, content: 0, action: 0 });
  });

  test("recognizes introduction, content, and conversion sections separately", () => {
    const result = calculateSiteReadiness(
      site,
      [page(["nav", "hero", "features", "text_image", "contact_form"])],
      "example.alosites.com",
      readiness,
    );

    expect(result.content).toBe(80);
    expect(result.elements).toEqual({ navigation: 1, hero: 1, content: 2, action: 1 });
  });
});
