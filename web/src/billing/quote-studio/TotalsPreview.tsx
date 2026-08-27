import { cx } from "../../ds";
import type { QuoteTotalsPlacement } from "../quoteTableOptions";

export function TotalsPreview({ placement }: { placement: QuoteTotalsPlacement }) {
  return (
    <span className="block h-20 rounded-xl border border-subtle bg-raised/40 p-3" aria-hidden="true">
      <span className="block h-5 rounded bg-surface" />
      <span
        className={cx(
          "mt-2 flex flex-col gap-1 rounded-md bg-surface p-2",
          placement === "summary" && "ml-auto w-1/2",
          placement === "full" && "w-full",
          placement === "footer" && "mt-3 w-full rounded-t-none border-t border-accent/35",
        )}
      >
        <span className="flex justify-between">
          <span className="h-1 w-8 rounded bg-secondary/20" />
          <span className="h-1 w-6 rounded bg-secondary/20" />
        </span>
        <span className="flex justify-between">
          <span className="h-1 w-6 rounded bg-primary/25" />
          <span className="h-1 w-8 rounded bg-accent/55" />
        </span>
      </span>
    </span>
  );
}
