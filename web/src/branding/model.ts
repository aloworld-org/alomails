export interface BrandColor {
  id: string;
  name: string;
  value: string;
}

export interface BrandKit {
  primary: BrandColor;
  secondary: BrandColor | null;
  supporting: BrandColor[];
}

export const MAX_SUPPORTING_COLORS = 3;

export const DEFAULT_BRAND_KIT: BrandKit = {
  primary: { id: "primary", name: "Primary", value: "#E76F51" },
  secondary: { id: "secondary", name: "Secondary", value: "#102A43" },
  supporting: [],
};

export function isHexColor(value: string): boolean {
  return /^#[0-9A-F]{6}$/i.test(value);
}

export function normalizeBrandKit(value: BrandKit): BrandKit {
  return {
    primary: normalizeColor(value.primary, DEFAULT_BRAND_KIT.primary),
    secondary:
      value.secondary === null
        ? null
        : normalizeColor(value.secondary, DEFAULT_BRAND_KIT.secondary!),
    supporting: value.supporting
      .slice(0, MAX_SUPPORTING_COLORS)
      .map((color, index) =>
        normalizeColor(color, {
          id: `supporting-${index + 1}`,
          name: `Supporting ${index + 1}`,
          value: "#6B7280",
        }),
      ),
  };
}

export function addSupportingColor(kit: BrandKit): BrandKit {
  if (kit.supporting.length >= MAX_SUPPORTING_COLORS) return kit;
  const number = kit.supporting.length + 1;
  return {
    ...kit,
    supporting: [
      ...kit.supporting,
      {
        id: `supporting-${Date.now()}-${number}`,
        name: `Supporting ${number}`,
        value: "#6B7280",
      },
    ],
  };
}

export function brandKitIsValid(kit: BrandKit): boolean {
  const colors = [kit.primary, ...(kit.secondary === null ? [] : [kit.secondary]), ...kit.supporting];
  return colors.every((color) => color.name.trim() !== "" && isHexColor(color.value));
}

function normalizeColor(value: BrandColor, fallback: BrandColor): BrandColor {
  return {
    id: value.id.trim() || fallback.id,
    name: value.name.trim() || fallback.name,
    value: isHexColor(value.value) ? value.value.toUpperCase() : fallback.value,
  };
}
