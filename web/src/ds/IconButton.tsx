// IconButton — an icon-only button with a mandatory accessible label.
// `tone="rail"` renders for the dark left rail; `active` marks the current
// module. Used by the rail and every toolbar.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule, which for
// this one meant reading two pieces of cascade out of the source order and
// writing them down:
//
//   * **`tone="rail"` sets its own size.** `.rail` declared a 44px box and a
//     larger radius, and it beat `.md` and `.sm` only by being written after
//     them — equal specificity, resolved by position, which Tailwind does not
//     have. So a tone brings its whole geometry and the two are chosen
//     together rather than layered.
//   * **`active` suppresses the hover.** `.default.active` and `.default:hover`
//     are both two classes, and again the later one won; in utilities the
//     `hover:` variant would win instead, so an active toolbar button would
//     have flashed back to the plain hover fill under the pointer. The active
//     string therefore carries no hover of its own and the resting string is
//     the only one that does.
import type { ButtonHTMLAttributes, ReactNode } from "react";

/** `size-[var(--icon-size-control)]` on the glyph, not on the box: the box is
 *  the touch target and the icon is what you see, and the two are separate
 *  decisions of the system. `sm` and `md` were already the same 40px box — the
 *  difference the names promise was never drawn — and that is preserved here
 *  rather than invented. */
const BASE =
  "inline-flex items-center justify-center rounded-md " +
  "transition-[background-color,color] duration-[var(--duration-fast)] " +
  "ease-standard [&_svg]:size-[var(--icon-size-control)]";

/** The box each tone draws, and the two states it draws it in. `rest` carries
 *  the hover; `active` does not, because a control that is already on does not
 *  change under the pointer (see the header). */
const TONE = {
  default: {
    box: "size-control",
    rest: "text-secondary hover:bg-raised hover:text-primary",
    active: "bg-accent-tint text-accent-active",
  },
  // The rail is dark, so its quiet colour is a different token rather than a
  // dimmer of the same one, and its box is larger: 44px is a comfortable
  // target on the one bar that is always on screen.
  rail: {
    box: "size-[44px] rounded-lg",
    rest: "text-on-rail-muted hover:bg-rail-hover hover:text-on-rail",
    active: "bg-rail-hover text-on-rail",
  },
  // A destructive action — delete, remove. `default`'s geometry with the
  // danger ink on its tint under the pointer, so the button says what it will
  // do before it is pressed. Added for the task detail's delete (D2.11b),
  // which drew this by hand; `Button` already had the variant.
  danger: {
    box: "size-control",
    rest: "text-secondary hover:bg-danger-tint hover:text-danger",
    active: "bg-danger-tint text-danger",
  },
} as const;

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Accessible name — required; an icon alone is not a label. */
  label: string;
  icon: ReactNode;
  tone?: "default" | "rail" | "danger";
  size?: "sm" | "md";
  /** For a button that is genuinely on or off — a flag, a pinned pane. Omit it
   *  and the button is an action, not a toggle: `aria-pressed` used to be set
   *  on every icon button this rendered, so the twelve tools of mail's
   *  formatting bar were each announced as "not pressed" by a control that
   *  tracks no state at all. A toggle that never toggles is worse than silence. */
  active?: boolean;
}

export function IconButton({
  label,
  icon,
  tone = "default",
  size = "md",
  active,
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  // `size` is accepted and drawn the same way at both values, exactly as the
  // stylesheet did. It stays in the API because callers pass it and because
  // the two names are a decision waiting to be made, not one already unmade.
  void size;
  const style = TONE[tone];
  const classes = [
    BASE,
    style.box,
    active === true ? style.active : style.rest,
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
      {...(active === undefined ? {} : { "aria-pressed": active })}
      {...rest}
    >
      {icon}
    </button>
  );
}
