import { describe, expect, it } from "vitest";
import {
  vatReportFileName,
  vatReportRestatesAnything,
} from "./vatReportPresentation";
import type { VatReport } from "./types";

function report(currencies: string[], unconvertedCount = 0): VatReport {
  return {
    from: "2026-07-01",
    to: "2026-09-30",
    currencies: currencies.map((currency) => ({
      currency,
      invoiceCount: 0,
      creditNoteCount: 0,
      netCents: 0,
      vatCents: 0,
      grossCents: 0,
      byRate: [],
      baseNetCents: 0,
      baseVatCents: 0,
      baseGrossCents: 0,
      unconvertedCount: 0,
    })),
    base: {
      currency: "EUR",
      byRate: [],
      netCents: 0,
      vatCents: 0,
      grossCents: 0,
      unconvertedCount,
    },
  };
}

describe("VAT report presentation", () => {
  it("builds a stable period filename", () => {
    expect(vatReportFileName({ from: "2026-07-01", to: "2026-09-30" })).toBe(
      "vat-2026-07-01-to-2026-09-30.csv",
    );
  });

  it("only repeats the base table when another currency or missing rate matters", () => {
    expect(vatReportRestatesAnything(report(["EUR"]))).toBe(false);
    expect(vatReportRestatesAnything(report(["EUR", "USD"]))).toBe(true);
    expect(vatReportRestatesAnything(report(["EUR"], 1))).toBe(true);
  });
});
