import { ChoicePicker } from "../ds";
import { strings } from "../i18n";
import { BRAND_FONTS, brandFontStack } from "./brandTypography";
import type { BrandFont, BrandTypography } from "./model";

function fontLabel(font: BrandFont): string {
  if (font === "arial") return strings.brandingFontArial;
  if (font === "georgia") return strings.brandingFontGeorgia;
  if (font === "garamond") return strings.brandingFontGaramond;
  return strings.brandingFontInter;
}

export function BrandTypographyFields({ typography, onChange }: { typography: BrandTypography; onChange: (typography: BrandTypography) => void }) {
  const options = BRAND_FONTS.map((font) => ({ value: font, label: fontLabel(font) }));

  return (
    <section className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm sm:p-6" aria-labelledby="brand-typography-title">
      <div className="mb-5">
        <h3 id="brand-typography-title" className="m-0 text-lg font-semibold text-primary">{strings.brandingTypographyTitle}</h3>
        <p className="mb-0 mt-1 text-sm leading-5 text-secondary">{strings.brandingTypographyHint}</p>
      </div>
      <div className="grid gap-5 sm:grid-cols-2">
        <div className="grid gap-2">
          <span className="font-medium text-primary">{strings.brandingHeadingFont}</span>
          <ChoicePicker value={typography.heading} options={options} placeholder={strings.brandingHeadingFont} label={strings.brandingHeadingFont} onChange={(value) => onChange({ ...typography, heading: value as BrandFont })} />
        </div>
        <div className="grid gap-2">
          <span className="font-medium text-primary">{strings.brandingBodyFont}</span>
          <ChoicePicker value={typography.body} options={options} placeholder={strings.brandingBodyFont} label={strings.brandingBodyFont} onChange={(value) => onChange({ ...typography, body: value as BrandFont })} />
        </div>
      </div>
      <div className="mt-5 grid gap-3 sm:grid-cols-2">
        <div className="rounded-xl bg-raised p-4"><span className="block text-xs font-medium text-tertiary">{strings.brandingHeadingFont}</span><strong className="mt-2 block text-2xl text-primary" style={{ fontFamily: brandFontStack(typography.heading) }}>{strings.brandingSampleName}</strong></div>
        <div className="rounded-xl bg-raised p-4"><span className="block text-xs font-medium text-tertiary">{strings.brandingBodyFont}</span><p className="mb-0 mt-2 text-sm leading-6 text-secondary" style={{ fontFamily: brandFontStack(typography.body) }}>{strings.brandingPreviewWebsiteBody}</p></div>
      </div>
    </section>
  );
}
