export interface BrandColor {
  id: string;
  name: string;
  value: string;
}

export interface BrandFoundation {
  name: string;
  tagline: string;
  purpose: string;
  audience: string;
  positioning: string;
  personality: string;
  voice: string;
}

export type BrandFont = "inter" | "arial" | "georgia" | "garamond";

export interface BrandTypography {
  heading: BrandFont;
  body: BrandFont;
}

export interface BrandLogo {
  id: string;
  name: string;
  label: string;
  mimeType: "image/png" | "image/jpeg" | "image/webp" | "image/svg+xml";
  dataUrl: string;
}

export interface BrandKit {
  foundation: BrandFoundation;
  logos: BrandLogo[];
  primaryLogoId: string | null;
  typography: BrandTypography;
  primary: BrandColor;
  secondary: BrandColor | null;
  supporting: BrandColor[];
}

export const MAX_SUPPORTING_COLORS = 3;
export const MAX_BRAND_LOGOS = 8;
export const MAX_LOGO_BYTES = 500 * 1024;

export const DEFAULT_BRAND_KIT: BrandKit = {
  foundation: {
    name: "",
    tagline: "",
    purpose: "",
    audience: "",
    positioning: "",
    personality: "",
    voice: "",
  },
  logos: [],
  primaryLogoId: null,
  typography: { heading: "inter", body: "inter" },
  primary: { id: "primary", name: "Primary", value: "#E76F51" },
  secondary: { id: "secondary", name: "Secondary", value: "#102A43" },
  supporting: [],
};

const BRAND_FONTS: readonly BrandFont[] = ["inter", "arial", "georgia", "garamond"];

export function isHexColor(value: string): boolean {
  return /^#[0-9A-F]{6}$/i.test(value);
}

type LegacyBrandKit = Partial<BrandKit> & { logo?: Omit<BrandLogo, "id" | "label"> | BrandLogo | null };

export function normalizeBrandKit(value: LegacyBrandKit): BrandKit {
  const secondary = value.secondary;
  const logos = normalizeLogos(value.logos, value.logo);
  const requestedPrimary = value.primaryLogoId;
  return {
    foundation: normalizeFoundation(value.foundation),
    logos,
    primaryLogoId: logos.some((logo) => logo.id === requestedPrimary) ? requestedPrimary! : logos[0]?.id ?? null,
    typography: normalizeTypography(value.typography),
    primary: normalizeColor(value.primary, DEFAULT_BRAND_KIT.primary),
    secondary:
      secondary === null
        ? null
        : normalizeColor(secondary, DEFAULT_BRAND_KIT.secondary!),
    supporting: (value.supporting ?? [])
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
  return colors.every((color) => color.name.trim() !== "" && isHexColor(color.value))
    && BRAND_FONTS.includes(kit.typography.heading)
    && BRAND_FONTS.includes(kit.typography.body)
    && kit.logos.length <= MAX_BRAND_LOGOS
    && kit.logos.every(isSafeLogo)
    && (kit.primaryLogoId === null ? kit.logos.length === 0 : kit.logos.some((logo) => logo.id === kit.primaryLogoId));
}

export function primaryBrandLogo(kit: BrandKit): BrandLogo | null {
  return kit.logos.find((logo) => logo.id === kit.primaryLogoId) ?? kit.logos[0] ?? null;
}

export function renameBrandLogo(logo: BrandLogo, requestedName: string): BrandLogo {
  const label = requestedName.trim().replace(/\.(?:png|jpe?g|webp|svg)$/i, "").trim().slice(0, 48);
  if (label === "") return logo;
  const extension = logo.name.match(/\.[^.]+$/)?.[0] ?? extensionForLogo(logo.mimeType);
  return { ...logo, label, name: `${label}${extension}` };
}

function normalizeColor(value: BrandColor | undefined, fallback: BrandColor): BrandColor {
  if (value === undefined) return { ...fallback };
  return {
    id: value.id.trim() || fallback.id,
    name: value.name.trim() || fallback.name,
    value: isHexColor(value.value) ? value.value.toUpperCase() : fallback.value,
  };
}

function normalizeFoundation(value: BrandFoundation | undefined): BrandFoundation {
  if (value === undefined) return { ...DEFAULT_BRAND_KIT.foundation };
  return {
    name: normalizeText(value.name, 120),
    tagline: normalizeText(value.tagline, 180),
    purpose: normalizeText(value.purpose, 600),
    audience: normalizeText(value.audience, 600),
    positioning: normalizeText(value.positioning, 600),
    personality: normalizeText(value.personality, 300),
    voice: normalizeText(value.voice, 600),
  };
}

function normalizeTypography(value: BrandTypography | undefined): BrandTypography {
  if (value === undefined) return { ...DEFAULT_BRAND_KIT.typography };
  return {
    heading: BRAND_FONTS.includes(value.heading) ? value.heading : DEFAULT_BRAND_KIT.typography.heading,
    body: BRAND_FONTS.includes(value.body) ? value.body : DEFAULT_BRAND_KIT.typography.body,
  };
}

function normalizeLogos(values: BrandLogo[] | undefined, legacy: LegacyBrandKit["logo"]): BrandLogo[] {
  const candidates = values ?? (legacy === undefined || legacy === null ? [] : [legacy]);
  return candidates.slice(0, MAX_BRAND_LOGOS).flatMap((candidate, index) => {
    const logo: BrandLogo = {
      id: "id" in candidate && candidate.id.trim() !== "" ? candidate.id : `logo-${index + 1}`,
      name: candidate.name.trim(),
      label: "label" in candidate && candidate.label.trim() !== "" ? candidate.label.trim().slice(0, 48) : candidate.name.replace(/\.[^.]+$/, "").trim().slice(0, 48),
      mimeType: candidate.mimeType,
      dataUrl: candidate.dataUrl,
    };
    return isSafeLogo(logo) ? [logo] : [];
  });
}

function normalizeText(value: string | undefined, maximum: number): string {
  return typeof value === "string" ? value.trim().slice(0, maximum) : "";
}

function isSafeLogo(value: BrandLogo): boolean {
  return value.id.trim() !== ""
    && value.name.trim() !== ""
    && value.label.trim() !== ""
    && ["image/png", "image/jpeg", "image/webp", "image/svg+xml"].includes(value.mimeType)
    && value.dataUrl.startsWith(`data:${value.mimeType};base64,`)
    && value.dataUrl.length <= Math.ceil(MAX_LOGO_BYTES * 4 / 3) + 64;
}

function extensionForLogo(mimeType: BrandLogo["mimeType"]): string {
  if (mimeType === "image/jpeg") return ".jpg";
  if (mimeType === "image/svg+xml") return ".svg";
  if (mimeType === "image/webp") return ".webp";
  return ".png";
}
