// One option among several (ADR 0045). See `Toggle` for the distinction.
//
// Four of the seven stylesheets that declared `.toggle` were not drawing a
// switch at all — they were drawing this: a native checkbox with a word beside
// it, in the filter row above a list. "Include archived", "only mine", "stocked
// only". It applies at once, like a switch, but it is one of a set and it does
// not carry a setting's weight, and calling both of them `.toggle` is how the
// word stopped meaning anything.
//
// The label is a wrapping `<label>`, which is what all four already had and is
// the fewest moving parts that binds. Two call sites did not: `billing/
// ProductDialog` and `shell/SettingsModal` reach for the same class on a
// `<span>`, so their text is beside the box and attached to nothing — clicking
// it does nothing and it is never announced. Here it is not possible to write
// that, because the label is the component's own.
//
// No indeterminate state. Nothing in the repository has a parent box over a
// list of children, and building a third state for no caller is how a
// primitive grows a surface nobody asked for — the same reason `Table` has no
// sorting.
//
// A `Checkbox` brings its own label, so it does not go inside a `Field`, which
// brings another one. A checkbox that needs a hint belongs in a `Field` whose
// label names the question, with the box's own label naming the answer — which
// is what `billing/ProductDialog` does, correctly, today.
import { type ReactNode } from "react";

import styles from "./Checkbox.module.css";

export interface CheckboxProps {
  /** Controlled: this component holds no state of its own. */
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** What ticking it means. Required — a box with nothing beside it is
   *  announced as its role and nothing else. */
  label: ReactNode;
  /** Read but not drawn, for a box in a column the header already names. It is
   *  still in the document, so it is still the accessible name. */
  hideLabel?: boolean | undefined;
  disabled?: boolean | undefined;
  /** The form key, for a checkbox inside a real `<form>`. */
  name?: string | undefined;
  className?: string | undefined;
}

export function Checkbox({
  checked,
  onChange,
  label,
  hideLabel,
  disabled,
  name,
  className,
}: CheckboxProps) {
  const classes = [
    styles.checkbox,
    disabled === true ? styles.disabled : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <label className={classes}>
      <input
        type="checkbox"
        className={styles.input}
        checked={checked}
        disabled={disabled === true}
        {...(name === undefined ? {} : { name })}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className={hideLabel === true ? styles.srOnly : undefined}>
        {label}
      </span>
    </label>
  );
}
