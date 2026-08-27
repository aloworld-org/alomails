import { cx } from "../ds";
import type { QuoteLineContent } from "./quoteTableOptions";

export type ImageDraft = Required<Pick<
  QuoteLineContent,
  "image" | "imageFit" | "imagePosition" | "imageZoom"
>> & { key: string };

export const IMAGE_SIZE = {
  small: "size-16",
  medium: "size-24",
  large: "size-32",
} as const;

const IMAGE_POSITION = {
  center: "object-center",
  top: "object-top",
  bottom: "object-bottom",
  left: "object-left",
  right: "object-right",
} as const;

const IMAGE_ZOOM = {
  50: "scale-50", 60: "scale-[.6]", 70: "scale-[.7]", 75: "scale-75",
  80: "scale-[.8]", 90: "scale-90", 100: "scale-100", 110: "scale-110",
  120: "scale-[1.2]", 125: "scale-125", 130: "scale-[1.3]", 140: "scale-[1.4]",
  150: "scale-150", 160: "scale-[1.6]", 170: "scale-[1.7]", 175: "scale-[1.75]",
  180: "scale-[1.8]", 190: "scale-[1.9]", 200: "scale-200",
} as const;

export function normalizeZoom(value: number): keyof typeof IMAGE_ZOOM {
  if (!Number.isFinite(value)) return 100;
  const supported = Object.keys(IMAGE_ZOOM).map(Number);
  return supported.reduce((closest, candidate) =>
    Math.abs(candidate - value) < Math.abs(closest - value) ? candidate : closest,
  ) as keyof typeof IMAGE_ZOOM;
}

export function imageDraft(key: string, content: QuoteLineContent): ImageDraft {
  return {
    key,
    image: content.image,
    imageFit: content.imageFit ?? "cover",
    imagePosition: content.imagePosition ?? "center",
    imageZoom: content.imageZoom ?? 100,
  };
}

export function imageClasses(content: Pick<QuoteLineContent, "imageFit" | "imagePosition" | "imageZoom">): string {
  return cx(
    "size-full transition-transform",
    content.imageFit === "contain" ? "object-contain" : "object-cover",
    IMAGE_POSITION[content.imagePosition ?? "center"],
    IMAGE_ZOOM[normalizeZoom(content.imageZoom ?? 100)],
  );
}
