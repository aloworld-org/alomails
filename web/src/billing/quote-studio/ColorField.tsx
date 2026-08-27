import { Copy } from "lucide-react";

import { ColorPicker, IconButton } from "../../ds";
import { strings } from "../../i18n";

export function ColorField({
  label,
  help,
  value,
  onChange,
}: {
  label: string;
  help: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const fieldId = `quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`;

  return (
    <div className="flex min-h-20 items-center gap-4 rounded-xl border border-default bg-surface px-4 py-3 focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/10">
      <ColorPicker
        label={strings.quoteStudioChooseColour}
        value={value}
        onChange={onChange}
        triggerClassName="!size-14 !rounded-xl"
      />
      <div className="min-w-0 flex-1">
        <label
          className="block text-sm font-semibold text-primary"
          htmlFor={fieldId}
        >
          {label}
        </label>
        <p className="mt-1 text-xs text-secondary">{help}</p>
      </div>
      <input
        id={fieldId}
        value={value.toUpperCase()}
        aria-label={strings.quoteStudioHexColour}
        className="h-10 w-[6.25rem] shrink-0 rounded-lg border border-default bg-raised px-3 font-mono text-sm font-medium uppercase text-primary outline-none focus:border-accent focus:ring-2 focus:ring-accent/10"
        maxLength={7}
        spellCheck={false}
        onChange={(event) => {
          const next = event.target.value.startsWith("#")
            ? event.target.value
            : `#${event.target.value}`;
          onChange(next.slice(0, 7));
        }}
      />
      <IconButton
        label={strings.quoteStudioCopyColour}
        icon={<Copy />}
        onClick={() => void navigator.clipboard?.writeText(value.toUpperCase())}
      />
    </div>
  );
}
