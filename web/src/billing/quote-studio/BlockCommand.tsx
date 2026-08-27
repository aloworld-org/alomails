import type { ReactNode } from "react";

import { cx } from "../../ds";

export function BlockCommand({
  label,
  children,
  onClick,
  disabled = false,
  danger = false,
  accent = false,
}: {
  label: string;
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
  accent?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex size-10 shrink-0 items-center justify-center rounded-lg border border-transparent text-xs font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-45",
        danger
          ? "text-danger hover:border-danger/20 hover:bg-danger-tint"
          : accent
            ? "bg-accent-soft text-accent hover:border-accent/25 hover:bg-accent hover:text-on-accent"
            : "text-secondary hover:border-accent/25 hover:bg-accent-soft hover:text-accent",
      )}
      onClick={onClick}
    >
      {children}
      <span className="sr-only">{label}</span>
    </button>
  );
}
