import { cx } from "../../ds";
import { strings } from "../../i18n";
import type { QuoteStudioBlock } from "./QuoteStudioBlock";

type ImageBlock = Extract<QuoteStudioBlock, { kind: "image" }>;

const RATIOS = [["33-67", 33, 67], ["40-60", 40, 60], ["50-50", 50, 50], ["60-40", 60, 40], ["67-33", 67, 33]] as const;
const WIDTH = { 33: "w-1/3", 40: "w-2/5", 50: "w-1/2", 60: "w-3/5", 67: "w-2/3" } as const;

interface ImageColumnRatioPickerProps {
  value: NonNullable<ImageBlock["columnRatio"]>;
  placement: NonNullable<ImageBlock["placement"]>;
  onChange: (value: NonNullable<ImageBlock["columnRatio"]>) => void;
}

export function ImageColumnRatioPicker({ value, placement, onChange }: ImageColumnRatioPickerProps) {
  const disabled = placement === "full";
  return (
    <fieldset className="min-w-0" disabled={disabled}>
      <legend className="sr-only">{strings.quoteStudioColumnWidth}</legend>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.quoteStudioColumnWidth}</p>
        {disabled && <span className="text-[11px] text-tertiary">{strings.quoteStudioSideBySideOnly}</span>}
      </div>
      <div className="grid grid-cols-5 gap-1.5">
        {RATIOS.map(([id, image, text]) => {
          const selected = value === id;
          const imageFirst = placement !== "right";
          return (
            <button key={id} type="button" aria-label={`${strings.quoteStudioImage} ${image}%, ${strings.quoteStudioText} ${text}%`} aria-pressed={selected} className={cx("group h-20 rounded-xl border bg-surface p-2 transition-colors hover:border-accent hover:bg-accent-soft disabled:cursor-not-allowed disabled:opacity-40", selected ? "border-accent ring-1 ring-inset ring-accent/15" : "border-default")} onClick={() => onChange(id)}>
              <span className="mx-auto flex h-10 max-w-24 gap-1 overflow-hidden rounded-md bg-raised p-1.5">
                <span className={cx("rounded-sm bg-accent/25", imageFirst ? "order-1" : "order-2", WIDTH[image])} />
                <span className={cx("rounded-sm bg-surface shadow-sm", imageFirst ? "order-2" : "order-1", WIDTH[text])} />
              </span>
              <span className={cx("mt-1 block text-center text-[10px] font-semibold tabular-nums", selected ? "text-accent" : "text-tertiary")}>{image}:{text}</span>
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}
