// Reading the funnel the server sends (S2.10b) without changing what it says.
//
// Nothing here re-derives a figure. The counts are the server's, the money is
// the server's, and the two properties an owner could be misled by are carried
// rather than smoothed away:
//
//   * A **view** and a **start** are reported by the visitor's browser; a
//     **submit** is counted inside the write that stored the enquiry, and
//     everything after it is a row somebody created. A rate that crosses that
//     line is therefore a floor, never a measurement — so each stage says
//     where its number comes from and the screen says it in words.
//   * The stages are counted independently, so `starts > views` is possible
//     (an anchor arrival, a lost beacon) and the bars are drawn against the
//     largest stage rather than against the first one. A funnel drawn as
//     percentages of its top would show 130 % here and be a lie about the
//     data rather than a picture of it.
import { getLocale, strings } from "../i18n";
import type { SiteAttributionSource } from "./types";

/** Where one stage's number came from: the page script in the visitor's
 *  browser, or a write on the server that could not be faked by a script. */
export type FunnelEvidence = "browser" | "server";

/** One step of the funnel, ready to draw. */
export interface FunnelStage {
  key: string;
  label: string;
  count: number;
  evidence: FunnelEvidence;
  /** Share of the largest stage, 0–1 — the bar's length, nothing more. */
  share: number;
}

/** The site totals, or one conversion point's, as the six steps a person
 *  reads left to right. `invoices` is `null` when alo Billing is switched off
 *  for this reader, and that step is then dropped rather than drawn as zero. */
export function funnelStages(
  totals: Pick<
    SiteAttributionSource,
    "views" | "starts" | "submits" | "leads" | "dealsWon" | "invoices"
  >,
): FunnelStage[] {
  const steps: Array<{ key: string; label: string; count: number; evidence: FunnelEvidence }> = [
    { key: "views", label: strings.sitesFunnelStageViews, count: totals.views, evidence: "browser" },
    {
      key: "starts",
      label: strings.sitesFunnelStageStarts,
      count: totals.starts,
      evidence: "browser",
    },
    {
      key: "submits",
      label: strings.sitesFunnelStageSubmits,
      count: totals.submits,
      evidence: "server",
    },
    { key: "leads", label: strings.sitesFunnelStageLeads, count: totals.leads, evidence: "server" },
    { key: "won", label: strings.sitesFunnelStageWon, count: totals.dealsWon, evidence: "server" },
    ...(totals.invoices === null
      ? []
      : [
          {
            key: "invoices",
            label: strings.sitesFunnelStageInvoices,
            count: totals.invoices,
            evidence: "server" as const,
          },
        ]),
  ];
  const largest = steps.reduce((max, step) => Math.max(max, step.count), 0);
  return steps.map((step) => ({
    ...step,
    share: largest === 0 ? 0 : step.count / largest,
  }));
}

/** One stored money figure, in its own currency and the reader's language.
 *  Never converted and never summed across currencies. */
export function funnelMoney(cents: number, currency: string): string {
  try {
    return new Intl.NumberFormat(getLocale(), {
      style: "currency",
      currency,
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(cents / 100);
  } catch {
    // The server validates the shape of a currency code, not the ISO list; an
    // unknown one must not blank a figure.
    return `${(cents / 100).toFixed(2)} ${currency}`;
  }
}

/** What to call a conversion point. The assistant conversation (`chat`) is a
 *  site-level source with no record of its own, so it is named here; a form
 *  deleted since the enquiries came in keeps its counts — deleting a form
 *  must not rewrite last month — so it is named as gone rather than shown
 *  with an empty label. */
export function sourceLabel(source: Pick<SiteAttributionSource, "kind" | "name">): string {
  if (source.kind === "chat") {
    return strings.sitesFunnelChatSource;
  }
  return source.name === null || source.name === "" ? strings.sitesFunnelDeletedSource : source.name;
}

/** Whether a conversion point has anything to say yet. Used only to sort the
 *  quiet ones last: a form nobody has reached is still listed, because "no
 *  one has reached this form" is a finding an owner should see. */
export function sourceIsQuiet(source: SiteAttributionSource): boolean {
  return (
    source.views === 0 &&
    source.starts === 0 &&
    source.submits === 0 &&
    source.leads === 0
  );
}
