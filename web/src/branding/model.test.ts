import { describe, expect, test } from "vitest";

import {
  DEFAULT_BRAND_KIT,
  MAX_SUPPORTING_COLORS,
  addSupportingColor,
  brandKitIsValid,
  normalizeBrandKit,
  renameBrandLogo,
} from "./model";

describe("the workspace brand kit", () => {
  test("has one primary, one secondary and no invented supporting colors", () => {
    expect(DEFAULT_BRAND_KIT.foundation.name).toBe("");
    expect(DEFAULT_BRAND_KIT.typography).toEqual({ heading: "inter", body: "inter" });
    expect(DEFAULT_BRAND_KIT.logos).toEqual([]);
    expect(DEFAULT_BRAND_KIT.primaryLogoId).toBeNull();
    expect(DEFAULT_BRAND_KIT.primary.id).toBe("primary");
    expect(DEFAULT_BRAND_KIT.secondary?.id).toBe("secondary");
    expect(DEFAULT_BRAND_KIT.supporting).toEqual([]);
  });

  test("allows supporting colors up to the professional cap", () => {
    let kit = DEFAULT_BRAND_KIT;
    for (let index = 0; index < MAX_SUPPORTING_COLORS + 2; index += 1) {
      kit = addSupportingColor(kit);
    }
    expect(kit.supporting).toHaveLength(MAX_SUPPORTING_COLORS);
  });

  test("normalizes hex values and refuses unnamed or malformed colors", () => {
    const valid = normalizeBrandKit({
      ...DEFAULT_BRAND_KIT,
      primary: { ...DEFAULT_BRAND_KIT.primary, value: "#abcdef" },
    });
    expect(valid.primary.value).toBe("#ABCDEF");
    expect(brandKitIsValid(valid)).toBe(true);
    expect(brandKitIsValid({ ...valid, primary: { ...valid.primary, name: "" } })).toBe(false);
    expect(brandKitIsValid({ ...valid, primary: { ...valid.primary, value: "red" } })).toBe(false);
  });

  test("migrates a stored color-only kit without losing its palette", () => {
    const migrated = normalizeBrandKit({
      primary: { id: "primary", name: "Coral", value: "#e76f51" },
      secondary: null,
      supporting: [],
    });
    expect(migrated.primary.value).toBe("#E76F51");
    expect(migrated.secondary).toBeNull();
    expect(migrated.foundation).toEqual(DEFAULT_BRAND_KIT.foundation);
    expect(migrated.typography).toEqual(DEFAULT_BRAND_KIT.typography);
  });

  test("migrates the original single logo into a primary logo library", () => {
    const migrated = normalizeBrandKit({
      logo: { name: "original.png", mimeType: "image/png", dataUrl: "data:image/png;base64,AAAA" },
    });
    expect(migrated.logos).toEqual([expect.objectContaining({ id: "logo-1", label: "original" })]);
    expect(migrated.primaryLogoId).toBe("logo-1");
  });

  test("renames both the logo label and stored filename while preserving its format", () => {
    const logo = { id: "logo-1", label: "Group 38262", name: "Group 38262.png", mimeType: "image/png" as const, dataUrl: "data:image/png;base64,AAAA" };
    expect(renameBrandLogo(logo, "Company long.png")).toEqual(expect.objectContaining({ label: "Company long", name: "Company long.png" }));
  });
});
