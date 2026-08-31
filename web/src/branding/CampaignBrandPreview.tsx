import { strings } from "../i18n";
import { BrandMark } from "./BrandMark";
import { BrandedButton } from "./BrandedButton";
import { presentedBrandName } from "./brandPresentation";
import type { BrandKit } from "./model";

export function CampaignBrandPreview({ kit }: { kit: BrandKit }) {
  return (
    <div className="mx-auto max-w-xl overflow-hidden rounded-xl bg-white text-slate-800 shadow-lg" style={{ fontFamily: "var(--brand-body)" }}>
      <div className="flex items-center gap-2 px-6 py-4"><BrandMark kit={kit} /><strong style={{ fontFamily: "var(--brand-heading)" }}>{presentedBrandName(kit)}</strong></div>
      <div className="grid min-h-36 place-items-center bg-[linear-gradient(135deg,var(--brand-secondary),var(--brand-primary))]"><span className="rounded-full border border-white/40 bg-white/10 px-4 py-2 text-xs font-semibold tracking-[0.12em] text-white">{strings.brandingPreviewCampaignBadge}</span></div>
      <div className="px-8 py-7 text-center"><small className="font-semibold uppercase tracking-[0.08em] text-[var(--brand-primary)]">{strings.brandingPreviewCampaignEyebrow}</small><h3 className="mb-3 mt-2 text-2xl font-semibold text-slate-900" style={{ fontFamily: "var(--brand-heading)" }}>{strings.brandingPreviewCampaignHeading}</h3><p className="mx-auto mb-5 max-w-md text-sm leading-6 text-slate-500">{strings.brandingPreviewCampaignBody}</p><BrandedButton>{strings.brandingPreviewCampaignAction}</BrandedButton></div>
      <footer className="bg-slate-50 px-6 py-3 text-center text-xs text-slate-400">{presentedBrandName(kit)} · {strings.brandingPreviewCampaignLocation}</footer>
    </div>
  );
}
