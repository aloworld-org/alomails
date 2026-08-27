import { describe, expect, it } from "vitest";
import { IMAGE_BLOCK_ZOOM, IMAGE_COLUMN_GRID, IMAGE_FRAME } from "./imageBlockLayout";

describe("image block layout", () => {
  it("maps persisted options to Tailwind presentation", () => {
    expect(Object.keys(IMAGE_FRAME)).toEqual(["natural", "landscape", "square"]);
    expect(IMAGE_BLOCK_ZOOM[100]).toBe("scale-100");
    expect(IMAGE_COLUMN_GRID["50-50"].left).toBe("md:grid-cols-2");
  });
});
