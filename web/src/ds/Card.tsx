// The one surface (ADR 0045).
//
// Twelve stylesheets declared `.card`, agreeing on the idea and on nothing
// else. The variants below are the differences that were decisions; the rest
// were accidents of whoever wrote the screen that week.
import type { HTMLAttributes } from "react";

import styles from "./Card.module.css";

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  /** `sm` for dense lists, `lg` for a card that is the whole screen. */
  pad?: "sm" | "md" | "lg" | undefined;
  /** Drop the shadow, for a card sitting inside another surface. */
  flat?: boolean | undefined;
  /** Hover and focus affordances. Only when the card really is clickable —
   *  and then it needs a role and a key handler from the caller, or to be
   *  wrapping a link. */
  interactive?: boolean | undefined;
}

export function Card({
  pad = "md",
  flat,
  interactive,
  className,
  ...rest
}: CardProps) {
  const classes = [
    styles.card,
    pad === "sm" ? styles.sm : pad === "lg" ? styles.lg : "",
    flat === true ? styles.flat : "",
    interactive === true ? styles.interactive : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return <div className={classes} {...rest} />;
}
