import { afterEach, describe, expect, test, vi } from "vitest";

import { en } from "./en";
import { fr } from "./fr";
import { buildCatalog, getLocale, setLocale } from "./locale";
import { strings } from "./strings";

afterEach(() => {
  setLocale("en");
  vi.restoreAllMocks();
});

describe("catalog fallback", () => {
  test("a built catalog has every English key (no blanks possible)", () => {
    const catalog = buildCatalog("fr");
    for (const key of Object.keys(en)) {
      expect(catalog).toHaveProperty(key);
      expect((catalog as Record<string, unknown>)[key]).toBeDefined();
    }
  });

  test("French keys override English, untranslated keys keep English", () => {
    const catalog = buildCatalog("fr");
    // A key present in fr is translated...
    expect(catalog.moduleMail).toBe("Courrier");
    // ...and every fr key actually matches an English key (no typos /
    // stale keys that would silently never render).
    for (const key of Object.keys(fr)) {
      expect(en).toHaveProperty(key);
    }
  });
});

describe("runtime switching", () => {
  test("strings proxy reflects the active locale live", () => {
    expect(getLocale()).toBe("en");
    expect(strings.compose).toBe("Compose");
    setLocale("fr");
    expect(getLocale()).toBe("fr");
    expect(strings.compose).toBe("Écrire");
    // Interpolation functions switch too.
    expect(strings.selectedCount(2)).toBe("2 sélectionnés");
    setLocale("en");
    expect(strings.compose).toBe("Compose");
  });

  test("setLocale persists the choice to localStorage", () => {
    // jsdom's localStorage is a non-functional stub here; install a
    // real in-memory one to observe the persistence contract.
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k),
      clear: () => store.clear(),
    });
    setLocale("fr");
    expect(store.get("alo.locale")).toBe("fr");
    setLocale("en");
    expect(store.get("alo.locale")).toBe("en");
    vi.unstubAllGlobals();
  });
});
