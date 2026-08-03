// Button — the one text button primitive. Variants map to the palette:
// primary (verdigris fill), secondary (copper fill), ghost (quiet), and
// danger. Every button in the app uses this; none restyle a raw <button>.
import type { ButtonHTMLAttributes, ReactNode } from "react";

import styles from "./Button.module.css";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Leading icon (e.g. a lucide icon element). */
  icon?: ReactNode;
  /** Stretch to the container width. */
  block?: boolean;
}

export function Button({
  variant = "primary",
  size = "md",
  icon,
  block = false,
  type = "button",
  className,
  children,
  ...rest
}: ButtonProps) {
  const classes = [
    styles.button,
    styles[variant],
    styles[size],
    block ? styles.block : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button type={type} className={classes} {...rest}>
      {icon !== undefined && <span className={styles.icon}>{icon}</span>}
      {children !== undefined && <span>{children}</span>}
    </button>
  );
}
