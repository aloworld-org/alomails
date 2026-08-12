// IconButton — an icon-only button with a mandatory accessible label.
// `tone="rail"` renders for the dark left rail; `active` marks the current
// module. Used by the rail and every toolbar.
import type { ButtonHTMLAttributes, ReactNode } from "react";

import styles from "./IconButton.module.css";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Accessible name — required; an icon alone is not a label. */
  label: string;
  icon: ReactNode;
  tone?: "default" | "rail";
  size?: "sm" | "md";
  /** For a button that is genuinely on or off — a flag, a pinned pane. Omit it
   *  and the button is an action, not a toggle: `aria-pressed` used to be set
   *  on every icon button this rendered, so the twelve tools of mail's
   *  formatting bar were each announced as "not pressed" by a control that
   *  tracks no state at all. A toggle that never toggles is worse than silence. */
  active?: boolean;
}

export function IconButton({
  label,
  icon,
  tone = "default",
  size = "md",
  active,
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  const classes = [
    styles.iconButton,
    styles[tone],
    styles[size],
    active === true ? styles.active : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button
      type={type}
      className={classes}
      aria-label={label}
      title={label}
      {...(active === undefined ? {} : { "aria-pressed": active })}
      {...rest}
    >
      {icon}
    </button>
  );
}
