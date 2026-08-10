import { describe, expect, test } from "vitest";

import { en } from "../i18n/en";
import { fr } from "../i18n/fr";
import { nl } from "../i18n/nl";

const sitesKeys = Object.keys(en).filter(
  (key) => key === "moduleSites" || key.startsWith("sites"),
);

describe("Sites translations", () => {
  test.each([
    ["French", fr],
    ["Dutch", nl],
  ])("%s covers every Sites surface", (_language, catalog) => {
    expect(sitesKeys.filter((key) => !Object.hasOwn(catalog, key))).toEqual([]);
  });
});
