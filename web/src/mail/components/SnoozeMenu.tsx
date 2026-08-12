// A snooze control: a clock button that opens a menu of preset wake times.
// Picking one calls onPick with the Unix-seconds wake time.
import { useEffect, useRef, useState } from "react";
import { Clock } from "lucide-react";

import { strings } from "../../i18n";
import { IconButton } from "../../ds";
import { snoozePresets } from "../snooze";
import styles from "./SnoozeMenu.module.css";

interface SnoozeMenuProps {
  onPick: (until: number) => void;
  /** Compact icon-only trigger (list/bulk bar) vs a labelled button (toolbar). */
  compact?: boolean;
}

export function SnoozeMenu({ onPick, compact }: SnoozeMenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className={styles.anchor} ref={ref}>
      <IconButton
        size={compact === true ? "sm" : "md"}
        label={strings.snooze}
        icon={<Clock />}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
      />
      {open && (
        <div className={styles.menu} role="menu">
          <div className={styles.heading}>{strings.snoozeUntil}</div>
          {snoozePresets().map((p) => (
            <button
              key={p.key}
              type="button"
              className={styles.item}
              onClick={() => {
                onPick(p.at);
                setOpen(false);
              }}
            >
              <span className={styles.itemLabel}>{p.label}</span>
              <span className={styles.itemWhen}>{p.when}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
