import { describe, expect, it } from "vitest";

import { clampWidth } from "./usePanelWidth";

describe("clampWidth", () => {
  it("keeps a value within range unchanged", () => {
    expect(clampWidth(300, 176, 420)).toBe(300);
  });
  it("clamps below the minimum", () => {
    expect(clampWidth(100, 176, 420)).toBe(176);
  });
  it("clamps above the maximum", () => {
    expect(clampWidth(999, 176, 420)).toBe(420);
  });
  it("accepts the exact bounds", () => {
    expect(clampWidth(176, 176, 420)).toBe(176);
    expect(clampWidth(420, 176, 420)).toBe(420);
  });
});
