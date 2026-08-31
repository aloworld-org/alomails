import { Navigate, Route, Routes } from "react-router-dom";

import { BrandApplicationsView } from "./BrandApplicationsView";
import { BrandFoundationView } from "./BrandFoundationView";
import { BrandGuidelinesView } from "./BrandGuidelinesView";
import { BrandingHeader } from "./BrandingHeader";
import { BrandingNavigation } from "./BrandingNavigation";
import { VisualIdentityView } from "./VisualIdentityView";
import { useBrandKit } from "./useBrandKit";

export function BrandingModule() {
  const brand = useBrandKit();

  return (
    <main className="flex h-full min-h-0 flex-col bg-app text-primary">
      <BrandingHeader brand={brand} />
      <BrandingNavigation />
      <div className="min-h-0 flex-1 overflow-auto px-4 py-6 sm:px-6 lg:px-8 lg:py-8">
        <div className="mx-auto w-full max-w-[94rem]">
          <Routes>
            <Route index element={<Navigate to="foundation" replace />} />
            <Route path="foundation" element={<BrandFoundationView kit={brand.draft} onChange={brand.setDraft} />} />
            <Route path="visual-identity" element={<VisualIdentityView kit={brand.draft} onChange={brand.setDraft} />} />
            <Route path="applications" element={<BrandApplicationsView kit={brand.draft} />} />
            <Route path="guidelines" element={<BrandGuidelinesView kit={brand.draft} />} />
            <Route path="*" element={<Navigate to="foundation" replace />} />
          </Routes>
        </div>
      </div>
    </main>
  );
}
