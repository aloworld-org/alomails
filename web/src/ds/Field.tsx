// A labelled control, with its hint and its error (ADR 0045).
//
// Seventeen stylesheets built this before it existed. The layout was never the
// hard part — they all reached the same column — but the wiring was: a label
// that is not bound to its control is invisible to a screen reader, and an
// error that is not announced is invisible to everyone not looking at it.
// Doing that once, correctly, is the whole point of the component.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The seventeen agreed on the layout — a column with a small gap — while
// disagreeing on everything around it: whether the hint sat above or below,
// whether an error replaced the hint or joined it, and whether the label was
// bound to the control at all.
import { Info } from "lucide-react";
import { useId, type ReactNode } from "react";

/** The hint and the error read the same, because they are the same kind of
 *  line: one short run of help under a control. */
const HELP = "text-sm leading-snug";

export interface FieldProps {
  label: string;
  /** What goes in the box. Stays visible when an error is shown. */
  hint?: string | undefined;
  /** Keep longer guidance behind an accessible information control. */
  hintDisplay?: "below" | "tooltip" | undefined;
  /** What went wrong. Announced, and it marks the control invalid. */
  error?: string | undefined;
  /** Receives the id and invalid state to spread onto the control. */
  children: (control: {
    id: string;
    invalid: boolean;
    "aria-describedby": string | undefined;
  }) => ReactNode;
}

export function Field({ label, hint, hintDisplay = "below", error, children }: FieldProps) {
  const id = useId();
  const hintId = hint === undefined ? undefined : `${id}-hint`;
  const errorId = error === undefined ? undefined : `${id}-error`;
  // Both, when both are present: the instruction and the failure are read out
  // together, in that order.
  const describedBy = [hintId, errorId].filter(Boolean).join(" ") || undefined;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-1.5">
        <label className="font-medium text-primary" htmlFor={id}>
          {label}
        </label>
        {hint !== undefined && hintDisplay === "tooltip" && (
          <button
            type="button"
            className="group relative inline-flex size-6 shrink-0 cursor-help items-center justify-center rounded-full text-tertiary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:bg-accent-soft focus-visible:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20"
            aria-label={hint}
          >
            <Info className="size-4" aria-hidden="true" />
            <span className="pointer-events-none absolute left-1/2 top-[calc(100%+.4rem)] z-30 hidden w-max max-w-72 -translate-x-1/2 rounded-lg bg-primary px-3 py-2 text-left text-xs font-normal leading-relaxed text-on-accent shadow-lg group-hover:block group-focus-visible:block" role="tooltip">
              {hint}
            </span>
          </button>
        )}
      </div>
      {children({
        id,
        invalid: error !== undefined,
        "aria-describedby": describedBy,
      })}
      {/* The hint stays visible when an error appears. They answer different
          questions — "what goes here" and "what went wrong" — and hiding the
          first to show the second means somebody who has just made a mistake
          loses the instruction that would prevent the next one. */}
      {hint !== undefined && hintDisplay === "below" && (
        <span className={`${HELP} text-secondary`} id={hintId}>
          {hint}
        </span>
      )}
      {hint !== undefined && hintDisplay === "tooltip" && (
        <span className="sr-only" id={hintId}>{hint}</span>
      )}
      {error !== undefined && (
        <span className={`${HELP} text-danger`} id={errorId} role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
