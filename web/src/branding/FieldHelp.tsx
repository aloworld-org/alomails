import { useCallback, useRef, useState } from "react";
import { Info } from "lucide-react";

import { useDismiss } from "../ds/useDismiss";
import { strings } from "../i18n";

export function FieldHelp({ title, children }: { title: string; children: string }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, root, close);

  return (
    <span ref={root} className="relative inline-flex shrink-0">
      <button
        type="button"
        aria-label={strings.brandingMoreInfo(title)}
        aria-expanded={open}
        aria-haspopup="dialog"
        className="grid size-7 place-items-center rounded-full border-0 bg-transparent text-tertiary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30 aria-expanded:bg-accent-soft aria-expanded:text-accent"
        onClick={() => setOpen((current) => !current)}
      >
        <Info size={15} aria-hidden="true" />
      </button>
      {open && (
        <span className="absolute left-0 top-full z-[var(--z-popover)] mt-1 grid w-[min(19rem,calc(100vw-3rem))] gap-1 rounded-xl border border-default bg-surface p-3 text-left text-xs leading-5 text-primary shadow-lg" role="dialog" aria-label={title}>
          <strong className="font-semibold">{title}</strong>
          <span className="text-secondary">{children}</span>
        </span>
      )}
    </span>
  );
}
