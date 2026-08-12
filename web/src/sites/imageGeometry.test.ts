// The framing arithmetic, tested where a mouse is not needed: every rule the
// store would refuse a write for is a rule this module must never produce.
import { describe, expect, test } from "vitest";

import {
  FULL_BP,
  MIN_EXTENT_BP,
  centerOf,
  clampCrop,
  clampFocal,
  cropBetween,
  focalOf,
  framedImage,
  fromFraction,
  fullCrop,
  isFullFrame,
  moveCrop,
  setCropEdge,
  toPercent,
} from "./imageGeometry";
import type { SectionImage } from "./sections";

const image = (over: Partial<SectionImage> = {}): SectionImage => ({
  blob_id: "Ph0t0aaaaaaaaaaaaaaaa1",
  alt: "",
  ...over,
});

describe("a rectangle the server would accept", () => {
  test("a crop dragged past an edge slides back in at its full size", () => {
    const crop = clampCrop({ x_bp: 9_000, y_bp: -500, width_bp: 5_000, height_bp: 5_000 });
    expect(crop).toEqual({ x_bp: 5_000, y_bp: 0, width_bp: 5_000, height_bp: 5_000 });
    expect(crop.x_bp + crop.width_bp).toBeLessThanOrEqual(FULL_BP);
  });

  test("a crop can never collapse below the minimum extent", () => {
    const crop = clampCrop({ x_bp: 100, y_bp: 100, width_bp: 0, height_bp: 3 });
    expect(crop.width_bp).toBe(MIN_EXTENT_BP);
    expect(crop.height_bp).toBe(MIN_EXTENT_BP);
  });

  test("a drag reads the same rectangle in either direction", () => {
    const forwards = cropBetween({ x_bp: 2_000, y_bp: 1_000 }, { x_bp: 6_000, y_bp: 8_000 });
    const backwards = cropBetween({ x_bp: 6_000, y_bp: 8_000 }, { x_bp: 2_000, y_bp: 1_000 });
    expect(forwards).toEqual(backwards);
    expect(forwards).toEqual({ x_bp: 2_000, y_bp: 1_000, width_bp: 4_000, height_bp: 7_000 });
  });

  test("moving stops at the edge instead of shrinking", () => {
    const crop = { x_bp: 7_000, y_bp: 0, width_bp: 3_000, height_bp: 3_000 };
    const moved = moveCrop(crop, 2_000, -2_000);
    expect(moved).toEqual({ x_bp: 7_000, y_bp: 0, width_bp: 3_000, height_bp: 3_000 });
  });

  test("a typed width that would leave the picture pulls the left edge back", () => {
    const crop = { x_bp: 6_000, y_bp: 0, width_bp: 2_000, height_bp: 10_000 };
    expect(setCropEdge(crop, "width_bp", 80)).toEqual({
      x_bp: 2_000,
      y_bp: 0,
      width_bp: 8_000,
      height_bp: 10_000,
    });
  });

  test("a percent box that is emptied does not produce NaN geometry", () => {
    const crop = fullCrop();
    expect(setCropEdge(crop, "x_bp", Number.NaN)).toEqual(fullCrop());
  });
});

describe("the focal point and its crop can never contradict each other", () => {
  test("a point outside the rectangle is pulled onto it", () => {
    const crop = { x_bp: 2_000, y_bp: 2_000, width_bp: 4_000, height_bp: 4_000 };
    expect(clampFocal({ x_bp: 9_000, y_bp: 100 }, crop)).toEqual({ x_bp: 6_000, y_bp: 2_000 });
  });

  test("an unstated focal point reads as the centre of the crop, not of the source", () => {
    const framed = image({ crop: { x_bp: 0, y_bp: 0, width_bp: 4_000, height_bp: 10_000 } });
    expect(focalOf(framed)).toEqual({ x_bp: 2_000, y_bp: 5_000 });
    expect(centerOf(fullCrop())).toEqual({ x_bp: 5_000, y_bp: 5_000 });
  });
});

describe("what gets stored", () => {
  test("framing the whole picture stores no crop at all", () => {
    const framed = framedImage(image(), fullCrop(), { x_bp: 5_000, y_bp: 5_000 }, false);
    expect(framed.crop).toBeUndefined();
    expect(framed.focal).toBeUndefined();
    expect(isFullFrame(fullCrop())).toBe(true);
  });

  test("a real frame is stored, and a chosen focal point with it", () => {
    const crop = { x_bp: 1_000, y_bp: 0, width_bp: 8_000, height_bp: 10_000 };
    const framed = framedImage(image({ alt: "A rack of loaves" }), crop, { x_bp: 0, y_bp: 0 }, true);
    expect(framed.crop).toEqual(crop);
    // Chosen where the frame does not reach: kept, but pulled onto the frame.
    expect(framed.focal).toEqual({ x_bp: 1_000, y_bp: 0 });
    expect(framed.alt).toBe("A rack of loaves");
  });

  test("every other prop of the image survives framing untouched", () => {
    const original = image({ alt: "", decorative: true });
    expect(framedImage(original, fullCrop(), { x_bp: 0, y_bp: 0 }, false).decorative).toBe(true);
  });
});

describe("reading pointer positions", () => {
  test("a fraction of the rendered box becomes a point of the source", () => {
    expect(fromFraction(0.5, 0.25)).toEqual({ x_bp: 5_000, y_bp: 2_500 });
    // A pointer dragged outside the box is still a point of the picture.
    expect(fromFraction(-0.4, 1.8)).toEqual({ x_bp: 0, y_bp: 10_000 });
  });

  test("basis points read back as whole percents", () => {
    expect(toPercent(2_550)).toBe(26);
  });
});
