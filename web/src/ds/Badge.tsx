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
import type { HTMLAttributes } from "react";

import styles from "./Badge.module.css";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "neutral" | "accent" | "danger" | "success" | undefined;
}

export function Badge({ tone = "neutral", className, ...rest }: BadgeProps) {
  const classes = [styles.badge, styles[tone] ?? "", className ?? ""]
    .filter(Boolean)
    .join(" ");
  return <span className={classes} {...rest} />;
}
