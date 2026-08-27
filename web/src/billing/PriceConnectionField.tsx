import type { ReactNode } from "react";

export function PriceConnectionField({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <label className="flex min-w-0 flex-col gap-2"><span className="text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</span>{children}{hint !== undefined && <span className="text-xs leading-relaxed text-tertiary">{hint}</span>}</label>;
}
