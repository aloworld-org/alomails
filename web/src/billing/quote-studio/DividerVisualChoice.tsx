import type { ReactNode } from "react";
import { Check } from "lucide-react";

import { cx } from "../../ds";

export function DividerVisualChoice({
  label,
  selected,
  compact = false,
  onClick,
  children,
}: {
  label: string;
  selected: boolean;
  compact?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      className="group block w-full rounded-xl text-left focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15"
      onClick={onClick}
    >
      <span
        className={cx(
          "relative flex w-full flex-col rounded-xl border transition-colors duration-150",
          compact ? "min-h-[6.5rem] p-3" : "min-h-[10rem] p-4",
          selected
            ? "border-accent bg-accent-soft/20"
            : "border-default bg-surface group-hover:border-accent group-hover:bg-accent-soft/20",
        )}
      >
        {selected && (
          <span
            className={cx(
              "absolute inline-flex size-5 items-center justify-center rounded-full bg-accent text-white",
              compact ? "right-2 top-2" : "right-3 top-3",
            )}
          >
            <Check className="size-3.5" aria-hidden="true" />
          </span>
        )}
        <span
          className={cx(
            "flex w-full items-center",
            compact ? "min-h-10 px-2 pr-7" : "min-h-16 px-3 pr-8",
          )}
          aria-hidden="true"
        >
          {children}
        </span>
        <span
          className={cx(
            "mt-auto text-center text-sm font-medium text-primary",
            compact ? "pt-2" : "pt-4",
          )}
        >
          {label}
        </span>
      </span>
    </button>
  );
}
