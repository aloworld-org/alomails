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

/** The tones, each a whole fill-and-ink pair.
 *
 *  `success` filled with `bg-raised` until D1.55, and not because anybody chose
 *  that: the eight stylesheets this replaced all asked for a tint through a
 *  `var(--success-tint, var(--bg-raised))` fallback, and the token did not
 *  exist, so every one of them drew the fallback. A "Paid" badge and an
 *  "Unknown" badge sat on the same grey and only their ink told them apart —
 *  which is the one signal somebody who cannot distinguish green from grey does
 *  not get. The token exists now, so the tone fills like the other three. */
const TONE = {
  neutral: "bg-raised text-tertiary",
  accent: "bg-accent-tint text-accent",
  danger: "bg-danger-tint text-danger",
  success: "bg-success-tint text-success",
  /* Added for Tasks (D2.11), whose priorities are a three-step scale: folding
   * `medium` onto `danger` — inventory's move for its order states — would
   * have made two different priorities one colour. The ink is `--warning-ink`,
   * not `--warning`: amber is a mark colour and cannot carry text. */
  warning: "bg-warning-tint text-warning-ink",
} as const;

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "neutral" | "accent" | "danger" | "success" | "warning" | undefined;
}

export function Badge({ tone = "neutral", className, ...rest }: BadgeProps) {
  const classes = [BASE, TONE[tone], className ?? ""].filter(Boolean).join(" ");
  return <span className={classes} {...rest} />;
}
