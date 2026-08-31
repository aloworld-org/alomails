import { strings } from "../i18n";
import { BrandMark } from "./BrandMark";
import { presentedBrandName, presentedTagline } from "./brandPresentation";
import type { BrandKit } from "./model";

export function DocumentBrandPreview({ kit }: { kit: BrandKit }) {
  const body = kit.foundation.purpose.trim() || strings.brandingPreviewDocumentBody;
  const section = kit.foundation.positioning.trim() || strings.brandingPreviewDocumentSectionBody;
  return (
    <article className="mx-auto min-h-[32rem] max-w-3xl bg-white p-8 text-slate-800 shadow-lg sm:p-12" style={{ fontFamily: "var(--brand-body)" }}>
      <header className="flex items-start justify-between border-b border-slate-200 pb-7"><div className="flex items-center gap-3"><BrandMark kit={kit} large /><div><strong className="block text-lg" style={{ fontFamily: "var(--brand-heading)" }}>{presentedBrandName(kit)}</strong><span className="text-xs text-slate-500">{presentedTagline(kit)}</span></div></div><span className="text-xs font-bold tracking-[0.15em] text-[var(--brand-primary)]">{strings.brandingPreviewDocumentType}</span></header>
      <div className="py-12"><span className="mb-5 block h-1 w-16 rounded-full bg-[var(--brand-primary)]" /><h3 className="m-0 max-w-2xl text-4xl font-semibold leading-tight tracking-tight text-slate-900" style={{ fontFamily: "var(--brand-heading)" }}>{strings.brandingPreviewDocumentHeading}</h3><p className="mb-0 mt-5 max-w-2xl text-base leading-7 text-slate-500">{body}</p></div>
      <section className="border-l-4 border-[var(--brand-secondary)] bg-slate-50 p-6"><h4 className="m-0 text-sm font-semibold uppercase tracking-wider text-[var(--brand-secondary)]">{strings.brandingPreviewDocumentSection}</h4><p className="mb-0 mt-3 text-sm leading-6 text-slate-600">{section}</p></section>
    </article>
  );
}
