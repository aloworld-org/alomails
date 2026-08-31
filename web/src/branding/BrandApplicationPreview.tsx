import { FileSignature, FileText, Globe2, Megaphone } from "lucide-react";
import { useState } from "react";

import { strings } from "../i18n";
import { brandPresentationVariables } from "./brandPresentation";
import { CampaignBrandPreview } from "./CampaignBrandPreview";
import { DocumentBrandPreview } from "./DocumentBrandPreview";
import type { BrandKit } from "./model";
import { QuotationBrandPreview } from "./QuotationBrandPreview";
import { WebsiteBrandPreview } from "./WebsiteBrandPreview";

type Preview = "website" | "quotation" | "campaign" | "document";

export function BrandApplicationPreview({ kit }: { kit: BrandKit }) {
  const [preview, setPreview] = useState<Preview>("website");
  const previews = [
    ["website", strings.brandingPreviewWebsite, Globe2],
    ["quotation", strings.brandingPreviewDocument, FileSignature],
    ["campaign", strings.brandingPreviewCampaign, Megaphone],
    ["document", strings.brandingPreviewWorkspaceDocument, FileText],
  ] as const;

  return (
    <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm" style={brandPresentationVariables(kit)}>
      <header className="flex flex-wrap items-center justify-between gap-5 border-b border-subtle px-5 py-4 lg:px-6">
        <div><h3 className="m-0 text-lg font-semibold text-primary">{strings.brandingVisualStudio}</h3><p className="mb-0 mt-1 text-sm text-secondary">{strings.brandingSeeItInUse}</p></div>
        <div className="inline-flex max-w-full gap-1 overflow-x-auto rounded-xl border border-subtle bg-raised p-1" role="tablist" aria-label={strings.brandingPreviewContexts}>
          {previews.map(([id, label, Icon]) => <button key={id} type="button" role="tab" aria-selected={preview === id} className={`inline-flex min-h-9 shrink-0 items-center gap-2 rounded-lg px-3 text-xs font-medium transition-[background-color,color,box-shadow] ${preview === id ? "bg-surface text-primary shadow-sm ring-1 ring-inset ring-subtle" : "text-secondary hover:bg-surface/60 hover:text-primary"}`} onClick={() => setPreview(id)}><Icon size={15} aria-hidden="true" />{label}</button>)}
        </div>
      </header>
      <div className="min-h-[34rem] bg-raised p-4 sm:p-6 lg:p-8">
        {preview === "website" && <WebsiteBrandPreview kit={kit} />}
        {preview === "quotation" && <QuotationBrandPreview kit={kit} />}
        {preview === "campaign" && <CampaignBrandPreview kit={kit} />}
        {preview === "document" && <DocumentBrandPreview kit={kit} />}
      </div>
    </section>
  );
}
