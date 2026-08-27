import { Minus, Plus } from "lucide-react";
import { strings } from "../../i18n";
import type { QuoteStudioBlock } from "./QuoteStudioBlock";

type ImageBlock = Extract<QuoteStudioBlock, { kind: "image" }>;
const STEPS = [50, 75, 100, 125, 150, 175, 200] as const;

interface ImageZoomControlProps {
  value: Exclude<ImageBlock["zoom"], undefined>;
  minimum?: 50 | 100;
  onChange: (value: Exclude<ImageBlock["zoom"], undefined>) => void;
}

export function ImageZoomControl({ value, minimum = 50, onChange }: ImageZoomControlProps) {
  const index = STEPS.indexOf(value);
  const minimumIndex = STEPS.indexOf(minimum);
  const previous = STEPS[Math.max(minimumIndex, index - 1)] ?? minimum;
  const next = STEPS[Math.min(STEPS.length - 1, index + 1)] ?? 200;
  return (
    <section className="min-w-0">
      <div className="flex items-center justify-between gap-3">
        <h4 className="text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.quoteStudioZoom}</h4>
        <button type="button" className="rounded-md px-2 py-1 text-xs font-semibold text-secondary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-default disabled:opacity-35" disabled={value === 100} onClick={() => onChange(100)}>{strings.quoteStudioReset}</button>
      </div>
      <div className="mt-2 grid grid-cols-[2.5rem_minmax(0,1fr)_2.5rem] items-center gap-1 rounded-xl border border-default bg-raised/60 p-1 shadow-sm">
        <button type="button" className="grid size-10 place-items-center rounded-lg text-primary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-not-allowed disabled:opacity-35" aria-label={strings.quoteStudioZoomOut} disabled={index <= minimumIndex} onClick={() => onChange(previous)}><Minus className="size-4" aria-hidden="true" /></button>
        <strong className="text-center text-sm font-semibold tabular-nums text-primary">{value}%</strong>
        <button type="button" className="grid size-10 place-items-center rounded-lg text-primary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-not-allowed disabled:opacity-35" aria-label={strings.quoteStudioZoomIn} disabled={index === STEPS.length - 1} onClick={() => onChange(next)}><Plus className="size-4" aria-hidden="true" /></button>
      </div>
      <div className="mt-2 flex justify-between px-1 text-[11px] text-tertiary"><span>{minimum}%</span><span>100%</span><span>200%</span></div>
    </section>
  );
}
