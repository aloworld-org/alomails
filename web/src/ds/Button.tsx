// Button — the one text button primitive. Variants map to the palette:
// primary (brand-orange fill), secondary (deep-orange fill), ghost (quiet),
// and danger. Every button in the app uses this; none restyle a raw <button>.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046), so
// `--radius-md` and `rounded-md` are one definition with two spellings. The
// build that replaced this file's stylesheet changed no rule.
//
// The variants are whole, mutually exclusive strings rather than layers. Two
// utilities that set one property have no defined winner — Tailwind emits them
// in its own order, not in the order they appear in `class` — and this file's
// stylesheet leaned on source order twice: `.primary:disabled` reset the
// `opacity: 0.5` that `.button:disabled` had just set, and every variant's
// hover overrode the variant's own fill. The first is resolved by each variant
// carrying its own disabled treatment; the second needs no help, because a
// `hover:` utility does reliably beat its own unvariant form.
import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md";

/** What every button is. `leading-none` and `whitespace-nowrap` keep the label
 *  on one line inside a fixed height; the transition covers the four
 *  properties a variant can move. `text-decoration` is not repeated here — the
 *  reset in `global.css` already strips it from `button` in every state, and a
 *  rule that is a duplicate of the reset only looks like a decision. */
const BASE =
  "inline-flex shrink-0 items-center justify-center gap-2 rounded-md " +
  "font-ui font-medium leading-none whitespace-nowrap " +
  "transition-all " +
  "duration-[var(--duration-fast)] ease-standard " +
  "disabled:cursor-not-allowed";

/** The two heights, from `--button-height-*`: they are the button's own scale
 *  rather than steps on the 4px spacing scale, and naming them is what keeps
 *  the 38 of a button and the 40 of a field visibly a decision. Padding is
 *  important because `global.css` follows the Tailwind import and its native
 *  button reset would otherwise win the cascade and flatten it to zero. */
const SIZE = {
  sm: "min-h-10 !px-[var(--space-5)] !py-[var(--space-2)] text-sm",
  md: "min-h-10 !px-[var(--space-6)] !py-[0.625rem] text-base",
} as const;

/** A faded accent reads as a broken button rather than an unavailable one, so
 *  a disabled primary or danger becomes a clean neutral instead of dimming.
 *  Carried by the two variants that fill with a signal colour; the other two
 *  dim, which is what the stylesheet did. */
const DISABLED_NEUTRAL = "disabled:!bg-default disabled:!text-tertiary";
const DISABLED_DIM = "disabled:opacity-50";

const VARIANT = {
  // Primary is the universal call-to-action treatment. `--accent` is alo's
  // brand orange (#e76f51); screens must not replace it with a local fill.
  primary:
    "!h-10 !rounded-lg !px-4 !text-sm !bg-accent !text-on-accent shadow-sm " +
    "enabled:hover:!bg-accent-hover enabled:active:!bg-accent-active " +
    "enabled:active:scale-[0.98] focus-visible:outline-none " +
    "focus-visible:ring-4 focus-visible:ring-accent/15 " +
    "disabled:pointer-events-none disabled:opacity-50",
  secondary:
    "bg-accent-secondary text-on-accent " +
    "enabled:hover:bg-accent-secondary-hover " +
    DISABLED_DIM,
  ghost:
    "bg-transparent text-primary border border-default " +
    "enabled:hover:bg-raised enabled:hover:border-strong " +
    DISABLED_DIM,
  // `brightness-94` is Tailwind's percentage form of the `brightness(0.94)`
  // the stylesheet wrote: the danger red has no darker token, and mixing one
  // here would be a new colour rather than the same rule spelled differently.
  danger:
    "bg-danger text-on-accent enabled:hover:brightness-94 " + DISABLED_NEUTRAL,
} as const;

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
    BASE,
    VARIANT[variant],
    SIZE[size],
    block ? "w-full" : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button type={type} className={classes} {...rest}>
      {icon !== undefined && (
        // Primary CTA icons are exactly 16px; keeping the icon slot here makes
        // that contract consistent across every module.
        <span className="inline-flex items-center [&_svg]:size-4">
          {icon}
        </span>
      )}
      {children !== undefined && <span>{children}</span>}
    </button>
  );
}
