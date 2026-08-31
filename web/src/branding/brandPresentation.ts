import type { CSSProperties } from "react";

import { strings } from "../i18n";
import { readableInk } from "./colorTools";
import { brandFontStack } from "./brandTypography";
import type { BrandKit } from "./model";

export function presentedBrandName(kit: BrandKit): string {
  return kit.foundation.name.trim() || strings.brandingSampleName;
}

export function presentedTagline(kit: BrandKit): string {
  return kit.foundation.tagline.trim() || strings.brandingSampleTagline;
}

export function brandPresentationVariables(kit: BrandKit): CSSProperties {
  const primary = kit.primary.value;
  const secondary = kit.secondary?.value ?? primary;
  return {
    "--brand-primary": primary,
    "--brand-primary-ink": readableInk(primary),
    "--brand-secondary": secondary,
    "--brand-secondary-ink": readableInk(secondary),
    "--brand-heading": brandFontStack(kit.typography.heading),
    "--brand-body": brandFontStack(kit.typography.body),
  } as CSSProperties;
}
