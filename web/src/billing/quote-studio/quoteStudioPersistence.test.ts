import { afterEach, describe, expect, it, vi } from "vitest";

import { loadQuoteStudioDesign } from "./quoteStudioPersistence";

describe("quote studio persistence", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("falls back to a normalized legacy design", async () => {
    const values = new Map([
      ["quote-test", JSON.stringify({ blocks: [], headerStyle: "minimal" })],
    ]);
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      removeItem: (key: string) => values.delete(key),
      setItem: (key: string, value: string) => values.set(key, value),
    });
    vi.stubGlobal("indexedDB", {
      open: () => {
        throw new Error("unavailable");
      },
    });
    const design = await loadQuoteStudioDesign("quote-test");
    expect(design.headerStyle).toBe("minimal");
    expect(design.blocks.some((block) => block.kind === "pricing")).toBe(true);
  });
});
