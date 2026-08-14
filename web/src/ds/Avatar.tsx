// Avatar — a person's mark. Renders initials on a deterministic warm tint
// derived from the name, so the same person is always the same color. Photo
// support is additive later (src prop).
import styles from "./Avatar.module.css";

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
      className={`${styles.avatar} ${styles[size]}`}
      style={{ background: tintFor(email ?? name) }}
      aria-hidden="true"
    >
      {initials(name)}
    </span>
  );
}
