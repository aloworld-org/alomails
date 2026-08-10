// A dropdown menu: an icon-button trigger and a popover of actions. Closes on
// outside click, Escape, or selecting an item. Used for the reading-pane
// "Move to" and "More" menus; reusable anywhere a small action menu is needed.
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ChevronDown } from "lucide-react";

import { IconButton } from "./IconButton";
import { cx } from "./cx";
import { useDismiss } from "./useDismiss";
import styles from "./Menu.module.css";

export interface MenuItem {
  key: string;
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
  /** Draw a separator line above this item (groups the menu). */
  divider?: boolean;
}

interface MenuProps {
  /** Accessible name for the trigger. */
  label: string;
  /** Trigger icon. */
  icon: ReactNode;
  items: MenuItem[];
  /** Which edge the popover aligns to (default: end/right). */
  align?: "start" | "end";
  /** When set, the trigger is a labelled text button (e.g. "New ▾") instead of
   *  an icon-only button. */
  triggerLabel?: string;
}

export function Menu({
  label,
  icon,
  items,
  align = "end",
  triggerLabel,
}: MenuProps) {
  const [open, setOpen] = useState(false);
  const [placement, setPlacement] = useState<{
    align: "start" | "end";
    up: boolean;
  }>({ align, up: false });
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (!open || ref.current === null) return;
    const rect = ref.current.getBoundingClientRect();
    const menuWidth = 216;
    const resolvedAlign =
      align === "end" && rect.right < menuWidth + 8
        ? "start"
        : align === "start" && window.innerWidth - rect.left < menuWidth + 8
          ? "end"
          : align;
    setPlacement({
      align: resolvedAlign,
      up: window.innerHeight - rect.bottom < 340 && rect.top > 340,
    });
  }, [align, open]);

  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, ref, close);

  return (
    <div className={styles.wrap} ref={ref}>
      {triggerLabel !== undefined ? (
        <button
          type="button"
          className={cx(styles.textTrigger, open && styles.textTriggerOpen)}
          aria-label={label}
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          {icon}
          <span>{triggerLabel}</span>
          <ChevronDown size={15} className={styles.chev} aria-hidden />
        </button>
      ) : (
        <IconButton
          size="sm"
          label={label}
          icon={icon}
          active={open}
          onClick={() => setOpen((v) => !v)}
        />
      )}
      {open && (
        <div
          className={cx(
            styles.menu,
            placement.align === "start" ? styles.start : styles.end,
            placement.up && styles.up,
          )}
          role="menu"
        >
          {items.map((item) => (
            <div key={item.key} className={styles.itemWrap}>
              {item.divider === true && (
                <div className={styles.divider} role="separator" />
              )}
              <button
                type="button"
                role="menuitem"
                className={cx(styles.item, item.danger && styles.danger)}
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  item.onClick();
                }}
              >
                {item.icon !== undefined && (
                  <span className={styles.itemIcon}>{item.icon}</span>
                )}
                <span className={styles.itemLabel}>{item.label}</span>
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
