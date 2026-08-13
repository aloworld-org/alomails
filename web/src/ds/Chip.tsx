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
import { useEffect, type CSSProperties, type ReactNode } from "react";
import { X } from "lucide-react";

import styles from "./Chip.module.css";

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

  const classes = [
    styles.chip,
    color === undefined ? (styles[tone] ?? "") : styles.tinted,
    onClick === undefined ? "" : styles.pressable,
    onRemove === undefined ? "" : styles.removable,
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  // The value's colour reaches the stylesheet as a custom property, so the
  // mixing that keeps the label readable stays in CSS with the rest of the
  // chip rather than being computed at every call site.
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
          className={styles.remove}
          onClick={onRemove}
          aria-label={removeLabel ?? "Remove"}
        >
          <X size={12} />
        </button>
      )}
    </span>
  );
}
