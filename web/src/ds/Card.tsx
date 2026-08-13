// The one surface (ADR 0045).
//
// Twelve stylesheets declared `.card`, agreeing on the idea and on nothing
// else. The variants below are the differences that were decisions; the rest
// were accidents of whoever wrote the screen that week.
import type { HTMLAttributes } from "react";

import styles from "./Card.module.css";

export interface CardProps extends HTMLAttributes<HTMLElement> {
  /** The element to draw the surface on. A card is a surface, not a meaning,
   *  and the meaning belongs to the caller: the two-factor screen's card *is*
   *  the sign-in form, `home`'s cards are the page's sections, and `meet`
   *  draws one per list item. Wrapping a `<form>` in a `<div class="card">`
   *  would put the padding and the border on something that is not the thing
   *  you submit, so the element is a prop instead. Defaults to `div`, which is
   *  what a card with nothing else to say should be.
   *
   *  Added for `auth/TwoFactorScreen` (D2.05). */
  as?: "div" | "section" | "form" | "li" | undefined;
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
  as: Element = "div",
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
  return <Element className={classes} {...rest} />;
}
