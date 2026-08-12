// A chip is a thing you can act on (ADR 0045). See `Badge` for the distinction.
import type { ReactNode } from "react";
import { X } from "lucide-react";

import styles from "./Chip.module.css";

export interface ChipProps {
  children: ReactNode;
  /** Shows the remove button. Its label must name *what* is being removed —
   *  a row of buttons all called "Remove" is useless read aloud. */
  onRemove?: (() => void) | undefined;
  removeLabel?: string | undefined;
  className?: string | undefined;
}

export function Chip({
  children,
  onRemove,
  removeLabel,
  className,
}: ChipProps) {
  const classes = [
    styles.chip,
    onRemove === undefined ? "" : styles.removable,
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <span className={classes}>
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
