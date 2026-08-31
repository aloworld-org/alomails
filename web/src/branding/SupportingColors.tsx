import { Plus } from "lucide-react";

import { strings } from "../i18n";
import { BrandColorField } from "./BrandColorField";
import { FieldHelp } from "./FieldHelp";
import { MAX_SUPPORTING_COLORS, addSupportingColor, type BrandKit } from "./model";

export function SupportingColors({
  kit,
  onChange,
}: {
  kit: BrandKit;
  onChange: (kit: BrandKit) => void;
}) {
  return (
    <section className="border-t border-subtle p-5">
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-1">
            <h2 className="m-0 text-base font-semibold text-primary">{strings.brandingSupportingTitle}</h2>
            <FieldHelp title={strings.brandingSupportingTitle}>
              {`${strings.brandingSupportingHint} ${strings.brandingSupportingLimit}`}
            </FieldHelp>
          </div>
        </div>
        {kit.supporting.length < MAX_SUPPORTING_COLORS && (
          <button type="button" className="grid size-9 shrink-0 place-items-center rounded-lg border border-default bg-surface text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/10" aria-label={strings.brandingAddSupporting} onClick={() => onChange(addSupportingColor(kit))}>
            <Plus size={17} />
          </button>
        )}
      </div>

      {kit.supporting.length === 0 ? (
        <button type="button" className="flex min-h-20 w-full items-center gap-3 rounded-xl border border-dashed border-default bg-raised px-4 text-left transition-colors hover:border-accent hover:bg-accent-soft" aria-label={strings.brandingAddSupporting} onClick={() => onChange(addSupportingColor(kit))}>
          <span className="grid size-9 shrink-0 place-items-center rounded-full bg-surface text-accent shadow-sm" aria-hidden="true"><Plus size={16} /></span>
          <span className="min-w-0">
            <strong className="block text-sm font-semibold text-primary">{strings.brandingAddSupporting}</strong>
            <small className="mt-0.5 block text-xs leading-4 text-tertiary">{strings.brandingSupportingLimit}</small>
          </span>
        </button>
      ) : (
        <div className="grid gap-3">
          {kit.supporting.map((color, index) => (
            <BrandColorField
              key={color.id}
              color={color}
              title={color.name || strings.brandingSupportingName(index + 1)}
              editableName
              onChange={(next) =>
                onChange({
                  ...kit,
                  supporting: kit.supporting.map((item, itemIndex) =>
                    itemIndex === index ? next : item,
                  ),
                })
              }
              onRemove={() =>
                onChange({
                  ...kit,
                  supporting: kit.supporting.filter((_, itemIndex) => itemIndex !== index),
                })
              }
            />
          ))}
        </div>
      )}
    </section>
  );
}
