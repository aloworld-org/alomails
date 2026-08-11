// Ranking what somebody reaches for. The interesting property is not that
// frequency counts — it is that it stops counting once somebody has moved on.
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { mostUsedApps, recordAppVisit } from "./appUsage";

/** A real store: this environment's localStorage has no `clear`, and a stub
 *  that silently drops writes would let every assertion below pass for the
 *  wrong reason. */
function freshStorage() {
  const map = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: (i: number) => [...map.keys()][i] ?? null,
    get length() {
      return map.size;
    },
  });
}

beforeEach(() => {
  freshStorage();
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("what somebody reaches for", () => {
  test("more visits rank higher", () => {
    recordAppVisit("mail");
    recordAppVisit("mail");
    recordAppVisit("drive");
    expect(mostUsedApps(2)).toEqual(["mail", "drive"]);
  });

  test("a month-end in Billing does not follow you into March", () => {
    // Heavy use, ten weeks ago.
    vi.setSystemTime(new Date("2026-01-01"));
    for (let i = 0; i < 10; i += 1) recordAppVisit("billing");

    // Light use, this week.
    vi.setSystemTime(new Date("2026-03-15"));
    recordAppVisit("chat");
    recordAppVisit("chat");

    // This is the whole point: frequency alone would keep Billing first
    // for ever, and a shortcut to what somebody has stopped doing is worse
    // than no shortcut.
    expect(mostUsedApps(1)).toEqual(["chat"]);
  });

  test("something opened once and never again eventually drops out", () => {
    vi.setSystemTime(new Date("2025-01-01"));
    recordAppVisit("insights");
    vi.setSystemTime(new Date("2026-06-01"));
    expect(mostUsedApps(6)).not.toContain("insights");
  });

  test("home is not a favourite, since it is already the first thing", () => {
    recordAppVisit("home");
    expect(mostUsedApps(6)).toEqual([]);
  });

  test("a corrupt preference never stops navigation rendering", () => {
    localStorage.setItem("alo-app-usage", "{ not json");
    expect(mostUsedApps(6)).toEqual([]);
    // And it recovers rather than staying broken.
    recordAppVisit("mail");
    expect(mostUsedApps(6)).toEqual(["mail"]);
  });
});
