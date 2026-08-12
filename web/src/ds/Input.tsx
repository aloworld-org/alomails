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
  /** Draws the error state and marks the control for assistive technology.
   *  `Field` sets this from its own `error`, so callers rarely pass it. */
  invalid?: boolean | undefined;
  /** HTML's own `size` attribute, for the rare caller that wants it. */
  htmlSize?: number | undefined;
}

export function Input({
  size = "md",
  invalid,
  htmlSize,
  className,
  ...rest
}: InputProps) {
  const classes = [
    styles.input,
    size === "lg" ? styles.lg : "",
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
