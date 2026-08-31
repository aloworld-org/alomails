import { toneScale } from "../../branding/colorTools";
import type { BrandKit } from "../../branding/model";
import type { QuoteStudioColors } from "./QuoteStudioDesign";

export function importBrandQuoteColors(brand: BrandKit, current: QuoteStudioColors): QuoteStudioColors {
  const primary = brand.primary.value;
  const secondary = brand.secondary?.value ?? current.text;
  return {
    ...current,
    accent: primary,
    contactIcons: primary,
    text: secondary,
    tableHeader: toneScale(primary)[0] ?? current.tableHeader,
    bulletMarker: primary,
    numberMarker: secondary,
  };
}
