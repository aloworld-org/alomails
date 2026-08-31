import { beforeEach, describe, expect, test, vi } from "vitest";

import { BRAND_KIT_CHANGED, readBrandKit, saveBrandKit } from "./repository";
import { DEFAULT_BRAND_KIT } from "./model";

beforeEach(() => {
  const values = new Map<string, string>();
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      get length() { return values.size; },
      clear: () => values.clear(),
      getItem: (key: string) => values.get(key) ?? null,
      key: (index: number) => [...values.keys()][index] ?? null,
      removeItem: (key: string) => values.delete(key),
      setItem: (key: string, value: string) => values.set(key, value),
    } satisfies Storage,
  });
});

describe("brand kit repository", () => {
  test("migrates the original color-only browser record", () => {
    window.localStorage.setItem("alo-workspace-brand-kit-v1", JSON.stringify({
      primary: { id: "primary", name: "Coral", value: "#E76F51" },
      secondary: null,
      supporting: [],
    }));
    expect(readBrandKit()).toEqual(expect.objectContaining({ foundation: DEFAULT_BRAND_KIT.foundation, typography: DEFAULT_BRAND_KIT.typography, secondary: null }));
  });

  test("publishes the normalized kit after saving", () => {
    const listener = vi.fn();
    window.addEventListener(BRAND_KIT_CHANGED, listener);
    saveBrandKit({ ...DEFAULT_BRAND_KIT, foundation: { ...DEFAULT_BRAND_KIT.foundation, name: "Northstar" } });
    expect(readBrandKit().foundation.name).toBe("Northstar");
    expect(listener).toHaveBeenCalledOnce();
    window.removeEventListener(BRAND_KIT_CHANGED, listener);
  });
});
