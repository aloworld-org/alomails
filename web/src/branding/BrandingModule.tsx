import { Check, Palette, Plus } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { BrandColorField } from "./BrandColorField";
import { BrandApplicationPreview } from "./BrandApplicationPreview";
import { FieldHelp } from "./FieldHelp";
import { PaletteInsights } from "./PaletteInsights";
import { SupportingColors } from "./SupportingColors";
import { DEFAULT_BRAND_KIT } from "./model";
import { useBrandKit } from "./useBrandKit";

export function BrandingModule() {
  const brand = useBrandKit();

  return (
    <main className="flex h-full min-h-0 flex-col bg-app text-primary">
      <header className="shrink-0 border-b border-subtle bg-surface px-5 py-5 lg:px-8">
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
          <span
            className={`inline-flex min-w-32 items-center justify-end gap-1 text-xs ${brand.savedNotice ? "text-success" : "text-tertiary"}`}
            role="status"
          >
            {brand.savedNotice ? (
              <><Check size={14} />{strings.brandingSaved}</>
            ) : brand.dirty ? strings.brandingUnsaved : null}
          </span>
          <Button disabled={!brand.dirty || !brand.valid} onClick={brand.save}>
            {strings.brandingSave}
          </Button>
        </div>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-auto px-5 py-6 lg:px-8 lg:py-7">
        <div className="mx-auto grid w-full max-w-[94rem] items-start gap-6 xl:grid-cols-[24rem_minmax(0,1fr)]">
          <aside className="overflow-visible rounded-2xl border border-subtle bg-surface shadow-sm xl:sticky xl:top-0">
            <section className="p-5">
              <div className="mb-5">
                <div className="flex items-center gap-1">
                  <h2 className="m-0 text-lg font-semibold text-primary">{strings.brandingAccentsTitle}</h2>
                  <FieldHelp title={strings.brandingAccentsTitle}>
                    {strings.brandingAccentsHint}
                  </FieldHelp>
                </div>
                <p className="mb-0 mt-1 text-sm leading-5 text-secondary">{strings.brandingAccentsHint}</p>
              </div>
              <div className="grid gap-3">
              <BrandColorField
                color={brand.draft.primary}
                title={strings.brandingPrimary}
                hint={strings.brandingPrimaryHint}
                onChange={(primary) => brand.setDraft({ ...brand.draft, primary })}
              />
              {brand.draft.secondary === null ? (
                <button
                  type="button"
                  className="flex min-h-32 w-full cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-default bg-transparent p-4 text-center text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-primary"
                  onClick={() =>
                    brand.setDraft({
                      ...brand.draft,
                      secondary: { ...DEFAULT_BRAND_KIT.secondary! },
                    })
                  }
                >
                  <span className="grid size-10 place-items-center rounded-full bg-raised text-accent"><Plus size={18} /></span>
                  <strong className="text-sm">{strings.brandingAddSecondary}</strong>
                </button>
              ) : (
                <BrandColorField
                  color={brand.draft.secondary}
                  title={strings.brandingSecondary}
                  hint={strings.brandingSecondaryHint}
                  onChange={(secondary) => brand.setDraft({ ...brand.draft, secondary })}
                  onRemove={() => brand.setDraft({ ...brand.draft, secondary: null })}
                />
              )}
              </div>
            </section>

            <SupportingColors kit={brand.draft} onChange={brand.setDraft} />

            {!brand.valid && (
              <p className="m-5 rounded-lg bg-danger-tint px-4 py-3 text-sm text-danger-text" role="alert">{strings.brandingInvalidColor}</p>
            )}
          </aside>
          <div className="grid min-w-0 gap-5">
            <BrandApplicationPreview kit={brand.draft} />
            <PaletteInsights kit={brand.draft} />
          </div>
        </div>
      </div>
    </main>
  );
}
