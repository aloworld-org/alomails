import { strings } from "../i18n";
import { BrandApplicationPreview } from "./BrandApplicationPreview";
import type { BrandKit } from "./model";

export function BrandApplicationsView({ kit }: { kit: BrandKit }) {
  return (
    <section aria-labelledby="brand-applications-title">
      <div className="mb-6 max-w-3xl"><h2 id="brand-applications-title" className="m-0 text-2xl font-semibold tracking-tight text-primary">{strings.brandingApplicationsTitle}</h2><p className="mb-0 mt-2 text-sm leading-6 text-secondary">{strings.brandingApplicationsSubtitle}</p></div>
      <BrandApplicationPreview kit={kit} />
    </section>
  );
}
