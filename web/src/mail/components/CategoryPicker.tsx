// A popover to tag/untag the open conversation with categories. Stays open
// across toggles (unlike the action menus) so several can be set at once. The
// catalog is managed in the sidebar; this only applies existing categories.
import { useEffect, useRef, useState } from "react";
import { Check, Tags } from "lucide-react";

import type { Category } from "../../jmap";
import { IconButton, cx } from "../../ds";
import { strings } from "../../i18n";
import styles from "./CategoryPicker.module.css";

interface CategoryPickerProps {
  categories: Category[];
  /** Ids currently applied to the conversation. */
  activeIds: ReadonlySet<string>;
  onToggle: (categoryId: string, on: boolean) => void;
}

export function CategoryPicker({ categories, activeIds, onToggle }: CategoryPickerProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    function onPointerDown(e: PointerEvent) {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className={styles.wrap} ref={ref}>
      <IconButton
        size="sm"
        label={strings.categorize}
        icon={<Tags />}
        active={open}
        onClick={() => setOpen((v) => !v)}
      />
      {open && (
        <div className={styles.menu} role="menu" aria-label={strings.categorize}>
          {categories.length === 0 ? (
            <p className={styles.empty}>{strings.categoryNoneHint}</p>
          ) : (
            categories.map((c) => {
              const on = activeIds.has(c.id);
              return (
                <button
                  key={c.id}
                  type="button"
                  role="menuitemcheckbox"
                  aria-checked={on}
                  className={cx(styles.item, "hover:!bg-accent-soft hover:!text-accent", on && styles.itemOn)}
                  onClick={() => onToggle(c.id, !on)}
                >
                  <span
                    className={styles.dot}
                    style={c.color !== null ? { background: c.color } : undefined}
                    aria-hidden
                  />
                  <span className={styles.name}>{c.name}</span>
                  {on && <Check size={14} className={styles.check} />}
                </button>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
