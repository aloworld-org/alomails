import { cloneElement, isValidElement, type ReactNode } from "react";
import { Info } from "lucide-react";
import { strings } from "../i18n";

export function Field({ label, hint, error, children }: { label: string; hint?: string | undefined; error?: string | undefined; children: ReactNode }) {
  const control = isValidElement<{ "aria-label"?: string }>(children) ? cloneElement(children, { "aria-label": children.props["aria-label"] ?? label }) : children;
  return <div className="flex min-w-0 flex-col gap-1.5">
    <div className="flex min-w-0 items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-tertiary"><span>{label}</span>
      {hint !== undefined && error === undefined && <button type="button" className="group relative inline-flex size-5 shrink-0 cursor-help items-center justify-center rounded-full text-tertiary outline-none transition-colors hover:bg-[var(--accent-soft)] hover:text-accent focus-visible:bg-[var(--accent-soft)] focus-visible:text-accent focus-visible:ring-2 focus-visible:ring-accent/20" aria-label={strings.sheetFormulaInformation}><Info className="size-3.5" aria-hidden="true" /><span className="pointer-events-none absolute left-0 top-[calc(100%+.4rem)] z-20 hidden w-max max-w-72 rounded-lg bg-primary px-3 py-2 text-left text-xs font-normal normal-case leading-relaxed tracking-normal text-on-accent shadow-lg group-hover:block group-focus-visible:block" role="tooltip">{hint}</span></button>}
    </div>{control}{error !== undefined && <span className="text-xs leading-relaxed text-danger">{error}</span>}
  </div>;
}
