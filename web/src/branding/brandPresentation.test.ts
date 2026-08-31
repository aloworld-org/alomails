import { expect, test } from "vitest";

import { strings } from "../i18n";
import { presentedBrandName, presentedTagline } from "./brandPresentation";
import { DEFAULT_BRAND_KIT } from "./model";

test("brand presentation provides localized examples for an empty foundation", () => {
  expect(presentedBrandName(DEFAULT_BRAND_KIT)).toBe(strings.brandingSampleName);
  expect(presentedTagline(DEFAULT_BRAND_KIT)).toBe(strings.brandingSampleTagline);
});
