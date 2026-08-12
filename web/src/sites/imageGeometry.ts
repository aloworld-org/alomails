// The arithmetic behind the framing control: crop rectangles and focal points
// in basis points, exactly as `site_model` stores them.
//
// It lives apart from the component for two reasons. Dragging a corner and
// typing "25" into a percent box are the same operation on the same numbers,
// and only one of them can be tested in a DOM with no layout — so the rules
// (never leave the picture, never collapse to nothing, never strand the focal
// point outside its own crop) are written once, here, where a test can reach
// them without a mouse.
//
// It holds no validation of its own in the sense the store means it: the
// server rules on every write and names the rule it broke. What this file
// does is keep the editor from *offering* a rectangle the server would have
// to refuse.
import type { ImageCrop, ImageFocalPoint, SectionImage } from "./sections";

/** The whole width or height of the source, in basis points — the unit the
 *  schema stores geometry in (ten-thousandths, never pixels, so the same
 *  crop survives every derivative width). */
export const FULL_BP = 10_000;

/** The smallest crop the schema accepts on either axis (1% of the source). */
export const MIN_EXTENT_BP = 100;

/** One nudge of an arrow key: 1% of the source. */
export const NUDGE_BP = 100;

/** The whole picture — what an absent crop means. */
export const fullCrop = (): ImageCrop => ({
  x_bp: 0,
  y_bp: 0,
  width_bp: FULL_BP,
  height_bp: FULL_BP,
});

/** Whether a rectangle is the whole picture (an unframed image stores no
 *  crop at all, rather than a crop that happens to cover everything). */
export const isFullFrame = (crop: ImageCrop): boolean =>
  crop.x_bp === 0 && crop.y_bp === 0 && crop.width_bp === FULL_BP && crop.height_bp === FULL_BP;

/** The visible rectangle of an image: its crop, or the whole picture. */
export const cropOf = (image: SectionImage): ImageCrop => image.crop ?? fullCrop();

/** The point kept in frame: the stored focal point, or the crop's own
 *  centre — the same fallback the renderer applies. */
export const focalOf = (image: SectionImage): ImageFocalPoint =>
  image.focal ?? centerOf(cropOf(image));

/** The midpoint of a rectangle. */
export const centerOf = (crop: ImageCrop): ImageFocalPoint => ({
  x_bp: crop.x_bp + Math.round(crop.width_bp / 2),
  y_bp: crop.y_bp + Math.round(crop.height_bp / 2),
});

const clamp = (value: number, low: number, high: number): number =>
  Math.min(high, Math.max(low, Math.round(value)));

/**
 * The nearest rectangle to `crop` that the schema accepts: whole basis
 * points, at least [`MIN_EXTENT_BP`] on each axis, entirely inside the
 * picture. Width is settled before position, so a rectangle dragged past an
 * edge slides back in rather than silently shrinking.
 */
export function clampCrop(crop: ImageCrop): ImageCrop {
  const width = clamp(crop.width_bp, MIN_EXTENT_BP, FULL_BP);
  const height = clamp(crop.height_bp, MIN_EXTENT_BP, FULL_BP);
  return {
    x_bp: clamp(crop.x_bp, 0, FULL_BP - width),
    y_bp: clamp(crop.y_bp, 0, FULL_BP - height),
    width_bp: width,
    height_bp: height,
  };
}

/** The rectangle between two points of the picture (a drag), as basis
 *  points. Either corner may be dragged in either direction. */
export function cropBetween(
  from: { x_bp: number; y_bp: number },
  to: { x_bp: number; y_bp: number },
): ImageCrop {
  const x = Math.min(from.x_bp, to.x_bp);
  const y = Math.min(from.y_bp, to.y_bp);
  return clampCrop({
    x_bp: x,
    y_bp: y,
    width_bp: Math.abs(to.x_bp - from.x_bp),
    height_bp: Math.abs(to.y_bp - from.y_bp),
  });
}

/** Moves a rectangle without resizing it: at an edge it stops rather than
 *  shrinking, which is what a person dragging a frame expects. */
export function moveCrop(crop: ImageCrop, dx: number, dy: number): ImageCrop {
  return {
    ...crop,
    x_bp: clamp(crop.x_bp + dx, 0, FULL_BP - crop.width_bp),
    y_bp: clamp(crop.y_bp + dy, 0, FULL_BP - crop.height_bp),
  };
}

/** Which number a percent box edits. */
export type CropEdge = "x_bp" | "y_bp" | "width_bp" | "height_bp";

/** Sets one number of the rectangle from a typed percentage, then puts the
 *  whole rectangle back inside the picture. */
export function setCropEdge(crop: ImageCrop, edge: CropEdge, percent: number): ImageCrop {
  const value = Number.isFinite(percent) ? Math.round(percent * 100) : 0;
  const next = { ...crop, [edge]: value };
  if (edge === "width_bp") next.x_bp = Math.min(next.x_bp, FULL_BP - MIN_EXTENT_BP);
  if (edge === "height_bp") next.y_bp = Math.min(next.y_bp, FULL_BP - MIN_EXTENT_BP);
  return clampCrop(next);
}

/** Pulls a focal point onto its crop. A point outside the rectangle it
 *  belongs to is the one contradiction the schema refuses outright, so
 *  re-framing moves the point rather than producing a page that cannot save. */
export function clampFocal(focal: ImageFocalPoint, crop: ImageCrop): ImageFocalPoint {
  return {
    x_bp: clamp(focal.x_bp, crop.x_bp, crop.x_bp + crop.width_bp),
    y_bp: clamp(focal.y_bp, crop.y_bp, crop.y_bp + crop.height_bp),
  };
}

/** Basis points as a CSS percentage string. */
export const asPercent = (bp: number): string => `${bp / 100}%`;

/** Basis points as a number of percent, rounded for a number input. */
export const toPercent = (bp: number): number => Math.round(bp / 100);

/** A point of the picture from a fraction of the rendered box (0–1). */
export const fromFraction = (fx: number, fy: number): { x_bp: number; y_bp: number } => ({
  x_bp: clamp(fx * FULL_BP, 0, FULL_BP),
  y_bp: clamp(fy * FULL_BP, 0, FULL_BP),
});

/**
 * The image as it should be stored after framing: a crop covering the whole
 * picture, and a focal point sitting exactly at the centre of the crop, are
 * both left out entirely — an absent value already means that, and writing it
 * would put geometry on every image anybody ever touched.
 */
export function framedImage(
  image: SectionImage,
  crop: ImageCrop,
  focal: ImageFocalPoint,
  focalSet: boolean,
): SectionImage {
  const bounded = clampCrop(crop);
  const point = clampFocal(focal, bounded);
  return {
    ...image,
    crop: isFullFrame(bounded) ? undefined : bounded,
    focal: focalSet ? point : undefined,
  };
}
