import { Printer } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { brandPresentationVariables, presentedBrandName, presentedTagline } from "./brandPresentation";
import { brandFontStack } from "./brandTypography";
import { primaryBrandLogo, type BrandFont, type BrandKit } from "./model";

function fontName(font: BrandFont): string {
  if (font === "arial") return strings.brandingFontArial;
  if (font === "georgia") return strings.brandingFontGeorgia;
  if (font === "garamond") return strings.brandingFontGaramond;
  return strings.brandingFontInter;
}

export function BrandGuidelinesView({ kit }: { kit: BrandKit }) {
  const missing = strings.brandingGuidelineMissing;
  const logo = primaryBrandLogo(kit);
  const colors = [kit.primary, ...(kit.secondary === null ? [] : [kit.secondary]), ...kit.supporting];

  return (
    <section aria-labelledby="brand-guidelines-title" style={brandPresentationVariables(kit)}>
      <div className="mb-6 flex flex-wrap items-end justify-between gap-4 print:hidden">
        <div className="max-w-3xl"><h2 id="brand-guidelines-title" className="m-0 text-2xl font-semibold tracking-tight text-primary">{strings.brandingGuidelinesTitle}</h2><p className="mb-0 mt-2 text-sm leading-6 text-secondary">{strings.brandingGuidelinesSubtitle}</p></div>
        <Button variant="secondary" onClick={() => window.print()}><Printer size={16} aria-hidden="true" />{strings.brandingPrintGuidelines}</Button>
      </div>
      <article className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm print:border-0 print:shadow-none">
        <header className="bg-[var(--brand-secondary)] px-6 py-10 text-[var(--brand-secondary-ink)] sm:px-10 sm:py-14">
          <div className="flex items-center gap-5">
            {logo === null ? <span className="grid size-16 place-items-center rounded-xl bg-[var(--brand-primary)] text-2xl font-bold text-[var(--brand-primary-ink)]" aria-hidden="true">{presentedBrandName(kit).slice(0, 1).toUpperCase()}</span> : <span className="grid size-20 place-items-center rounded-xl bg-white p-2"><img className="max-h-full max-w-full object-contain" src={logo.dataUrl} alt={logo.name} /></span>}
            <div><h3 className="m-0 text-3xl font-semibold" style={{ fontFamily: brandFontStack(kit.typography.heading) }}>{presentedBrandName(kit)}</h3><p className="mb-0 mt-2 opacity-80" style={{ fontFamily: brandFontStack(kit.typography.body) }}>{presentedTagline(kit)}</p></div>
          </div>
        </header>
        <div className="grid lg:grid-cols-2">
          <section className="border-b border-subtle p-6 sm:p-8 lg:border-r"><h3 className="m-0 text-lg font-semibold text-primary">{strings.brandingGuidelineFoundation}</h3><dl className="mt-5 grid gap-4"><div><dt className="text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.brandingPurpose}</dt><dd className="m-0 mt-1 text-sm leading-6 text-secondary">{kit.foundation.purpose || missing}</dd></div><div><dt className="text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.brandingAudience}</dt><dd className="m-0 mt-1 text-sm leading-6 text-secondary">{kit.foundation.audience || missing}</dd></div><div><dt className="text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.brandingPositioning}</dt><dd className="m-0 mt-1 text-sm leading-6 text-secondary">{kit.foundation.positioning || missing}</dd></div><div><dt className="text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.brandingPersonality}</dt><dd className="m-0 mt-1 text-sm leading-6 text-secondary">{kit.foundation.personality || missing}</dd></div></dl></section>
          <section className="border-b border-subtle p-6 sm:p-8"><h3 className="m-0 text-lg font-semibold text-primary">{strings.brandingGuidelineLogo}</h3><div className="mt-5 grid min-h-32 place-items-center rounded-xl bg-raised p-5">{logo === null ? <p className="m-0 text-sm text-tertiary">{strings.brandingGuidelineLogoMissing}</p> : <img className="max-h-24 max-w-[70%] object-contain" src={logo.dataUrl} alt={logo.name} />}</div><p className="mb-0 mt-4 text-sm leading-6 text-secondary">{strings.brandingGuidelineLogoRule}</p></section>
          <section className="border-b border-subtle p-6 sm:p-8 lg:border-r"><h3 className="m-0 text-lg font-semibold text-primary">{strings.brandingGuidelineColors}</h3><div className="mt-5 grid gap-3 sm:grid-cols-2">{colors.map((color) => <div key={color.id} className="flex items-center gap-3 rounded-xl border border-subtle p-3"><span className="size-11 shrink-0 rounded-lg border border-black/5" style={{ backgroundColor: color.value }} /><span><strong className="block text-sm text-primary">{color.name}</strong><small className="text-xs text-tertiary">{color.value}</small></span></div>)}</div><p className="mb-0 mt-4 text-sm leading-6 text-secondary">{strings.brandingGuidelineColorRule}</p></section>
          <section className="border-b border-subtle p-6 sm:p-8"><h3 className="m-0 text-lg font-semibold text-primary">{strings.brandingGuidelineTypography}</h3><div className="mt-5 grid gap-4"><div className="rounded-xl bg-raised p-4"><span className="text-xs text-tertiary">{strings.brandingHeadingFont} · {fontName(kit.typography.heading)}</span><strong className="mt-2 block text-2xl text-primary" style={{ fontFamily: brandFontStack(kit.typography.heading) }}>{presentedBrandName(kit)}</strong></div><div className="rounded-xl bg-raised p-4"><span className="text-xs text-tertiary">{strings.brandingBodyFont} · {fontName(kit.typography.body)}</span><p className="mb-0 mt-2 text-sm leading-6 text-secondary" style={{ fontFamily: brandFontStack(kit.typography.body) }}>{strings.brandingGuidelineTypographyRule}</p></div></div></section>
          <section className="p-6 sm:p-8 lg:col-span-2"><h3 className="m-0 text-lg font-semibold text-primary">{strings.brandingGuidelineVoice}</h3><div className="mt-5 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(18rem,0.55fr)]"><blockquote className="m-0 rounded-xl border-l-4 border-[var(--brand-primary)] bg-raised p-5 text-base leading-7 text-primary">{kit.foundation.voice || missing}</blockquote><p className="m-0 rounded-xl bg-accent-soft p-5 text-sm leading-6 text-secondary">{strings.brandingGuidelineVoiceRule}</p></div></section>
        </div>
      </article>
    </section>
  );
}
