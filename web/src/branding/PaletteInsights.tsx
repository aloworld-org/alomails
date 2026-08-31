import { CheckCircle2, AlertCircle } from "lucide-react";
import type { ReactNode } from "react";

import { strings } from "../i18n";
import { contrastPasses, readableInk, toneScale } from "./colorTools";
import type { BrandKit } from "./model";

export function PaletteInsights({ kit }: { kit: BrandKit }) {
  const primaryInk = readableInk(kit.primary.value);
  const secondary = kit.secondary?.value ?? kit.primary.value;
  const secondaryInk = readableInk(secondary);

  return (
    <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
      <header className="border-b border-subtle px-5 py-4 lg:px-6">
        <h2 className="m-0 text-base font-semibold text-primary">{strings.brandingColorBalance}</h2>
        <p className="mb-0 mt-1 text-sm text-secondary">{strings.brandingColorBalanceHint}</p>
      </header>
      <div className="grid divide-y divide-subtle lg:grid-cols-3 lg:divide-x lg:divide-y-0">
      <Insight title={strings.brandingToneScale} meta={strings.brandingGenerated}>
        <div className="mt-4 grid h-12 grid-cols-6 overflow-hidden rounded-xl border border-black/5">
          {toneScale(kit.primary.value).map((color) => <i key={color} style={{ background: color }} title={color} />)}
        </div>
        <p className="mb-0 mt-3 text-xs leading-5 text-secondary">{strings.brandingToneScaleHint}</p>
      </Insight>
      <Insight title={strings.brandingContrast} meta="WCAG AA">
        <div className="mt-3 grid gap-2">
          <ContrastRow label={strings.brandingPrimary} color={kit.primary.value} ink={primaryInk} />
          <ContrastRow label={strings.brandingSecondary} color={secondary} ink={secondaryInk} />
        </div>
      </Insight>
      <Insight title={strings.brandingColorBalance} meta="70 / 20 / 10">
        <div className="mt-4 flex h-10 overflow-hidden rounded-xl border border-black/5">
          <i className="basis-[70%] bg-raised" />
          <i className="basis-[20%]" style={{ background: secondary }} />
          <i className="basis-[10%]" style={{ background: kit.primary.value }} />
        </div>
        <p className="mb-0 mt-3 text-xs leading-5 text-secondary">{strings.brandingColorBalanceHint}</p>
      </Insight>
      </div>
    </section>
  );
}

function Insight({ title, meta, children }: { title: string; meta: string; children: ReactNode }) {
  return (
    <section className="min-w-0 p-5">
      <div className="flex items-center justify-between gap-3"><h3 className="m-0 text-sm font-semibold text-primary">{title}</h3><span className="text-[0.68rem] font-semibold uppercase tracking-wide text-tertiary">{meta}</span></div>
      {children}
    </section>
  );
}

function ContrastRow({ label, color, ink }: { label: string; color: string; ink: string }) {
  const pass = contrastPasses(color, ink);
  return (
    <div className="flex items-center gap-3 rounded-xl bg-raised p-2.5">
      <span className="grid size-10 shrink-0 place-items-center rounded-lg text-sm font-bold" style={{ background: color, color: ink }}>Aa</span>
      <div className="min-w-0 flex-1"><strong className="block truncate text-xs text-primary">{label}</strong><small className="text-[0.68rem] text-tertiary">{ink === "#FFFFFF" ? strings.brandingUseLightText : strings.brandingUseDarkText}</small></div>
      {pass ? <CheckCircle2 className="size-4 text-success" /> : <AlertCircle className="size-4 text-danger" />}
    </div>
  );
}
