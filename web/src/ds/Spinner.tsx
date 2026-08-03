// Spinner — the one loading indicator. Inherits color via currentColor so it
// works on any surface. Carries an accessible label for screen readers.
import styles from "./Spinner.module.css";

interface SpinnerProps {
  size?: number;
  label?: string;
}

export function Spinner({ size = 20, label = "Loading" }: SpinnerProps) {
  return (
    <span
      className={styles.spinner}
      style={{ width: size, height: size }}
      role="status"
      aria-label={label}
    />
  );
}
