import { Check } from "lucide-react";

import { cx } from "../../ds";

interface TableToggleProps {
  label: string;
  help: string;
  checked: boolean;
  onClick: () => void;
}

export function TableToggle({
  label,
  help,
  checked,
  onClick,
}: TableToggleProps) {
  return (
    <button
      type="button"
      aria-pressed={checked}
      className={cx(
        "flex min-h-24 items-center gap-4 rounded-xl border !px-5 !py-4 text-left transition-colors hover:border-accent hover:bg-accent-soft/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
        checked ? "border-accent bg-accent-soft" : "border-default bg-surface",
      )}
      onClick={onClick}
    >
      <span
        className={cx(
          "flex size-5 shrink-0 items-center justify-center rounded border",
          checked
            ? "border-accent bg-accent text-on-accent"
            : "border-default bg-surface",
        )}
      >
        {checked && <Check className="size-3.5" />}
      </span>
      <span>
        <strong className="block text-sm font-semibold text-primary">
          {label}
        </strong>
        <span className="mt-0.5 block text-xs text-secondary">{help}</span>
      </span>
    </button>
  );
}
