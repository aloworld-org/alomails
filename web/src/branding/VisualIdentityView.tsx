import { strings } from "../i18n";
import { BrandColorPalette } from "./BrandColorPalette";
import { BrandLogoField } from "./BrandLogoField";
import { BrandTypographyFields } from "./BrandTypographyFields";
import { PaletteInsights } from "./PaletteInsights";
import type { BrandKit } from "./model";

export function VisualIdentityView({ kit, onChange }: { kit: BrandKit; onChange: (kit: BrandKit) => void }) {
  return (
    <section aria-labelledby="visual-identity-title">
      <div className="mb-6 max-w-3xl">
        <h2 id="visual-identity-title" className="m-0 text-2xl font-semibold tracking-tight text-primary">{strings.brandingVisualIdentityTitle}</h2>
        <p className="mb-0 mt-2 text-sm leading-6 text-secondary">{strings.brandingVisualIdentitySubtitle}</p>
      </div>
      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(24rem,0.8fr)]">
        <div className="grid content-start gap-6">
          <BrandLogoField
            logos={kit.logos}
            primaryLogoId={kit.primaryLogoId}
            onChange={(logos, primaryLogoId) => onChange({ ...kit, logos, primaryLogoId })}
          />
          <BrandColorPalette kit={kit} onChange={onChange} />
        </div>
        <div className="grid content-start gap-6">
          <BrandTypographyFields typography={kit.typography} onChange={(typography) => onChange({ ...kit, typography })} />
          <PaletteInsights kit={kit} />
        </div>
      </div>
    </section>
  );
}
