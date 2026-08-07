// The drawable model is where a server answer becomes a chart's shape, so this
// is where the promises about that shape are proven: the server's order is
// kept, a bucket only one group has is still drawn, a bucket a group has no
// answer for is a gap rather than a zero, labels are translated or passed
// through as the tenant typed them, and every figure's text is the server's
// integer formatted — never a number worked out here.
import { describe, expect, test } from "vitest";

import { align, chartModel } from "./model";
import type { Series } from "../types";

/** Two currencies over three months, with a hole in each. */
const TWO_CURRENCIES: Series = {
  unit: { kind: "money" },
  series: [
    {
      key: "EUR",
      label: { kind: "raw", text: "EUR" },
      points: [
        { bucket: "2026-06", value: 1_000_000 },
        { bucket: "2026-07", value: 0 },
      ],
    },
    {
      key: "USD",
      label: { kind: "raw", text: "USD" },
      points: [{ bucket: "2026-08", value: 250_000 }],
    },
  ],
  notes: [],
  truncated: false,
};

/** Ageing, as the receivables dataset answers it: catalog labels, one group. */
const AGEING: Series = {
  unit: { kind: "money", currency: "EUR" },
  series: [
    {
      key: "EUR",
      label: { kind: "raw", text: "EUR" },
      points: [
        { bucket: "age.not_due", label: { kind: "catalog", id: "age.not_due" }, value: 400_000 },
        { bucket: "age.31_60", label: { kind: "catalog", id: "age.31_60" }, value: 120_000 },
        { bucket: "other", label: { kind: "catalog", id: "bucket.other" }, value: 5_000 },
      ],
    },
  ],
  notes: [],
  truncated: true,
};

describe("aligning an answer", () => {
  test("keeps the server's order and draws every bucket some group has", () => {
    const model = align(TWO_CURRENCIES);
    expect(model.categories).toEqual(["Jun 2026", "Jul 2026", "Aug 2026"]);
    expect(model.multi).toBe(true);
  });

  test("a bucket a group had no answer for is a gap, not a zero", () => {
    const [euro, dollar] = align(TWO_CURRENCIES).series;
    // The euro group measured zero in July — a real figure — and was not asked
    // about August at all. The two must not look the same.
    expect(euro?.values.map((v) => v.value)).toEqual([1_000_000, 0, null]);
    expect(euro?.values[1]?.text).toBe("€0.00");
    expect(euro?.values[2]?.text).toBe("");
    expect(dollar?.values.map((v) => v.value)).toEqual([null, null, 250_000]);
  });

  test("each group's money is read in its own currency when the answer has no single one", () => {
    const [euro, dollar] = align(TWO_CURRENCIES).series;
    expect(euro?.values[0]?.text).toBe("€10,000.00");
    expect(dollar?.values[2]?.text).toBe("$2,500.00");
  });

  test("catalog buckets are translated, and the tenant's own words are not", () => {
    const model = align(AGEING);
    expect(model.categories).toEqual(["Not due", "31–60 days", "Other"]);
    expect(model.series[0]?.name).toBe("EUR");
    expect(model.multi).toBe(false);
  });
});

describe("a chart model", () => {
  test("formats its axis in the unit the answer declared", () => {
    const model = chartModel(AGEING, "bar");
    expect(model.kind).toBe("bar");
    // The axis drops the currency symbol the tile's figures still carry.
    expect(model.axisLabel(400_000)).toBe("4,000.00");
  });

  test("a pie is narrowed to one group, because shares are shares of one whole", () => {
    const model = chartModel(TWO_CURRENCIES, "pie", "USD");
    expect(model.multi).toBe(false);
    expect(model.categories).toEqual(["Aug 2026"]);
    expect(model.series.map((s) => s.key)).toEqual(["USD"]);
  });

  test("a count is a plain number and a ratio is a percentage", () => {
    const won: Series = {
      unit: { kind: "percent_bp" },
      series: [
        {
          key: "all",
          label: { kind: "catalog", id: "series.all" },
          points: [{ bucket: "total", value: 4_250 }],
        },
      ],
      notes: [],
      truncated: false,
    };
    expect(chartModel(won, "bar").series[0]?.values[0]?.text).toBe("42.5%");

    const deals: Series = {
      unit: { kind: "count" },
      series: [
        {
          key: "all",
          label: { kind: "catalog", id: "series.all" },
          points: [{ bucket: "total", value: 1_234 }],
        },
      ],
      notes: [],
      truncated: false,
    };
    const model = chartModel(deals, "bar");
    expect(model.series[0]?.values[0]?.text).toBe("1,234");
    expect(model.series[0]?.name).toBe("All");
  });
});
