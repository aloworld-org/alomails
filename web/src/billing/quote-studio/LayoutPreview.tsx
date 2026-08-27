import { cx } from "../../ds";
import type { QuoteTableLayout } from "../quoteTableOptions";

export function LayoutPreview({ layout, selected }: { layout: QuoteTableLayout; selected: boolean }) {
  return (
    <span className={cx("block rounded-xl border p-3", selected ? "border-accent/25 bg-surface" : "border-subtle bg-raised/45")} aria-hidden="true">
      <span className="mb-2 flex items-center gap-2 border-b border-subtle pb-2">
        {layout === "catalogue" && <span data-testid="catalogue-image" className="size-6 rounded-md bg-accent-soft" />}
        <span className="h-1.5 w-16 rounded-full bg-secondary/25" />
        <span className="ml-auto h-1.5 w-8 rounded-full bg-accent/55" />
      </span>
      {[0, 1].map((row) => (
        <span key={row} className="flex items-center gap-2 py-1.5">
          {layout === "catalogue" && <span className="size-8 shrink-0 rounded-md bg-accent-soft" />}
          <span className="min-w-0 flex-1">
            <span className="block h-1.5 rounded-full bg-primary/20" />
            {layout !== "compact" && <span className="mt-1.5 block h-1 w-3/4 rounded-full bg-secondary/15" />}
          </span>
          <span className="h-1.5 w-8 rounded-full bg-secondary/20" />
        </span>
      ))}
    </span>
  );
}
