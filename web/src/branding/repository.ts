import { DEFAULT_BRAND_KIT, normalizeBrandKit, type BrandKit } from "./model";

const STORAGE_KEY = "alo-workspace-brand-kit-v1";
export const BRAND_KIT_CHANGED = "alo-brand-kit-changed";

export function readBrandKit(): BrandKit {
  if (typeof window === "undefined") return DEFAULT_BRAND_KIT;
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null");
    if (!isBrandKit(stored)) return DEFAULT_BRAND_KIT;
    return normalizeBrandKit(stored);
  } catch {
    return DEFAULT_BRAND_KIT;
  }
}

export function saveBrandKit(kit: BrandKit): BrandKit {
  const normalized = normalizeBrandKit(kit);
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(normalized));
  window.dispatchEvent(new CustomEvent(BRAND_KIT_CHANGED, { detail: normalized }));
  return normalized;
}

function isBrandKit(value: unknown): value is BrandKit {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<BrandKit>;
  return isColor(candidate.primary)
    && (candidate.secondary === null || isColor(candidate.secondary))
    && Array.isArray(candidate.supporting)
    && candidate.supporting.every(isColor);
}

function isColor(value: unknown): boolean {
  if (typeof value !== "object" || value === null) return false;
  const color = value as Record<string, unknown>;
  return typeof color.id === "string"
    && typeof color.name === "string"
    && typeof color.value === "string";
}
