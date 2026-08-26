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
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The seven copies agreed on nearly everything that matters — a 1px
// `--border-default` on `--bg-surface`, `--radius-md`, `--text-primary`,
// `font-family: inherit` — which is what makes the disagreements worth naming.
//
// The base is `Input`'s, not the majority's. Four of the seven wrote
// `.input, .select, .textarea` in one rule, which is the codebase saying out
// loud that these two controls stand side by side in a form row and must be
// the same height; finance and projects were byte-identical doing it. So the
// select follows `Input` (`h-control`, `text-base`) rather than the ~33px each
// module had derived from padding. The toolbar copies (billing 6px 8px,
// inventory 6px 10px, shell's 34px) sit beside a search field that is making
// the same move, so the row stays level.
//
// Reconciled rather than offered as options:
//
//   * **One focus treatment.** There were four: an inset `outline` (finance,
//     projects, inventory), `outline: none` traded for a `--focus-ring` shadow
//     (billing), and none at all (both shell sections, left to the UA
//     default). This is `Input`'s ring, because a select is not a different
//     kind of field.
//   * **One disabled state.** shell dimmed the whole control to
//     `opacity: 0.55`, which takes its text below the contrast floor — a
//     control you cannot use still has to be readable. `Input`'s build says it
//     with surface and cursor.
//   * **Room for the arrow.** Every copy padded both sides equally, so the
//     platform's chevron sits on top of a long option's last characters.
//     Nothing here sets `appearance: none`: the native control is the whole
//     reason a select is a select on a phone, and drawing our own chevron
//     would mean taking the platform's away on every device to fix it on one.
import {
  useEffect,
  useRef,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";

/** What every select is. `max-w-full` is never wider than what holds it — only
 *  billing thought of this, and it is the difference between a long product
 *  name and a broken toolbar. As on `Input`, utilities that set the same
 *  property are chosen exclusively below rather than layered: Tailwind emits
 *  them in its own order, not the order they are written in. */
const BASE =
  "max-w-full rounded-md border font-[inherit] text-base text-primary cursor-pointer " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "focus:outline-none focus-visible:outline-none " +
  "disabled:text-tertiary disabled:cursor-not-allowed";

/** A select in a form: `Input`'s box, down to the same height, so a row of the
 *  two does not step. */
const OUTLINE = "bg-surface disabled:bg-raised";

/** mail's formatting bar: a dozen controls in a row, where a box around each
 *  one would read as a wall rather than a set. Same hover surface as
 *  `IconButton`, its neighbour in that bar. A focused ghost still draws its
 *  border, so the control the keyboard is on is never only an outline. */
const GHOST = "bg-transparent hover:enabled:bg-raised";

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

  const ghost = variant === "ghost";
  const classes = [
    BASE,
    ghost ? GHOST : OUTLINE,
    // Border colour, chosen once: an error outranks both variants, which is the
    // order the stylesheet this replaces resolved to.
    invalid === true
      ? "border-danger focus-visible:border-danger"
      : ghost
        ? "border-transparent focus:border-accent focus-visible:border-accent"
        : "border-default focus:border-accent focus-visible:border-accent",
    size === "lg" ? "h-control-lg" : "h-control",
    // Room for the platform's chevron on the trailing side; a ghost sits
    // tighter because it has no box to sit inside.
    ghost ? "pl-2 pr-6" : size === "lg" ? "pl-4 pr-8" : "pl-3 pr-8",
    // Take the width of the column rather than of the longest option. For a
    // select in a form; a filter in a toolbar wants its natural width, because
    // unlike a text input a select's natural width carries information.
    fullWidth === true ? "w-full" : "",
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
