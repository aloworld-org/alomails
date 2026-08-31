import { ColorPicker, IconButton } from "../ds";
import { strings } from "../i18n";
import { Trash2 } from "lucide-react";

import type { BrandColor } from "./model";
import { FieldHelp } from "./FieldHelp";

export function BrandColorField({
  color,
  title,
  hint,
  editableName = false,
  onChange,
  onRemove,
}: {
  color: BrandColor;
  title: string;
  hint?: string;
  editableName?: boolean;
  onChange: (color: BrandColor) => void;
  onRemove?: () => void;
}) {
  return (
    <article className="relative min-w-0 rounded-xl border border-subtle bg-surface p-4 transition-[border-color,box-shadow] hover:border-default hover:shadow-sm">
      <div className="mb-3 flex min-h-7 items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-1">
            <h3 className="m-0 text-sm font-semibold text-primary">{title}</h3>
            {hint !== undefined && <FieldHelp title={title}>{hint}</FieldHelp>}
          </div>
        </div>
        {onRemove !== undefined && (
          <span className="absolute right-3 top-3">
          <IconButton
            size="sm"
            label={strings.brandingRemoveColor(color.name || title)}
            icon={<Trash2 size={16} />}
            onClick={onRemove}
          />
          </span>
        )}
      </div>
      <div className="grid grid-cols-[auto_minmax(8rem,1fr)] items-center gap-3">
        <ColorPicker
          label={title}
          value={color.value}
          triggerClassName="!size-12 !rounded-xl !shadow-sm"
          onChange={(value) => onChange({ ...color, value })}
        />
        <label className="grid min-w-0 gap-1">
          <span className="sr-only">{strings.brandingColorHex}</span>
          <input
            value={color.value}
            maxLength={7}
            autoCapitalize="characters"
            spellCheck={false}
            aria-label={`${title} ${strings.brandingColorHex}`}
            className="min-h-12 w-full min-w-0 rounded-xl border border-default bg-raised px-3.5 font-mono text-sm text-primary uppercase outline-none transition-[border-color,box-shadow] focus:border-accent focus:bg-surface focus:ring-4 focus:ring-accent/10"
            onChange={(event) => onChange({ ...color, value: event.target.value.toUpperCase() })}
          />
        </label>
        {editableName && (
          <label className="col-span-full grid min-w-0 gap-1">
            <span className="text-xs font-medium text-tertiary">{strings.brandingColorName}</span>
            <input
              value={color.name}
              maxLength={32}
              className="min-h-control w-full min-w-0 rounded-lg border border-default bg-surface px-3 text-primary outline-none transition-[border-color,box-shadow] focus:border-accent focus:ring-2 focus:ring-accent/10"
              onChange={(event) => onChange({ ...color, name: event.target.value })}
            />
          </label>
        )}
      </div>
    </article>
  );
}
