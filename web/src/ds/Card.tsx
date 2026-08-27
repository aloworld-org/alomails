// The one surface (ADR 0045).
//
// Twelve stylesheets declared `.card`, agreeing on the idea and on nothing
// else. The variants below are the differences that were decisions; the rest
// were accidents of whoever wrote the screen that week. Their disagreements,
// for the record: padding from `--space-4` to `--space-8` and, in two of them,
// a hardcoded `10px 12px`; radius from md to xl; border subtle or default;
// shadow absent, `--shadow-sm`, `--shadow-md`, or a hand-mixed
// `rgba(40, 30, 20, 0.04)`. `crm` and `hr` were byte-identical, hardcoded
// values included — copied from one to the other, which is the honest way most
// of this happened.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046), so
// the radius and spacing utilities resolve through the shared token layer. The
// build that replaced this file's stylesheet changed no rule.
import type { HTMLAttributes } from "react";

/** The surface itself: the border, the corner and the ground. Elevation and
 *  padding are chosen below, because a variant replaces them rather than
 *  layering over them — two utilities setting one property have no defined
 *  winner, since Tailwind emits them in its own order rather than in the order
 *  they appear in `class`. */
const BASE = "rounded-2xl border border-subtle bg-surface";

/** The differences that were decisions. `sm` for a card in a dense list or a
 *  grid of many; `lg` for a card that is the only thing on the screen — a
 *  sign-in panel, an empty state — where the content should not sit against
 *  the edge. `none` emits no padding utility at all rather than a `p-0` that
 *  would have to out-rank one: see the prop. */
const PAD = { none: "", sm: "p-4", md: "p-6", lg: "p-8" } as const;

/** Only when the whole card is a link or a button. A hover state on something
 *  that does not respond to a click is a promise the screen does not keep.
 *  The focus ring is the accent rather than `global.css`'s neutral one: this
 *  card *is* the control, and it is the only thing on the row that moves. */
const INTERACTIVE =
  "cursor-pointer transition-[border-color,box-shadow] " +
  "duration-[var(--duration-fast)] ease-standard " +
  "hover:border-default hover:shadow-md " +
  "focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2";

export interface CardProps extends HTMLAttributes<HTMLElement> {
  /** The element to draw the surface on. A card is a surface, not a meaning,
   *  and the meaning belongs to the caller: the two-factor screen's card *is*
   *  the sign-in form, `home`'s cards are the page's sections, and `meet`
   *  draws one per list item. Wrapping a `<form>` in a `<div class="card">`
   *  would put the padding and the border on something that is not the thing
   *  you submit, so the element is a prop instead. Defaults to `div`, which is
   *  what a card with nothing else to say should be.
   *
   *  `"button"` is the card that *is* the control — home's four stat tiles,
   *  where the whole surface is the thing you press. It carries `type="button"`
   *  of its own, for the reason `ds/Button` does: a bare `<button>` inside a
   *  form submits it, and a card is not where anybody would look for that.
   *  Pair it with `interactive`, which is the affordance the press deserves.
   *
   *  Added for `auth/TwoFactorScreen` (D2.05) and `home/StatCard` (D2.07b). */
  as?: "div" | "section" | "form" | "li" | "button" | undefined;
  /** `sm` for dense lists, `lg` for a card that is the whole screen.
   *
   *  `none` is the card that lays out its own regions — a head, a body and a
   *  foot, each with padding of its own, where a padding on the surface would
   *  be a second inset inside the first and the head's controls would stop
   *  reaching the corner. It has to be a variant rather than a `className` of
   *  `p-0`, because two utilities setting one property have no defined winner:
   *  Tailwind emits them in its own order, not in the order they appear in
   *  `class`. Added for `insights/TileCard` (D2.08). */
  pad?: "none" | "sm" | "md" | "lg" | undefined;
  /** Drop the shadow, for a card sitting inside another surface: stacking two
   *  elevations reads as a mistake rather than as depth. */
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
    BASE,
    PAD[pad],
    // Elevation is chosen once. `hover:shadow-md` still wins over either, which
    // is the order the stylesheet resolved to: a flat card that is clickable
    // lifts under the pointer.
    flat === true ? "shadow-none" : "shadow-sm",
    interactive === true ? INTERACTIVE : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <Element
      className={classes}
      {...(Element === "button" ? { type: "button" as const } : {})}
      {...rest}
    />
  );
}
