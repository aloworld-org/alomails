// Spinner — the one loading indicator. Carries an accessible label for screen
// readers.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule, and closed
// the last raw-scale reference in `ds/`: the track was `--warm-300`, which the
// semantic layer had no name for, and now is `--border-track`.
//
// The keyframes are `alo-spin` in `global.css`. A keyframe name is global, and
// `ds/` no longer has a stylesheet to scope one in — the same move `Dialog`'s
// entrance made in D1.52 — so it carries the `alo-` prefix that keeps a
// module's own `@keyframes` from colliding with it. Tailwind's built-in
// `animate-spin` is not used: it turns in 1s and this turns in 0.7s
// (`--animation-spinner`), and matching the built-in would be a restyle.

/** A ring with one lit quarter. `border-2` is the stylesheet's 2px. */
const BASE =
  "inline-block border-2 border-track border-t-accent rounded-full " +
  "animate-[alo-spin_var(--animation-spinner)]";

interface SpinnerProps {
  size?: number;
  label?: string;
}

export function Spinner({ size = 20, label = "Loading" }: SpinnerProps) {
  return (
    <span
      className={BASE}
      // The caller passes a number of pixels, so this is a value the build has
      // never seen and cannot generate a utility for.
      style={{ width: size, height: size }}
      role="status"
      aria-label={label}
    />
  );
}
