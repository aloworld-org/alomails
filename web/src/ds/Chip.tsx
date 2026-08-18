// A chip is a thing you can act on (ADR 0045). See `Badge` for the distinction.
//
// There are exactly two ways to act on one, and they cannot be combined:
//
//   * **The chip is the button** (`onClick`) — mail's follow-up date opens a
//     menu of due dates when you press the pill itself.
//   * **The chip holds a button** (`onRemove`) — a recipient you can take out
//     of the To field.
//
// Asking for both would nest a button inside a button. That renders perfectly
// happily and then swallows one of the two clicks, so it is reported rather
// than quietly resolved.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// One rule changed after that restyle, at the D1.55 wave check: a toned chip
// no longer loses its colour under the pointer. See `PRESSABLE_HOVER` below.
import { useEffect, type CSSProperties, type ReactNode } from "react";
import { X } from "lucide-react";

/** The pill. `py-0.5` is the stylesheet's 2px and `px-2` its `--space-2`. */
const BASE =
  "inline-flex items-center gap-1 py-0.5 px-2 rounded-full " +
  "text-sm whitespace-nowrap";

/** Tones, named and matched to `Badge`'s so the same fact reads the same way
 *  whether it is stated or acted on. Mail's follow-up date arrived outlined
 *  rather than filled — border in `--accent`, border in `--danger` — which was
 *  the only chip in the product drawn that way; it is filled here like the
 *  other thirteen. */
const TONE = {
  neutral: "bg-raised text-secondary",
  accent: "bg-accent-tint text-accent",
  danger: "bg-danger-tint text-danger",
} as const;

/** A colour that comes from the chip's value, not from its state: a Base
 *  select field invents its own choices, so no named tone can cover them. The
 *  colour is mixed rather than used raw — a fill at 16% and a label at 70%
 *  against the text colour — which is what keeps a pale choice colour readable
 *  as a label and a dark one readable as a fill.
 *
 *  The mix stays in the class rather than moving to an inline `style`, and the
 *  distinction matters: an inline style would win over the hover rule below,
 *  so a tinted chip that is also a button would stop responding to the pointer.
 *  `--chip-color` is still set per element, so a caller never writes the mix. */
const TINTED =
  "bg-[color-mix(in_srgb,var(--chip-color)_16%,transparent)] " +
  "text-[color-mix(in_srgb,var(--chip-color)_70%,var(--text-primary))]";

/** The chip that is itself a button. The stylesheet's `border: none`,
 *  `cursor: pointer` and `font-family: inherit` are the reset in `global.css`
 *  said twice, so only the weight and the interaction states are left here. */
const PRESSABLE =
  "cursor-pointer font-medium " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "focus-visible:outline-2 focus-visible:outline-accent " +
  "focus-visible:outline-offset-1";

/** The hover fill of a chip that has no tone to lose.
 *
 *  The stylesheet this replaced said, beside the ring below, that "a toned chip
 *  already carries a fill; darkening it would change what the tone says, so the
 *  hover is the ring of the surface under it" — and then never stopped the fill
 *  changing: `.pressable:hover` is a class and a pseudo-class, `.accent` is one
 *  class, so an accent or danger chip took `--border-default` under the pointer
 *  *and* got the ring. Since every pressable chip in the product is toned (mail
 *  reads its follow-up control as neutral, accent or danger by the due date),
 *  the ring was decoration on a chip that had already stopped saying what it
 *  said — an overdue chip went grey the moment you pointed at it.
 *
 *  D1.55 settled it the way the comment always intended: the fill is for the
 *  untoned chip, and a toned one keeps its colour and answers with the ring
 *  alone. */
const PRESSABLE_HOVER = "hover:bg-default";

/** The ring a toned chip gets on hover, in its own ink. `--border-width` is
 *  the token the stylesheet's `inset 0 0 0 var(--border-width)` used. */
const TONED_HOVER_RING =
  "hover:shadow-[inset_0_0_0_var(--border-width)_currentColor]";

/** Room for the button, so the text does not sit against it. */
const REMOVABLE = "pr-1";

/** The remove button. 18px is this drawing's own proportion — small enough to
 *  sit inside the pill, large enough to hit — so it stays a literal. */
const REMOVE =
  "inline-flex items-center justify-center size-[18px] rounded-full " +
  "bg-transparent text-tertiary leading-none cursor-pointer " +
  "hover:bg-sunken hover:text-primary " +
  "focus-visible:outline-2 focus-visible:outline-accent " +
  "focus-visible:outline-offset-1";

export interface ChipProps {
  children: ReactNode;
  /** Makes the chip itself a button. Mutually exclusive with `onRemove`. */
  onClick?: (() => void) | undefined;
  /** Shows the remove button. Its label must name *what* is being removed —
   *  a row of buttons all called "Remove" is useless read aloud. */
  onRemove?: (() => void) | undefined;
  removeLabel?: string | undefined;
  /** Colour carries meaning, so it is a named tone rather than a class the
   *  caller invents — and, as on `Badge`, never the only signal. An overdue
   *  chip says "overdue" in its label as well as in its colour. */
  tone?: "neutral" | "accent" | "danger" | undefined;
  /** A colour derived from the chip's *value* rather than from its state — the
   *  choices of a Base select field, where the colour is what tells one value
   *  apart from another and no fixed palette could know them in advance. Any
   *  CSS colour; the component tints it rather than using it raw, so the label
   *  keeps its contrast whatever is passed. Overrides `tone`, which answers a
   *  different question: use one or the other, never both.
   *
   *  Like `tone`, it is never the only signal — a coloured chip still says
   *  what it is in its label. */
  color?: string | undefined;
  /** Passed through on the button form, for a chip that opens a menu. */
  "aria-haspopup"?: "menu" | "dialog" | "listbox" | undefined;
  "aria-expanded"?: boolean | undefined;
  title?: string | undefined;
  className?: string | undefined;
}

export function Chip({
  children,
  onClick,
  onRemove,
  removeLabel,
  tone = "neutral",
  color,
  className,
  title,
  "aria-haspopup": hasPopup,
  "aria-expanded": expanded,
}: ChipProps) {
  // Dev only: `import.meta.env.DEV` is replaced at build time, so this leaves
  // the production bundle entirely.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    if (onClick !== undefined && onRemove !== undefined) {
      console.error(
        "alo/ds: a <Chip> was given both onClick and onRemove. A chip is" +
          " either a button or a chip with a button in it — nesting them" +
          " swallows one of the two clicks. Split it into two controls.",
      );
    }
  }, [onClick, onRemove]);

  const toned = color !== undefined || tone !== "neutral";
  const classes = [
    BASE,
    color === undefined ? TONE[tone] : TINTED,
    onClick === undefined ? "" : PRESSABLE,
    onClick !== undefined && !toned ? PRESSABLE_HOVER : "",
    onClick !== undefined && toned ? TONED_HOVER_RING : "",
    onRemove === undefined ? "" : REMOVABLE,
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  // The value's colour reaches the class as a custom property, so the mixing
  // that keeps the label readable stays in the component rather than being
  // computed at every call site.
  const tint =
    color === undefined
      ? undefined
      : ({ "--chip-color": color } as CSSProperties);

  if (onClick !== undefined) {
    return (
      <button
        type="button"
        className={classes}
        style={tint}
        onClick={onClick}
        title={title}
        {...(hasPopup === undefined ? {} : { "aria-haspopup": hasPopup })}
        {...(expanded === undefined ? {} : { "aria-expanded": expanded })}
      >
        {children}
      </button>
    );
  }

  return (
    <span className={classes} style={tint} title={title}>
      {children}
      {onRemove !== undefined && (
        <button
          type="button"
          className={REMOVE}
          onClick={onRemove}
          aria-label={removeLabel ?? "Remove"}
        >
          <X size={12} />
        </button>
      )}
    </span>
  );
}
