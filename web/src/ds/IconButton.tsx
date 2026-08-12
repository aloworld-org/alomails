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
  active?: boolean;
}

export function IconButton({
  label,
  icon,
  tone = "default",
  size = "md",
  active = false,
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  const classes = [
    styles.iconButton,
    styles[tone],
    styles[size],
    active ? styles.active : "",
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
      aria-pressed={active}
      {...rest}
    >
      {icon}
    </button>
  );
}
