// A badge states a fact (ADR 0045).
//
// The distinction from `Chip`, which the eight and fourteen copies of these
// had thoroughly blurred: **a badge is read, a chip is acted on.** "Admin"
// beside a name is a badge; a recipient you can remove is a chip. If it has a
// button in it, it is a chip.
//
// Colour carries meaning here, so it is a named tone rather than a class the
// caller invents — and never the only signal, since a tone alone is invisible
// to somebody who cannot distinguish it.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule; the eight
// copies' disagreements it had already reconciled — `--radius-full` over
// `--radius-sm`, `--text-xs` over a hardcoded `10px` — are still the ones
// below.
import type { HTMLAttributes } from "react";

/** The pill itself. `py-px` is the stylesheet's 1px: a badge sits inside a
 *  line of text and any more than that pushes the line apart. */
const BASE =
  "inline-flex items-center py-px px-2 rounded-full " +
  "text-xs font-medium whitespace-nowrap";

/** The tones, each a whole fill-and-ink pair. `success` fills with `bg-raised`
 *  because there is no `--success-tint` token: the stylesheet asked for one
 *  through a `var(--success-tint, var(--bg-raised))` fallback that has never
 *  resolved to anything else, so the fallback is written as what it draws.
 *  Adding the missing token would change how every success badge looks, which
 *  is a restyle and not this item's business — it is flagged instead. */
const TONE = {
  neutral: "bg-raised text-tertiary",
  accent: "bg-accent-tint text-accent",
  danger: "bg-danger-tint text-danger",
  success: "bg-raised text-success",
} as const;

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "neutral" | "accent" | "danger" | "success" | undefined;
}

export function Badge({ tone = "neutral", className, ...rest }: BadgeProps) {
  const classes = [BASE, TONE[tone], className ?? ""].filter(Boolean).join(" ");
  return <span className={classes} {...rest} />;
}
