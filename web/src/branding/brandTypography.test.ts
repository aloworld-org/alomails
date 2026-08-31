import { expect, test } from "vitest";

import { brandFontStack } from "./brandTypography";

test("brand fonts resolve to durable fallback stacks", () => {
  expect(brandFontStack("georgia")).toContain("serif");
  expect(brandFontStack("inter")).toContain("system-ui");
});
