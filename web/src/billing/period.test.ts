// The reporting periods a form is prefilled with. Pure calendar arithmetic, so
// the assertions are hand-written dates rather than a second implementation of
// the same maths — and every case is checked from a day near a boundary, which
// is where a quarter is actually got wrong.
import { describe, expect, test } from "vitest";

import { previousQuarterOf, quarterOf } from "./period";

/** A day, stated in UTC so the test says what it means everywhere. */
function on(iso: string): Date {
  return new Date(`${iso}T12:00:00Z`);
}

describe("the quarter a day falls in", () => {
  test("is the calendar quarter, from its first day to its last", () => {
    expect(quarterOf(on("2026-02-14"))).toEqual({ from: "2026-01-01", to: "2026-03-31" });
    expect(quarterOf(on("2026-05-01"))).toEqual({ from: "2026-04-01", to: "2026-06-30" });
    expect(quarterOf(on("2026-08-07"))).toEqual({ from: "2026-07-01", to: "2026-09-30" });
    expect(quarterOf(on("2026-11-30"))).toEqual({ from: "2026-10-01", to: "2026-12-31" });
  });

  test("includes its own first and last day", () => {
    const q3 = { from: "2026-07-01", to: "2026-09-30" };
    expect(quarterOf(on("2026-07-01"))).toEqual(q3);
    expect(quarterOf(on("2026-09-30"))).toEqual(q3);
    // One day either side is a different quarter — the boundaries are exact.
    expect(quarterOf(on("2026-06-30")).to).toBe("2026-06-30");
    expect(quarterOf(on("2026-10-01")).from).toBe("2026-10-01");
  });

  test("ends on the real last day of February, leap year or not", () => {
    expect(quarterOf(on("2024-01-15")).to).toBe("2024-03-31");
    expect(previousQuarterOf(on("2024-04-15"))).toEqual({ from: "2024-01-01", to: "2024-03-31" });
  });
});

describe("the quarter before", () => {
  test("is the one a return is actually filed for", () => {
    expect(previousQuarterOf(on("2026-08-07"))).toEqual({ from: "2026-04-01", to: "2026-06-30" });
    expect(previousQuarterOf(on("2026-11-30"))).toEqual({ from: "2026-07-01", to: "2026-09-30" });
  });

  test("crosses the year boundary backwards", () => {
    expect(previousQuarterOf(on("2026-01-01"))).toEqual({ from: "2025-10-01", to: "2025-12-31" });
    expect(previousQuarterOf(on("2026-03-31"))).toEqual({ from: "2025-10-01", to: "2025-12-31" });
  });
});
