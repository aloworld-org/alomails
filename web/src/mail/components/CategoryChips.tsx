// Read-only display of a message's categories: compact colored dots in the
// list, named pills in the reading pane. A category with no color falls back
// to a neutral ring so it is still visible.
import type { Category } from "../../jmap";
import { cx } from "../../ds";
import styles from "./CategoryChips.module.css";

interface CategoryChipsProps {
  categories: Category[];
  /** "dots" (list rows) or "pills" (reading pane). */
  variant?: "dots" | "pills";
}

export function CategoryChips({ categories, variant = "pills" }: CategoryChipsProps) {
  if (categories.length === 0) return null;
  if (variant === "dots") {
    return (
      <span className={styles.dots}>
        {categories.map((c) => (
          <span
            key={c.id}
            className={styles.dot}
            style={c.color !== null ? { background: c.color } : undefined}
            title={c.name}
            aria-label={c.name}
          />
        ))}
      </span>
    );
  }
  return (
    <span className={styles.pills}>
      {categories.map((c) => (
        <span
          key={c.id}
          className={cx(styles.pill, c.color === null && styles.pillNeutral)}
          style={c.color !== null ? { background: tint(c.color), color: c.color, borderColor: c.color } : undefined}
        >
          <span
            className={styles.pillDot}
            style={c.color !== null ? { background: c.color } : undefined}
            aria-hidden
          />
          {c.name}
        </span>
      ))}
    </span>
  );
}

/** A translucent wash of the category color for the pill background. Appends an
 * alpha byte to a "#rrggbb" hex; non-hex input is left to the neutral style. */
function tint(hex: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(hex) ? `${hex}22` : hex;
}
