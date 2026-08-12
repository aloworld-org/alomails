// A setting that applies itself (ADR 0045). See `Checkbox` for the distinction.
//
// Seven stylesheets declared `.toggle` and they were not all the same object.
// Two — `admin` and `shell/SettingsModal` — drew a switch: a hidden checkbox,
// a rounded track, a knob that slides. The other five styled a row containing
// an ordinary checkbox, or in three cases a `<select>`, under the same name.
// That is the whole reason the word had stopped meaning anything, so it is
// split here and the split is the useful part:
//
//   **A toggle is a setting. A checkbox is one option among several.**
//
// A toggle takes effect the moment it moves and is announced "on" or "off";
// it sits on its own row, usually with a sentence under it saying what turning
// it on does. "Give this person admin access" is a toggle. A box beside a
// search field that narrows a list is a `Checkbox` — see that file.
//
// It stays a native `<input type="checkbox">` with `role="switch"`, for the
// same reason `Select` stays a native select: space-to-toggle, form
// participation, `:checked` and the label association are all already there,
// and `role` is the one thing the platform cannot infer.
//
// What the two copies were missing was never the geometry:
//
//   * **A name.** `admin/UsersPage` names its switch with `title` on the
//     wrapping `<label>` — a tooltip, not a name; the label's text content is
//     empty, so the control is announced as "checkbox, not checked" in a table
//     of twenty identical ones. `admin/UserModal` puts `aria-label` on the
//     `<label>` while the visible text sits in a sibling `<span>` that is bound
//     to nothing. `label` is required here and is really bound.
//   * **The state, said as a state.** Both were plain checkboxes, so "out of
//     office" was read as "checked" rather than "on".
//   * **The hint.** `shell/SettingsModal` renders a hint under every switch
//     and describes none of them; it is on screen and absent from the
//     announcement. `hint` is wired to `aria-describedby`.
import { useId } from "react";

import styles from "./Toggle.module.css";

export interface ToggleProps {
  /** Controlled: this component holds no state of its own. */
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** What the switch turns on, as a sentence a person could act on. Required:
   *  a switch has no value to fall back on and no content of its own, so an
   *  unnamed one is announced as nothing but its role. */
  label: string;
  /** Read but not drawn — for a switch in a table row whose column header
   *  already says what it does. Never a way to skip naming it. */
  hideLabel?: boolean | undefined;
  /** What turning it on actually does, when that is not obvious from the
   *  label. Described, so it is heard as well as seen. */
  hint?: string | undefined;
  /** `row` gives the label the width and puts the switch at the end of it —
   *  a settings list. The default keeps them side by side at their natural
   *  widths. */
  layout?: "inline" | "row" | undefined;
  disabled?: boolean | undefined;
  className?: string | undefined;
}

export function Toggle({
  checked,
  onChange,
  label,
  hideLabel,
  hint,
  layout = "inline",
  disabled,
  className,
}: ToggleProps) {
  const id = useId();
  const hintId = hint === undefined ? undefined : `${id}-hint`;

  const classes = [
    styles.toggle,
    layout === "row" ? styles.row : "",
    disabled === true ? styles.disabled : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <span className={classes}>
      <span className={styles.switch}>
        <input
          type="checkbox"
          // Allowed on a checkbox by ARIA in HTML, and the browser maps the
          // element's checkedness onto it — so the state stays the platform's
          // and only the wording changes.
          role="switch"
          id={id}
          className={styles.input}
          checked={checked}
          disabled={disabled === true}
          {...(hintId === undefined ? {} : { "aria-describedby": hintId })}
          onChange={(e) => onChange(e.target.checked)}
        />
        {/* The knob and the track are the drawing; the input above them is the
            control. Hidden from assistive technology so the switch is
            announced once. */}
        <span className={styles.track} aria-hidden="true" />
      </span>
      <span className={styles.text}>
        <label
          htmlFor={id}
          className={
            hideLabel === true
              ? `${styles.label} ${styles.srOnly}`
              : styles.label
          }
        >
          {label}
        </label>
        {hint !== undefined && (
          <span className={styles.hint} id={hintId}>
            {hint}
          </span>
        )}
      </span>
    </span>
  );
}
