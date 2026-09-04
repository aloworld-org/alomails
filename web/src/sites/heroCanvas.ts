import {
  clampCrop,
  clampFocal,
  cropOf,
  focalOf,
  framedImage,
  moveCrop,
} from "./imageGeometry";
import type { HeroSection } from "./sections";
import type { HeroCanvasAction } from "./sectionMove";

const STEP = 500;

/** Applies one canvas-toolbar gesture without resetting framing state. */
export function heroAfterCanvasAction(
  hero: HeroSection,
  action: HeroCanvasAction,
): HeroSection | null {
  if (action.startsWith("align_"))
    return { ...hero, alignment: action.slice(6) as HeroSection["alignment"] };
  if (action.startsWith("width_"))
    return {
      ...hero,
      content_width: action.slice(6) as HeroSection["content_width"],
    };
  if (action.startsWith("background_")) {
    const background = action.slice(11) as NonNullable<
      HeroSection["appearance"]
    >["background"];
    return {
      ...hero,
      appearance: {
        primary_button: "accent_1",
        primary_button_hover: "accent_2",
        secondary_button: "accent_3",
        secondary_button_hover: "accent_1",
        ...hero.appearance,
        background,
      },
    };
  }
  if (hero.image === undefined) return null;

  const crop = cropOf(hero.image);
  const focal = focalOf(hero.image);
  const [dx, dy] =
    action === "move_left"
      ? [STEP, 0]
      : action === "move_right"
        ? [-STEP, 0]
        : action === "move_up"
          ? [0, STEP]
          : action === "move_down"
            ? [0, -STEP]
            : [0, 0];
  const nextCrop =
    action === "zoom_in"
      ? clampCrop({
          x_bp: crop.x_bp + STEP,
          y_bp: crop.y_bp + STEP,
          width_bp: crop.width_bp - STEP * 2,
          height_bp: crop.height_bp - STEP * 2,
        })
      : action === "zoom_out"
        ? clampCrop({
            x_bp: crop.x_bp - STEP,
            y_bp: crop.y_bp - STEP,
            width_bp: crop.width_bp + STEP * 2,
            height_bp: crop.height_bp + STEP * 2,
          })
        : moveCrop(crop, dx, dy);
  const nextFocal =
    dx === 0 && dy === 0
      ? clampFocal(focal, nextCrop)
      : clampFocal({ x_bp: focal.x_bp + dx, y_bp: focal.y_bp + dy }, nextCrop);
  return {
    ...hero,
    image: framedImage(
      hero.image,
      nextCrop,
      nextFocal,
      hero.image.focal !== undefined,
    ),
  };
}
