// A labelled control, with its hint and its error (ADR 0045).
//
// Seventeen stylesheets built this before it existed. The layout was never the
// hard part — they all reached the same column — but the wiring was: a label
// that is not bound to its control is invisible to a screen reader, and an
// error that is not announced is invisible to everyone not looking at it.
// Doing that once, correctly, is the whole point of the component.
import { useId, type ReactNode } from "react";

import styles from "./Field.module.css";

export interface FieldProps {
  label: string;
  /** What goes in the box. Stays visible when an error is shown. */
  hint?: string | undefined;
  /** What went wrong. Announced, and it marks the control invalid. */
  error?: string | undefined;
  /** Receives the id and invalid state to spread onto the control. */
  children: (control: {
    id: string;
    invalid: boolean;
    "aria-describedby": string | undefined;
  }) => ReactNode;
}

export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const hintId = hint === undefined ? undefined : `${id}-hint`;
  const errorId = error === undefined ? undefined : `${id}-error`;
  // Both, when both are present: the instruction and the failure are read out
  // together, in that order.
  const describedBy = [hintId, errorId].filter(Boolean).join(" ") || undefined;

  return (
    <div className={styles.field}>
      <label className={styles.label} htmlFor={id}>
        {label}
      </label>
      {children({
        id,
        invalid: error !== undefined,
        "aria-describedby": describedBy,
      })}
      {hint !== undefined && (
        <span className={styles.hint} id={hintId}>
          {hint}
        </span>
      )}
      {error !== undefined && (
        <span className={styles.error} id={errorId} role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
