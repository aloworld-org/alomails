import { afterEach, describe, expect, it, vi } from "vitest";

import type { BillingApi } from "../api";
import {
  loadQuoteStudioDesign,
  saveQuoteStudioDesign,
} from "./quoteStudioPersistence";

function fakeApi(stored: unknown) {
  const saved: unknown[] = [];
  const calls = {
    quoteDesign: vi.fn(async () => ({ design: stored, updatedAt: null })),
    saveQuoteDesign: vi.fn(async (_id: string, design: unknown) => {
      saved.push(design);
    }),
  };
  return { api: calls as unknown as BillingApi, saved, calls };
}

function noBrowserStorage(values = new Map<string, string>()) {
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
  return values;
}

describe("quote studio persistence", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("loads the design the server holds, normalized", async () => {
    const { api } = fakeApi({ blocks: [], headerStyle: "minimal" });
    const design = await loadQuoteStudioDesign(api, "q1");
    expect(design.headerStyle).toBe("minimal");
    // The price table is always present, whatever was saved.
    expect(design.blocks.some((block) => block.kind === "pricing")).toBe(true);
  });

  it("moves a design this browser saved before to the server, once", async () => {
    const values = noBrowserStorage(
      new Map([
        ["alo:quote-design:q2", JSON.stringify({ blocks: [], headerStyle: "minimal" })],
      ]),
    );
    const { api, saved, calls } = fakeApi(null);
    const design = await loadQuoteStudioDesign(api, "q2");
    expect(design.headerStyle).toBe("minimal");
    expect(saved).toHaveLength(1);
    expect(calls.saveQuoteDesign).toHaveBeenCalledWith("q2", design);
    // The browser copy is forgotten once the server has it.
    expect(values.has("alo:quote-design:q2")).toBe(false);
  });

  it("starts from the blank design when nobody designed the quote yet", async () => {
    noBrowserStorage();
    const { api, saved } = fakeApi(null);
    const design = await loadQuoteStudioDesign(api, "q3");
    expect(design.blocks).toEqual([{ id: "pricing-table", kind: "pricing" }]);
    expect(saved).toHaveLength(0);
  });

  it("still loads when the server cannot be reached, and never rejects", async () => {
    noBrowserStorage();
    const api = {
      quoteDesign: vi.fn(async () => {
        throw new Error("offline");
      }),
      saveQuoteDesign: vi.fn(),
    } as unknown as BillingApi;
    const design = await loadQuoteStudioDesign(api, "q5");
    expect(design.blocks).toEqual([{ id: "pricing-table", kind: "pricing" }]);
  });

  it("saves through the server and surfaces its refusal", async () => {
    const api = {
      saveQuoteDesign: vi.fn(async () => {
        throw new Error("409");
      }),
    } as unknown as BillingApi;
    await expect(
      saveQuoteStudioDesign(api, "q4", { blocks: [] } as never),
    ).rejects.toThrow("409");
  });
});
