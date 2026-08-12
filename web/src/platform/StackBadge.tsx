// A small, permanent marker in dev builds saying which stack is on screen.
//
// Deliberately not dismissible. The whole value is that it is there at the
// moment somebody has stopped thinking about ports and started debugging a
// "bug" — which is exactly when a dismissed badge would have been dismissed.
//
// Deliberately not translated, and this does not breach the catalog rule: it
// adds no key to `strings`. No user ever sees it, and three translations of a
// developer's diagnostic would be three strings nobody reads and everybody has
// to maintain.
import { API_BASE } from "./runtime";
import { stackLabel } from "./stack";
import styles from "./StackBadge.module.css";

export function StackBadge() {
  // `import.meta.env.DEV` is replaced at build time and the whole component
  // drops out of a production bundle. Belt and braces: `stackLabel` also
  // refuses any non-local origin, so even a mistaken render on a deployed
  // build shows nothing.
  if (!import.meta.env.DEV) return null;
  const label = stackLabel(window.location.origin, API_BASE);
  if (label === null) return null;
  return (
    <div className={styles.badge} title={label.detail} aria-hidden="true">
      {label.text}
    </div>
  );
}
