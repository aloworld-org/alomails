import { cx } from "../../ds";
import { ImageOptionPreview } from "./ImageOptionPreview";

interface ImageOptionGroupProps<T extends string | number> {
  label: string;
  visual?: "composition" | "frame" | "fit";
  value: T;
  options: Array<readonly [T, string]>;
  onChange: (value: T) => void;
}

export function ImageOptionGroup<T extends string | number>({ label, visual, value, options, onChange }: ImageOptionGroupProps<T>) {
  return (
    <fieldset className="min-w-0">
      <legend className="sr-only">{label}</legend>
      <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</p>
      <div className={cx("grid", visual ? "gap-2" : "gap-1 rounded-xl border border-default bg-raised/60 p-1 shadow-sm", options.length === 3 ? "grid-cols-3" : "grid-cols-2")}>
        {options.map(([id, name]) => (
          <button
            key={id}
            type="button"
            aria-label={name}
            aria-pressed={value === id}
            className={cx(
              "group relative whitespace-nowrap border text-center text-sm font-medium transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent",
              visual ? "h-20 rounded-xl bg-transparent p-2" : "min-h-11 rounded-lg px-3",
              value === id
                ? visual
                  ? "border-accent bg-transparent text-accent ring-1 ring-inset ring-accent/15"
                  : "border-accent/30 bg-accent-soft font-semibold text-accent shadow-sm ring-1 ring-inset ring-accent/15"
                : visual
                  ? "border-transparent text-secondary"
                  : "border-transparent bg-transparent text-secondary",
            )}
            onClick={() => onChange(id)}
          >
            {visual ? <ImageOptionPreview kind={visual} option={String(id)} /> : <span>{name}</span>}
            {visual && <span className="pointer-events-none absolute bottom-[calc(100%+.5rem)] left-1/2 z-20 -translate-x-1/2 whitespace-nowrap rounded-lg bg-primary px-2.5 py-1.5 text-xs font-medium text-surface opacity-0 shadow-md transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">{name}</span>}
          </button>
        ))}
      </div>
    </fieldset>
  );
}
