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
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The four stylesheets that drew this row nearly agreed: `inline-flex`,
// centred, `--text-sm`, `--text-secondary`, and a gap of 6px (crm, hr), 8px
// (inventory) or `--space-2` (billing), which is 8px. crm and hr were
// byte-identical again. So the row is settled; what none of the four had was
// any rule at all for the box itself.
//
// That is the one visible change. Left unstyled, the box is whatever the
// platform draws — on Chrome that is a blue tick, which makes the filter row
// above a list the one place in the product where the accent colour is not
// ours. `accent-color` fixes it in one declaration without replacing the
// control: the native checkbox keeps its own focus behaviour, its
// indeterminate drawing, and the size the platform thinks a touch target
// should be.
//
// Dropped as a caller's concern rather than the control's: billing's
// `margin-right: auto`, which pushed the rest of a toolbar to the right —
// `ToolbarSpacer` is that, and it says so. Kept: `whitespace-nowrap` (billing
// and inventory), so a two-word label breaks between controls now that
// `Toolbar` wraps, and never down the middle of itself.
import { type ReactNode } from "react";

/** The row. Its disabled treatment reads the control's own state
 *  (`has-[:disabled]`) rather than the prop, so the two can never disagree —
 *  none of the four had a disabled state at all, and a box you cannot tick
 *  should look unlike one you can and still be readable: `--text-tertiary` is
 *  4.8:1 on the surface. */
const ROW =
  "inline-flex items-center gap-2 text-sm text-secondary whitespace-nowrap cursor-pointer " +
  "has-[:disabled]:text-tertiary has-[:disabled]:cursor-not-allowed";

/** The box. 16px is the size all four rows were built around. */
const BOX =
  "flex-none w-4 h-4 m-0 accent-accent cursor-pointer disabled:cursor-not-allowed " +
  "focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2";

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
  const classes = [ROW, className ?? ""].filter(Boolean).join(" ");

  return (
    <label className={classes}>
      <input
        type="checkbox"
        className={BOX}
        checked={checked}
        disabled={disabled === true}
        {...(name === undefined ? {} : { name })}
        onChange={(e) => onChange(e.target.checked)}
      />
      {/* Read, not drawn — a checkbox in a table's select column, where the row
          is the label. The name still has to exist. */}
      <span className={hideLabel === true ? "sr-only" : undefined}>
        {label}
      </span>
    </label>
  );
}
