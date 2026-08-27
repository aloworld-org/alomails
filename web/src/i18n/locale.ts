// The runtime locale layer: which language is active, how it is
// detected and persisted, and the merged catalog the `strings` proxy
// reads. Adding a language is a catalog file plus one row in `LOCALES`;
// nothing else changes, because components read `strings.*` and never
// import a locale directly.
import { useSyncExternalStore } from "react";

import { de } from "./de";
import { en, type Catalog } from "./en";
import { fr } from "./fr";
import { nl } from "./nl";

/** The languages alo ships. `en` is the source and always complete. */
export type Locale = "en" | "fr" | "nl" | "de";

/** Display metadata for the language switcher, in menu order. */
export const LOCALES: ReadonlyArray<{ code: Locale; label: string }> = [
  { code: "en", label: "English" },
  { code: "nl", label: "Nederlands" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
];

/** Partial catalogs per locale; missing keys fall back to English. */
const CATALOGS: Record<Locale, Partial<Catalog>> = {
  en: {},
  nl,
  fr,
  de,
};

const STORAGE_KEY = "alo.locale";

function isLocale(value: string | null): value is Locale {
  return value === "en" || value === "fr" || value === "nl" || value === "de";
}

/**
 * The initial locale: an explicit stored choice wins; otherwise the
 * browser's preferred language if we ship it; otherwise English. Pure
 * and side-effect-free so it is safe at module load and in tests.
 */
function detectLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isLocale(stored)) return stored;
  } catch {
    // localStorage can throw in privacy modes — fall through to detection.
  }
  const nav = typeof navigator !== "undefined" ? navigator.language : "";
  const prefix = nav.slice(0, 2).toLowerCase();
  if (prefix === "nl") return "nl";
  if (prefix === "fr") return "fr";
  if (prefix === "de") return "de";
  return "en";
}

/** The full catalog for `locale`: English overlaid with its overrides. */
export function buildCatalog(locale: Locale): Catalog {
  return { ...en, ...CATALOGS[locale] };
}

// --- Active state + a minimal external store so React re-renders on a
// --- locale change (useSyncExternalStore is the tearing-free hook for
// --- an out-of-React mutable value).

let activeLocale: Locale = detectLocale();
let activeCatalog: Catalog = buildCatalog(activeLocale);
const listeners = new Set<() => void>();

/** The catalog the `strings` proxy currently reads. */
export function currentCatalog(): Catalog {
  return activeCatalog;
}

/** The active locale code. */
export function getLocale(): Locale {
  return activeLocale;
}

/**
 * Switches the active language: rebuilds the catalog, persists the
 * choice, updates `<html lang>`, and notifies subscribers so the app
 * re-renders in the new language. A no-op when already active.
 */
export function setLocale(locale: Locale): void {
  if (locale === activeLocale) return;
  activeLocale = locale;
  activeCatalog = buildCatalog(locale);
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // Persistence is best-effort; the in-memory switch still applies.
  }
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
  for (const notify of listeners) notify();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Subscribes a component to the active locale. Returning the code (not
 * the catalog) keeps the snapshot referentially stable between changes,
 * so `useSyncExternalStore` never loops.
 */
export function useLocale(): Locale {
  return useSyncExternalStore(subscribe, getLocale, getLocale);
}

// Reflect the initial locale on <html lang> for a11y / correct
// hyphenation from first paint.
if (typeof document !== "undefined") {
  document.documentElement.lang = activeLocale;
}
