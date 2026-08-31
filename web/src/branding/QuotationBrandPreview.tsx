import { strings } from "../i18n";
import { BrandMark } from "./BrandMark";
import { presentedBrandName } from "./brandPresentation";
import type { BrandKit } from "./model";

export function QuotationBrandPreview({ kit }: { kit: BrandKit }) {
  return (
    <div className="mx-auto max-w-3xl rounded-sm bg-white p-7 text-slate-800 shadow-lg sm:p-9" style={{ fontFamily: "var(--brand-body)" }}>
      <header className="flex items-start justify-between border-b-2 border-[var(--brand-primary)] pb-5"><div className="flex items-center gap-2"><BrandMark kit={kit} /><strong style={{ fontFamily: "var(--brand-heading)" }}>{presentedBrandName(kit)}</strong></div><span className="text-sm font-semibold tracking-[0.12em] text-[var(--brand-primary)]">{strings.brandingPreviewQuoteLabel}</span></header>
      <div className="grid grid-cols-2 gap-5 py-6 text-sm"><div><small className="block text-slate-400">{strings.brandingPreviewPreparedFor}</small><b>{strings.brandingSampleClient}</b></div><div className="text-right"><small className="block text-slate-400">{strings.brandingPreviewQuote}</small><b>QUO-2026-0042</b></div></div>
      <div className="divide-y divide-slate-200 border-y border-slate-200 text-sm"><span className="flex justify-between py-3">{strings.brandingPreviewBrandStrategy}<b>€2,400</b></span><span className="flex justify-between py-3">{strings.brandingPreviewVisualIdentity}<b>€4,800</b></span><span className="flex justify-between py-3">{strings.brandingPreviewLaunchToolkit}<b>€1,650</b></span></div>
      <div className="ml-auto mt-5 flex max-w-xs items-center justify-between rounded-xl bg-[var(--brand-secondary)] px-5 py-4 text-[var(--brand-secondary-ink)]"><span>{strings.brandingPreviewTotal}</span><strong className="text-xl">€8,850</strong></div>
      <footer className="mt-8 border-t border-slate-200 pt-4 text-xs text-slate-400">{strings.brandingPreviewQuoteFooter}</footer>
    </div>
  );
}
