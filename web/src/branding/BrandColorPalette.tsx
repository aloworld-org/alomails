import { Plus } from "lucide-react";

import { strings } from "../i18n";
import { BrandColorField } from "./BrandColorField";
import { SupportingColors } from "./SupportingColors";
import { DEFAULT_BRAND_KIT, type BrandKit } from "./model";

export function BrandColorPalette({ kit, onChange }: { kit: BrandKit; onChange: (kit: BrandKit) => void }) {
  return (
    <section className="relative rounded-2xl border border-subtle bg-surface shadow-sm" aria-labelledby="brand-colours-title">
      <div className="p-5 sm:p-6">
        <div className="mb-5">
          <h3 id="brand-colours-title" className="m-0 text-lg font-semibold text-primary">{strings.brandingColorsTitle}</h3>
          <p className="mb-0 mt-1 text-sm leading-5 text-secondary">{strings.brandingColorsSubtitle}</p>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <BrandColorField color={kit.primary} title={strings.brandingPrimary} hint={strings.brandingPrimaryHint} onChange={(primary) => onChange({ ...kit, primary })} />
          {kit.secondary === null ? (
            <button type="button" className="flex min-h-32 w-full cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-default bg-transparent p-4 text-center text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-primary" onClick={() => onChange({ ...kit, secondary: { ...DEFAULT_BRAND_KIT.secondary! } })}>
              <span className="grid size-10 place-items-center rounded-full bg-raised text-accent"><Plus size={18} aria-hidden="true" /></span>
              <strong className="text-sm">{strings.brandingAddSecondary}</strong>
            </button>
          ) : (
            <BrandColorField color={kit.secondary} title={strings.brandingSecondary} hint={strings.brandingSecondaryHint} onChange={(secondary) => onChange({ ...kit, secondary })} onRemove={() => onChange({ ...kit, secondary: null })} />
          )}
        </div>
      </div>
      <SupportingColors kit={kit} onChange={onChange} />
    </section>
  );
}
