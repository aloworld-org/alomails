import type { Period } from "./period";
import type { VatReport } from "./types";

export function vatReportFileName(period: Period): string {
  return `vat-${period.from}-to-${period.to}.csv`;
}

export function vatReportRestatesAnything(report: VatReport): boolean {
  return (
    report.base.unconvertedCount > 0 ||
    report.currencies.some((group) => group.currency !== report.base.currency)
  );
}
