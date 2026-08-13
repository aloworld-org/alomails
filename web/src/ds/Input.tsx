// The one text input (ADR 0045).
//
// Twenty-two stylesheets declared their own `.input` before this existed,
// because CSS Modules scope so completely that nobody could see the other
// twenty-one. Anything this cannot express is a case to add here, not a case
// to hand-roll beside it.
import type { InputHTMLAttributes } from "react";

import styles from "./Input.module.css";

// `size` is omitted from the native attributes on purpose: HTML's `size` is a
// character count almost nobody wants, and the name is far more useful for the
// control's height. A caller who genuinely needs the HTML attribute can set it
// through `htmlSize`.
export interface InputProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "size"
> {
  /** `lg` is the taller control the sign-in screens use. */
  size?: "md" | "lg" | undefined;
  /** `cell` is the editor *inside* a grid cell — Base's tables, where the cell
   *  already draws the box and a second border inside it reads as a field
   *  floating in a field. It fills its container, drops the border and the
   *  background, and keeps the focus ring inside the cell's own edge so a
   *  focused cell does not grow by two pixels and shift the row. Added for
   *  `drive/BaseCell` (D2.04), which was the only `.input` of the twenty-two
   *  that lived in a cell rather than in a form. */
  variant?: "field" | "cell" | undefined;
  /** Draws the error state and marks the control for assistive technology.
   *  `Field` sets this from its own `error`, so callers rarely pass it. */
  invalid?: boolean | undefined;
  /** HTML's own `size` attribute, for the rare caller that wants it. */
  htmlSize?: number | undefined;
}

export function Input({
  size = "md",
  variant = "field",
  invalid,
  htmlSize,
  className,
  ...rest
}: InputProps) {
  const classes = [
    styles.input,
    size === "lg" ? styles.lg : "",
    variant === "cell" ? styles.cell : "",
    invalid === true ? styles.invalid : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <input
      className={classes}
      {...(invalid === true ? { "aria-invalid": true } : {})}
      {...(htmlSize === undefined ? {} : { size: htmlSize })}
      {...rest}
    />
  );
}
