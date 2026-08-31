import { Check, Palette } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import type { BrandKitController } from "./useBrandKit";

export function BrandingHeader({ brand }: { brand: BrandKitController }) {
  const status = brand.saveFailed
    ? strings.brandingSaveFailed
    : brand.savedNotice
      ? strings.brandingSaved
      : brand.dirty
        ? strings.brandingUnsaved
        : "";

  return (
    <header className="shrink-0 border-b border-subtle bg-surface px-4 py-5 sm:px-6 lg:px-8 print:hidden">
      <div className="mx-auto flex w-full max-w-[94rem] flex-wrap items-center justify-between gap-5">
        <div className="flex min-w-0 items-center gap-3.5">
          <span className="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-accent-soft text-accent shadow-sm ring-1 ring-inset ring-accent/10" aria-hidden="true">
            <Palette className="size-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-bold tracking-tight text-primary">{strings.brandingTitle}</h1>
            <p className="mb-0 mt-1 max-w-3xl text-sm leading-5 text-secondary">{strings.brandingSubtitle}</p>
          </div>
        </div>
        <div className="flex min-h-control items-center gap-3">
          <span className={`inline-flex max-w-sm items-center justify-end gap-1 text-xs ${brand.saveFailed ? "text-danger" : brand.savedNotice ? "text-success" : "text-tertiary"}`} role="status" aria-live="polite">
            {brand.savedNotice && <Check size={14} aria-hidden="true" />}
            {status}
          </span>
          <Button disabled={!brand.dirty || !brand.valid} onClick={brand.save}>{strings.brandingSave}</Button>
        </div>
      </div>
    </header>
  );
}
