// Avatar — a person's mark. Renders initials on a deterministic warm tint
// derived from the name, so the same person is always the same color. Photo
// support is additive later (src prop).
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule.

/** The four sizes, box and type together. These are one drawing's proportions
 *  — an initial has to sit optically centred in a circle at every size, which
 *  is why the type does not step with the box — so they stay literals here
 *  rather than becoming tokens no other component would read (the same call as
 *  `Toggle`'s knob). `text-on-accent` is the `#fff` the stylesheet wrote,
 *  said in the layer that owns it: the tints below are all accent colours. */
const SIZE = {
  sm: "size-[28px] text-[11px]",
  md: "size-[34px] text-[13px]",
  lg: "size-[44px] text-[16px]",
  xl: "size-[64px] text-[22px]",
} as const;

/** `tracking-[0.02em]` opens uppercase initials just enough to stop "MM"
 *  reading as one mark; `shrink-0` keeps the circle a circle in a flex row
 *  that is out of room. */
const BASE =
  "inline-flex items-center justify-center rounded-full " +
  "text-on-accent font-semibold tracking-[0.02em] select-none shrink-0";

interface AvatarProps {
  name: string;
  email?: string | undefined;
  size?: "sm" | "md" | "lg" | "xl";
}

const TINTS = [
  "var(--verdigris-500)",
  "var(--copper-500)",
  "var(--verdigris-700)",
  "var(--copper-600)",
  "var(--verdigris-400)",
];

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

function tintFor(key: string): string {
  let hash = 0;
  for (let i = 0; i < key.length; i += 1) {
    hash = (hash * 31 + key.charCodeAt(i)) | 0;
  }
  return TINTS[Math.abs(hash) % TINTS.length]!;
}

export function Avatar({ name, email, size = "md" }: AvatarProps) {
  return (
    <span
      className={`${BASE} ${SIZE[size]}`}
      // The fill is chosen per person at render, so it is an inline style
      // rather than a class: a utility cannot be generated for a value the
      // build has never seen.
      style={{ background: tintFor(email ?? name) }}
      aria-hidden="true"
    >
      {initials(name)}
    </span>
  );
}
