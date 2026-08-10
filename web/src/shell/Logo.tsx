// The alo mark: a terracotta waving hand — the greeting at the heart of the
// brand — beside the lowercase "alo" wordmark. The hand is always terracotta
// (the one color that means "alo"); the wordmark is navy on light grounds and
// light on the dark rail / brand panel. `withWordmark` shows the name (used on
// the login screen and the rail header).
import { Hand } from "lucide-react";

import styles from "./Logo.module.css";

interface LogoProps {
  size?: number;
  withWordmark?: boolean;
  /** Light wordmark for placement on the dark brand panel / rail. */
  onDark?: boolean;
}

export function Logo({
  size = 32,
  withWordmark = false,
  onDark = false,
}: LogoProps) {
  return (
    <span className={styles.logo}>
      <Hand
        size={size}
        className={styles.mark}
        strokeWidth={2}
        aria-label="alo"
      />
      {withWordmark && (
        <span
          className={
            onDark ? `${styles.wordmark} ${styles.onDark}` : styles.wordmark
          }
        >
          alo
        </span>
      )}
    </span>
  );
}
