import { describe, expect, test } from "vitest";

import {
  DEFAULT_BRAND_KIT,
  MAX_SUPPORTING_COLORS,
  addSupportingColor,
  brandKitIsValid,
  normalizeBrandKit,
} from "./model";

describe("the workspace brand kit", () => {
  test("has one primary, one secondary and no invented supporting colors", () => {
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
});
