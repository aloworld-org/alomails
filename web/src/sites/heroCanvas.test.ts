import { describe, expect, test } from "vitest";

import { heroAfterCanvasAction } from "./heroCanvas";
import type { HeroSection } from "./sections";

const HERO: HeroSection = {
  type: "hero",
  heading: "A fan",
  image: {
    blob_id: "image-1",
    alt: "A fan",
    crop: { x_bp: 1_000, y_bp: 1_000, width_bp: 7_000, height_bp: 7_000 },
    focal: { x_bp: 3_000, y_bp: 4_000 },
  },
};

describe("Hero canvas controls", () => {
  test("moving left and up preserves the focal point's place inside the frame", () => {
    const left = heroAfterCanvasAction(HERO, "move_left")!;
    expect(left.image?.crop).toEqual({
      x_bp: 1_500,
      y_bp: 1_000,
      width_bp: 7_000,
      height_bp: 7_000,
    });
    expect(left.image?.focal).toEqual({ x_bp: 3_500, y_bp: 4_000 });

    const up = heroAfterCanvasAction(left, "move_up")!;
    expect(up.image?.crop?.y_bp).toBe(1_500);
    expect(up.image?.focal).toEqual({ x_bp: 3_500, y_bp: 4_500 });
  });

  test("zoom keeps an authored focal point instead of recentering it", () => {
    const zoomed = heroAfterCanvasAction(HERO, "zoom_in")!;
    expect(zoomed.image?.crop).toEqual({
      x_bp: 1_500,
      y_bp: 1_500,
      width_bp: 6_000,
      height_bp: 6_000,
    });
    expect(zoomed.image?.focal).toEqual(HERO.image?.focal);
  });

  test("alignment, width and background change only their named property", () => {
    expect(heroAfterCanvasAction(HERO, "align_right")?.alignment).toBe("right");
    expect(heroAfterCanvasAction(HERO, "width_wide")?.content_width).toBe(
      "wide",
    );
    expect(
      heroAfterCanvasAction(HERO, "background_accent_3")?.appearance
        ?.background,
    ).toBe("accent_3");
  });

  test("image-only commands do nothing when there is no image", () => {
    expect(
      heroAfterCanvasAction({ type: "hero", heading: "Hello" }, "move_up"),
    ).toBeNull();
  });
});
