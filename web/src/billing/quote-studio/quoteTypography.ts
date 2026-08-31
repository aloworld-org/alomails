import type { BrandFont, BrandKit } from "../../branding/model";
import type { QuoteStudioTheme } from "./QuoteStudioDesign";

export interface QuoteTypographyRoles { headingFont: BrandFont; bodyFont: BrandFont }

export function themeQuoteTypography(theme: QuoteStudioTheme): QuoteTypographyRoles {
  return theme === "editorial" ? { headingFont: "georgia", bodyFont: "inter" } : { headingFont: "inter", bodyFont: "inter" };
}

export function importBrandQuoteTypography(brand: BrandKit): QuoteTypographyRoles {
  return { headingFont: brand.typography.heading, bodyFont: brand.typography.body };
}
