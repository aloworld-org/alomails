import { strings } from "../i18n";
import { BrandTextField } from "./BrandTextField";
import type { BrandFoundation, BrandKit } from "./model";

export function BrandFoundationView({ kit, onChange }: { kit: BrandKit; onChange: (kit: BrandKit) => void }) {
  function update(field: keyof BrandFoundation, value: string) {
    onChange({ ...kit, foundation: { ...kit.foundation, [field]: value } });
  }

  return (
    <section aria-labelledby="brand-foundation-title">
      <div className="mb-6 max-w-3xl">
        <h2 id="brand-foundation-title" className="m-0 text-2xl font-semibold tracking-tight text-primary">{strings.brandingFoundationTitle}</h2>
        <p className="mb-0 mt-2 text-sm leading-6 text-secondary">{strings.brandingFoundationSubtitle}</p>
      </div>
      <div className="grid overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm lg:grid-cols-2 lg:divide-x lg:divide-subtle">
        <div className="grid content-start gap-6 p-5 sm:p-6 lg:p-8">
          <BrandTextField label={strings.brandingBrandName} hint={strings.brandingBrandNameHint} placeholder={strings.brandingBrandNamePlaceholder} value={kit.foundation.name} maximum={120} onChange={(value) => update("name", value)} />
          <BrandTextField label={strings.brandingTagline} hint={strings.brandingTaglineHint} placeholder={strings.brandingTaglinePlaceholder} value={kit.foundation.tagline} maximum={180} onChange={(value) => update("tagline", value)} />
          <BrandTextField multiline label={strings.brandingPurpose} hint={strings.brandingPurposeHint} placeholder={strings.brandingPurposePlaceholder} value={kit.foundation.purpose} maximum={600} onChange={(value) => update("purpose", value)} />
          <BrandTextField multiline label={strings.brandingAudience} hint={strings.brandingAudienceHint} placeholder={strings.brandingAudiencePlaceholder} value={kit.foundation.audience} maximum={600} onChange={(value) => update("audience", value)} />
        </div>
        <div className="grid content-start gap-6 border-t border-subtle p-5 sm:p-6 lg:border-t-0 lg:p-8">
          <BrandTextField multiline label={strings.brandingPositioning} hint={strings.brandingPositioningHint} placeholder={strings.brandingPositioningPlaceholder} value={kit.foundation.positioning} maximum={600} onChange={(value) => update("positioning", value)} />
          <BrandTextField label={strings.brandingPersonality} hint={strings.brandingPersonalityHint} placeholder={strings.brandingPersonalityPlaceholder} value={kit.foundation.personality} maximum={300} onChange={(value) => update("personality", value)} />
          <BrandTextField multiline label={strings.brandingVoice} hint={strings.brandingVoiceHint} placeholder={strings.brandingVoicePlaceholder} value={kit.foundation.voice} maximum={600} onChange={(value) => update("voice", value)} />
        </div>
      </div>
    </section>
  );
}
