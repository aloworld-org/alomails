// The one text input (ADR 0045).
//
// Twenty-two stylesheets declared their own `.input` before this existed,
// because CSS Modules scope so completely that nobody could see the other
// twenty-one. Anything this cannot express is a case to add here, not a case
// to hand-roll beside it.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046), so
// `--radius-md` and `rounded-md` are one definition with two spellings. The
// build that replaced this file's stylesheet changed no rule: the reconciled
// decisions below are the ones the CSS carried, and their reasoning came with
// them.
//
// The variants are computed as whole, mutually exclusive strings rather than
// layered one over another. Two utilities that set the same property have no
// defined winner — Tailwind emits them in its own order, not in the order they
// appear in `class` — so `bg-surface` and `bg-transparent` may never both be
// on the element and be expected to resolve. A variant that overrides a base
// value replaces it here instead. (A `variant:` utility does reliably beat its
// own unvariant form, which is why the `focus-visible:` rules can layer.)
import type { InputHTMLAttributes } from "react";

/** What both variants are. `font-[inherit]` keeps the page's typeface rather
 *  than the platform's form font, which is what every copy of this control
 *  wanted and what a bare `<input>` does not do. The focus ring is a real
 *  outline, not a border-colour change: neither implementation this replaces
 *  had one, and a 1px border shift is easy to miss and fails contrast guidance
 *  for a focus indicator. `:focus-visible` keeps it off a mouse click. */
const BASE =
  "w-full rounded-md border font-[inherit] text-primary " +
  "placeholder:text-tertiary " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "focus-visible:outline-2 focus-visible:outline-accent " +
  "disabled:bg-raised disabled:text-tertiary disabled:cursor-not-allowed";

/** A field in a form: a visible border on the surface, which reads as editable
 *  without needing focus. This is the admin console's build; the login
 *  screen's transitions came with it. `h-control` is the 40px the two most
 *  developed copies disagreed about (40 vs 46) — the 46 survives as `lg`,
 *  because a sign-in screen wanting a larger target is a decision, not a
 *  discrepancy. */
const FIELD = "bg-surface focus-visible:outline-offset-1";
const FIELD_MD = "h-control px-3 text-base";
const FIELD_LG = "h-control-lg px-4 text-base";

/** The editor inside a grid cell. Base's cells draw their own box, so the
 *  field's border and surface would be a second box inside the first; what is
 *  left is the type, the padding and the focus ring. The border stays in the
 *  layout as transparent rather than being removed, so the error state still
 *  has something to colour and a focused cell does not change width.
 *
 *  The ring is drawn *inside* the cell, which is the one place the outward
 *  ring is wrong: at the edge of a table it would be clipped by the
 *  neighbouring cell, and it would overlap the cell border on every side. The
 *  row keeps its height either way. `min-h-control` keeps the control a touch
 *  target where the cell gives `h-full` no definite height to resolve against.
 *
 *  A grid packs rows; `text-sm` is the density the Base tables were built at
 *  and is a decision, not a discrepancy with the `text-base` of a form. */
const CELL =
  "h-full min-h-control px-3 text-sm bg-transparent " +
  "focus-visible:-outline-offset-2 focus-visible:bg-surface focus-visible:rounded-sm";

/** Border colour is the one property three things compete for, so it is chosen
 *  once. `invalid` wins over both variants, which is the order the stylesheet
 *  it replaces resolved to. */
function borderColor(variant: "field" | "cell", invalid: boolean): string {
  if (invalid) return "border-danger focus-visible:border-danger";
  if (variant === "cell") return "border-transparent focus-visible:border-transparent";
  return "border-default focus-visible:border-strong";
}

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
    BASE,
    borderColor(variant, invalid === true),
    variant === "cell" ? CELL : FIELD,
    variant === "cell" ? "" : size === "lg" ? FIELD_LG : FIELD_MD,
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
