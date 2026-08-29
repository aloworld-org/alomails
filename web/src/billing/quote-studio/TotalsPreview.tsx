import { cx } from "../../ds";
import type {
  QuoteTotalsPlacement,
  QuoteTotalsStyle,
} from "../quoteTableOptions";

export function TotalsPreview({
  placement,
  style,
}: {
  placement: QuoteTotalsPlacement;
  style: QuoteTotalsStyle;
}) {
  return (
    <span className="block h-20 rounded-xl border border-subtle bg-surface p-3" aria-hidden="true">
      <span className="block h-5 rounded bg-surface" />
      <span
        className={cx(
          "mt-2 flex flex-col gap-1 p-2",
          placement === "summary" && "ml-auto w-1/2",
          placement === "full" && "w-full",
          placement === "footer" && "mt-3 w-full rounded-t-none border-t border-accent/35",
          style === "soft" && "rounded-md bg-raised/70",
          style === "minimal" && "border border-transparent bg-transparent px-0",
          style === "framed" && "rounded-md border-2 border-primary/25 bg-surface",
          style === "accent" && "rounded-md border border-accent/30 bg-surface",
        )}
      >
        <span className="flex justify-between">
          <span className="h-1 w-8 rounded bg-secondary/20" />
          <span className="h-1 w-6 rounded bg-secondary/20" />
        </span>
        <span
          className={cx(
            "flex justify-between",
            style === "accent" && "-mx-1 rounded bg-accent px-1 py-1",
          )}
        >
          <span
            className={cx(
              "h-1 w-6 rounded",
              style === "accent" ? "bg-on-accent/75" : "bg-primary/25",
            )}
          />
          <span
            className={cx(
              "h-1 w-8 rounded",
              style === "accent" ? "bg-on-accent" : "bg-accent/55",
            )}
          />
        </span>
      </span>
    </span>
  );
}
