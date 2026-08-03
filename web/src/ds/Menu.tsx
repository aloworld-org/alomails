// A dropdown menu: an icon-button trigger and a popover of actions. Closes on
// outside click, Escape, or selecting an item. Used for the reading-pane
// "Move to" and "More" menus; reusable anywhere a small action menu is needed.
import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

import { IconButton } from "./IconButton";
import { cx } from "./cx";
import styles from "./Menu.module.css";

export interface MenuItem {
  key: string;
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
}

interface MenuProps {
  /** Accessible name for the trigger. */
  label: string;
  /** Trigger icon. */
  icon: ReactNode;
  items: MenuItem[];
  /** Which edge the popover aligns to (default: end/right). */
  align?: "start" | "end";
}

export function Menu({ label, icon, items, align = "end" }: MenuProps) {
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
        label={label}
        icon={icon}
        active={open}
        onClick={() => setOpen((v) => !v)}
      />
      {open && (
        <div className={cx(styles.menu, align === "start" ? styles.start : styles.end)} role="menu">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              role="menuitem"
              className={cx(styles.item, item.danger && styles.danger)}
              disabled={item.disabled}
              onClick={() => {
                setOpen(false);
                item.onClick();
              }}
            >
              {item.icon !== undefined && <span className={styles.itemIcon}>{item.icon}</span>}
              <span className={styles.itemLabel}>{item.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
