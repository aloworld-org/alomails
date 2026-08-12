// The follow-up due-date control for a flagged message: a small chip showing
// the due date (red when overdue) or an "Add due date" prompt, opening a menu
// of quick choices plus a date picker. Shown in the reading pane only while the
// conversation is flagged.
import { useEffect, useRef, useState } from "react";
import { CalendarClock } from "lucide-react";

import { strings } from "../../i18n";
import { Chip, Input, cx } from "../../ds";
import styles from "./FlagDueControl.module.css";

interface FlagDueControlProps {
  /** The current due-date (ISO/UTCDate string) or null. */
  due: string | null;
  /** Set (epoch seconds) or clear (null) the due-date. */
  onSet: (dueAt: number | null) => void;
}

/** End of the given local day, as Unix epoch seconds (a due-date is a day, and
 * "due today" means by the end of today). */
function endOfDay(d: Date): number {
  const end = new Date(d.getFullYear(), d.getMonth(), d.getDate(), 23, 59, 0, 0);
  return Math.floor(end.getTime() / 1000);
}

function daysFromNow(n: number): number {
  const d = new Date();
  d.setDate(d.getDate() + n);
  return endOfDay(d);
}

/** A short, human due label: Today / Tomorrow / a localized date. */
function dueLabel(due: Date): string {
  const today = new Date();
  const midnight = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dayDiff = Math.round((midnight(due) - midnight(today)) / 86_400_000);
  if (dayDiff === 0) return strings.flagDueToday.toLowerCase();
  if (dayDiff === 1) return strings.flagDueTomorrow.toLowerCase();
  return due.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function FlagDueControl({ due, onSet }: FlagDueControlProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    function onDown(e: PointerEvent) {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const dueDate = due !== null ? new Date(due) : null;
  const overdue = dueDate !== null && dueDate.getTime() < Date.now();
  const label =
    dueDate === null
      ? strings.flagDueAdd
      : overdue
        ? strings.flagDueOverdue(dueLabel(dueDate))
        : strings.flagDueLabel(dueLabel(dueDate));

  function choose(epoch: number | null) {
    setOpen(false);
    onSet(epoch);
  }

  return (
    <div className={styles.wrap} ref={ref}>
      <Chip
        onClick={() => setOpen((v) => !v)}
        tone={overdue ? "danger" : dueDate !== null ? "accent" : "neutral"}
        title={strings.flagDueSet}
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <CalendarClock size={14} />
        <span>{label}</span>
      </Chip>
      {open && (
        <div className={styles.menu} role="menu">
          <button type="button" className={styles.item} onClick={() => choose(daysFromNow(0))}>
            {strings.flagDueToday}
          </button>
          <button type="button" className={styles.item} onClick={() => choose(daysFromNow(1))}>
            {strings.flagDueTomorrow}
          </button>
          <button type="button" className={styles.item} onClick={() => choose(daysFromNow(7))}>
            {strings.flagDueNextWeek}
          </button>
          <label className={styles.pick}>
            <span>{strings.flagDuePick}</span>
            <Input
              type="date"
              className={styles.date}
              onChange={(e) => {
                const parts = e.target.value.split("-").map(Number);
                if (parts.length !== 3 || parts.some((n) => Number.isNaN(n))) return;
                const [y, m, d] = parts as [number, number, number];
                // Interpret the picked calendar day in local time, due end-of-day.
                choose(endOfDay(new Date(y, m - 1, d)));
              }}
            />
          </label>
          {dueDate !== null && (
            <>
              <div className={styles.divider} />
              <button
                type="button"
                className={cx(styles.item, styles.clear)}
                onClick={() => choose(null)}
              >
                {strings.flagDueClear}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
