// The widget accent's palette-role pairs and their measured contrast — the
// number the appearance screen shows beside the colour choice (S3.02g), so
// accessibility is read off the screen rather than discovered after
// publishing. The server is the authority (no storable accent can fail AA —
// proven at build time against every shipped preset); this mirror exists to
// SHOW the guarantee with the real ratio, not to enforce it.
import type { SiteChatAccent, ThemePreset } from "./types";

/** The palette roles one accent paints the widget with, as `(fill, label)` —
 *  the same pairs the store's `ChatWidgetAccent::role_pair` names. */
export function accentRoles(
  accent: SiteChatAccent,
  palette: ThemePreset["palette"],
): { fill: string; label: string } {
  switch (accent) {
    case "primary":
      return { fill: palette.primary, label: palette.onPrimary };
    case "text":
      return { fill: palette.text, label: palette.background };
    case "surface":
      return { fill: palette.surface, label: palette.text };
  }
}

/** WCAG relative luminance of one `#rrggbb` channel value. */
function channel(hex: string, at: number): number | null {
  const raw = Number.parseInt(hex.slice(at, at + 2), 16);
  if (Number.isNaN(raw)) return null;
  const scaled = raw / 255;
  return scaled <= 0.03928 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance of a `#rrggbb` colour; `null` when malformed. */
function luminance(color: string): number | null {
  if (!/^#[0-9a-fA-F]{6}$/.test(color)) return null;
  const r = channel(color, 1);
  const g = channel(color, 3);
  const b = channel(color, 5);
  if (r === null || g === null || b === null) return null;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG contrast ratio between two `#rrggbb` colours, or `null` when
 *  either is malformed (the screen then states the server-side guarantee
 *  instead of a number). */
export function contrastRatio(a: string, b: string): number | null {
  const la = luminance(a);
  const lb = luminance(b);
  if (la === null || lb === null) return null;
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** The measured contrast of one accent choice on one shipped palette,
 *  rounded to one decimal for display; `null` when it cannot be computed. */
export function accentContrast(
  accent: SiteChatAccent,
  palette: ThemePreset["palette"],
): number | null {
  const { fill, label } = accentRoles(accent, palette);
  const ratio = contrastRatio(fill, label);
  return ratio === null ? null : Math.round(ratio * 10) / 10;
}
