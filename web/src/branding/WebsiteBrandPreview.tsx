import { strings } from "../i18n";
import { BrandMark } from "./BrandMark";
import { BrandedButton } from "./BrandedButton";
import { presentedBrandName } from "./brandPresentation";
import type { BrandKit } from "./model";

export function WebsiteBrandPreview({ kit }: { kit: BrandKit }) {
  return (
    <div className="mx-auto overflow-hidden rounded-xl border border-black/10 bg-white shadow-xl shadow-black/10" style={{ fontFamily: "var(--brand-body)" }}>
      <div className="flex h-8 items-center gap-1.5 border-b border-slate-200 bg-slate-50 px-4" aria-hidden="true"><i className="size-2 rounded-full bg-slate-300" /><i className="size-2 rounded-full bg-slate-300" /><i className="size-2 rounded-full bg-slate-300" /><span className="mx-auto h-2 w-36 rounded-full bg-slate-200" /></div>
      <nav className="flex min-h-16 items-center gap-4 px-6 text-slate-800">
        <BrandMark kit={kit} /><strong className="mr-auto text-sm" style={{ fontFamily: "var(--brand-heading)" }}>{presentedBrandName(kit)}</strong>
        <span className="hidden text-xs font-medium text-slate-500 sm:inline">{strings.brandingPreviewWork}</span><span className="hidden text-xs font-medium text-slate-500 sm:inline">{strings.brandingPreviewAbout}</span>
        <BrandedButton>{strings.brandingPreviewStartProject}</BrandedButton>
      </nav>
      <div className="grid min-h-80 md:grid-cols-[1.15fr_0.85fr]">
        <div className="flex flex-col justify-center px-8 py-12 lg:px-12">
          <small className="font-semibold uppercase tracking-[0.12em] text-[var(--brand-primary)]">{strings.brandingPreviewWebsiteEyebrow}</small>
          <h3 className="mb-3 mt-3 max-w-xl text-3xl font-semibold leading-tight tracking-tight text-slate-900 lg:text-4xl" style={{ fontFamily: "var(--brand-heading)" }}>{strings.brandingPreviewWebsiteHeading}</h3>
          <p className="mb-6 max-w-lg text-sm leading-6 text-slate-500">{strings.brandingPreviewWebsiteBody}</p>
          <div className="flex flex-wrap gap-2"><BrandedButton>{strings.brandingPreviewExploreWork}</BrandedButton><BrandedButton secondary>{strings.brandingPreviewOurApproach}</BrandedButton></div>
        </div>
        <div className="relative hidden overflow-hidden bg-[var(--brand-secondary)] md:block"><span className="absolute -right-10 -top-12 size-56 rounded-full border-[2.5rem] border-white/10" /><span className="absolute -bottom-16 -left-10 size-48 rounded-full bg-[var(--brand-primary)] opacity-90" /></div>
      </div>
      <div className="grid grid-cols-3 border-t border-slate-200 bg-slate-50 px-5 py-3 text-center text-xs text-slate-500"><span><b className="block text-base text-[var(--brand-secondary)]">42</b>{strings.brandingPreviewLaunches}</span><span><b className="block text-base text-[var(--brand-secondary)]">11</b>{strings.brandingPreviewCountries}</span><span><b className="block text-base text-[var(--brand-secondary)]">96%</b>{strings.brandingPreviewReferred}</span></div>
    </div>
  );
}
