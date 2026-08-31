import { strings } from "../i18n";
import { BrandColorBalance } from "./BrandColorBalance";
import { BrandToneScale } from "./BrandToneScale";
import { readableInk } from "./colorTools";
import { ContrastRow } from "./ContrastRow";
import type { BrandKit } from "./model";
import { PaletteInsightCard } from "./PaletteInsightCard";

export function PaletteInsights({ kit }: { kit: BrandKit }) {
  const primaryInk = readableInk(kit.primary.value);
  const secondary = kit.secondary?.value ?? kit.primary.value;
  const secondaryInk = readableInk(secondary);

  return (
    <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
      <header className="flex flex-col gap-3 border-b border-subtle px-5 py-5 sm:flex-row sm:items-start sm:justify-between lg:px-6">
        <div>
          <h3 className="m-0 text-base font-semibold text-primary">{strings.brandingColorBalance}</h3>
          <p className="mb-0 mt-1 max-w-2xl text-sm leading-5 text-secondary">{strings.brandingColorBalanceHint}</p>
        </div>
        <span className="w-fit shrink-0 rounded-full bg-accent-soft px-3 py-1 text-xs font-semibold tabular-nums text-accent">{strings.brandingBalanceRatio}</span>
      </header>
      <div className="p-5 lg:p-6">
        <BrandColorBalance primary={kit.primary.value} secondary={secondary} />
      </div>
      <div className="grid border-t border-subtle md:grid-cols-2 md:divide-x md:divide-subtle">
        <PaletteInsightCard title={strings.brandingToneScale} meta={strings.brandingGenerated}><BrandToneScale color={kit.primary.value} /><p className="mb-0 mt-3 text-xs leading-5 text-secondary">{strings.brandingToneScaleHint}</p></PaletteInsightCard>
        <PaletteInsightCard title={strings.brandingContrast} meta={strings.brandingWcagAa}><div className="mt-3 grid gap-2"><ContrastRow label={strings.brandingPrimary} color={kit.primary.value} ink={primaryInk} /><ContrastRow label={strings.brandingSecondary} color={secondary} ink={secondaryInk} /></div></PaletteInsightCard>
      </div>
    </section>
  );
}
