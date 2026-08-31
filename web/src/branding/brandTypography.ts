import type { BrandFont } from "./model";

const FONT_STACKS: Record<BrandFont, string> = {
  inter: "Inter, ui-sans-serif, system-ui, sans-serif",
  arial: "Arial, Helvetica, sans-serif",
  georgia: "Georgia, Times New Roman, serif",
  garamond: "Garamond, Georgia, serif",
};

export const BRAND_FONTS: readonly BrandFont[] = ["inter", "arial", "georgia", "garamond"];

export function brandFontStack(font: BrandFont): string {
  return FONT_STACKS[font];
}
