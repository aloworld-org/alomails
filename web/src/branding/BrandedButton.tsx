import type { ReactNode } from "react";

export function BrandedButton({ secondary = false, children }: { secondary?: boolean; children: ReactNode }) {
  return <span className={secondary
    ? "inline-flex min-h-10 items-center rounded-lg border border-[var(--brand-secondary)] bg-transparent px-4 text-sm font-semibold text-[var(--brand-secondary)]"
    : "inline-flex min-h-10 items-center rounded-lg bg-[var(--brand-primary)] px-4 text-sm font-semibold text-[var(--brand-primary-ink)]"
  }>{children}</span>;
}
