// The one dropdown (ADR 0045).
//
// Seven stylesheets declared `.select`, and four of them wrote it in the same
// rule as their `.input` — the codebase saying, seven times over, that this is
// a text field with a list attached. So it is styled as one: `Input`'s box,
// `Input`'s focus ring, `Input`'s disabled state, `Input`'s two sizes.
//
// It stays a native `<select>`. A hand-built listbox would have to re-implement
// typeahead, Home/End, the mobile picker and the screen-reader model that the
// platform already ships, and the seven copies were not asking for any of it —
// every one was a native select with a border drawn round it. A custom one is a
// large accessibility liability for no gain here.
//
// What the copies were missing is not styling either:
//
//   * **A name.** Two selects in `shell/FiltersSection` — the field and the
//     operator of every filter condition — have no label, no `aria-label` and
//     no wrapping `<label>`. A screen reader announces "combo box, From", which
//     is the current value read as though it were the question. Nothing in a
//     hand-rolled select made that visible, so this component says so out loud
//     in development.
//   * **The empty option.** Six call sites open their list with an option whose
//     value is `""` — "Pick a product", "All locations" — and that option means
//     two different things. Where empty is a real answer it must stay
//     choosable; where it is a prompt on a required field it must not be
//     choosable again, and it must be `value=""` or the browser's own required
//     check passes on the prompt. `placeholder` is that distinction, made once.
import {
  useEffect,
  useRef,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";

import styles from "./Select.module.css";

// `size` is omitted from the native attributes for the same reason as on
// `Input`: HTML's `size` is a row count for a list box, and the name is far
// more useful for the control's height. `htmlSize` is the way to the attribute.
export interface SelectProps extends Omit<
  SelectHTMLAttributes<HTMLSelectElement>,
  "size"
> {
  /** `lg` is the taller control, matching `Input`'s. */
  size?: "md" | "lg" | undefined;
  /** `ghost` drops the box for a select that lives in an editor toolbar beside
   *  icon buttons, where a border round every control reads as a wall. */
  variant?: "outline" | "ghost" | undefined;
  /** Draws the error state and marks the control for assistive technology.
   *  `Field` sets this from its own `error`, so callers rarely pass it. */
  invalid?: boolean | undefined;
  /** Take the width of the column rather than of the longest option. For a
   *  select in a form; a filter in a toolbar wants its natural width. */
  fullWidth?: boolean | undefined;
  /** The option shown before a choice is made, rendered first with `value=""`.
   *  On a `required` select it cannot be chosen again — it is a prompt, and
   *  the browser's own check rejects it. Everywhere else it stays choosable,
   *  because "All locations" and "no product" are answers. */
  placeholder?: string | undefined;
  /** HTML's own `size` attribute, for the rare caller that wants a list. */
  htmlSize?: number | undefined;
  /** The `<option>`s. */
  children?: ReactNode;
}

export function Select({
  size = "md",
  variant = "outline",
  invalid,
  fullWidth,
  placeholder,
  htmlSize,
  className,
  children,
  ...rest
}: SelectProps) {
  const ref = useRef<HTMLSelectElement>(null);

  // A select can be named three legitimate ways — a wrapping `<label>`, a
  // `Field`'s `htmlFor`, or `aria-label` — so the name cannot be a required
  // prop the way `Table`'s is. It can still be checked once the control is on
  // the page, which is the only moment all three are visible at all. Dev only:
  // `import.meta.env.DEV` is replaced at build time, so this whole block leaves
  // the production bundle.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const node = ref.current;
    if (node === null) return;
    if (
      node.labels !== null &&
      node.labels.length === 0 &&
      !node.hasAttribute("aria-label") &&
      !node.hasAttribute("aria-labelledby") &&
      !node.hasAttribute("title")
    ) {
      // The name attribute, when there is one, is a form field key — never a
      // value — so it is safe to say and it is the fastest way to the caller.
      console.error(
        "alo/ds: a <Select> has no accessible name" +
          (node.name === "" ? "" : ` (name="${node.name}")`) +
          ". Wrap it in a <label>, put it in a <Field>, or give it aria-label" +
          " — otherwise it is announced as its own current value.",
      );
    }
  }, []);

  const classes = [
    styles.select,
    size === "lg" ? styles.lg : "",
    variant === "ghost" ? styles.ghost : "",
    fullWidth === true ? styles.fullWidth : "",
    invalid === true ? styles.invalid : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <select
      ref={ref}
      className={classes}
      {...(invalid === true ? { "aria-invalid": true } : {})}
      {...(htmlSize === undefined ? {} : { size: htmlSize })}
      {...rest}
    >
      {placeholder === undefined ? null : (
        // Disabled rather than hidden: a hidden option is treated
        // inconsistently across browsers, and disabling it still lets it show
        // as the current selection while keeping it out of reach afterwards.
        <option value="" disabled={rest.required === true}>
          {placeholder}
        </option>
      )}
      {children}
    </select>
  );
}
