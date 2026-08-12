// A branded date picker: a field-styled trigger plus a calendar popover in the
// app's own palette — our replacement for the raw <input type="date">, whose
// native popup can't be themed. Value is an ISO date (YYYY-MM-DD) or "".
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Calendar as CalendarIcon,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";

import { getLocale, strings } from "../i18n";
import styles from "./DatePicker.module.css";

interface Props {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Show a leading calendar icon in the trigger (default true). */
  icon?: boolean;
}

function ymd(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** The 6×7 Monday-first grid of Dates covering `anchor`'s month. */
function monthGrid(anchor: Date): Date[] {
  const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  const start = new Date(first);
  start.setDate(1 - ((first.getDay() + 6) % 7)); // back to Monday
  return Array.from({ length: 42 }, (_, i) => {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    return d;
  });
}

export function DatePicker({
  value,
  onChange,
  placeholder,
  icon = true,
}: Props) {
  const locale = getLocale();
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<Date>(
    value !== "" ? new Date(`${value}T00:00`) : new Date(),
  );
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    function down(e: PointerEvent) {
      if (ref.current !== null && !ref.current.contains(e.target as Node))
        setOpen(false);
    }
    function key(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  const today = useMemo(() => new Date(), []);
  const selected = value !== "" ? new Date(`${value}T00:00`) : null;
  const days = monthGrid(anchor);
  const month = anchor.getMonth();

  const weekdays = useMemo(() => {
    const fmt = new Intl.DateTimeFormat(locale, { weekday: "short" });
    // Monday-first (2024-01-01 is a Monday).
    return Array.from({ length: 7 }, (_, i) =>
      fmt.format(new Date(2024, 0, 1 + i)),
    );
  }, [locale]);

  const label =
    selected !== null
      ? new Intl.DateTimeFormat(locale, {
          weekday: "short",
          day: "numeric",
          month: "short",
          year: "numeric",
        }).format(selected)
      : (placeholder ?? "");
  const monthLabel = new Intl.DateTimeFormat(locale, {
    month: "long",
    year: "numeric",
  }).format(anchor);

  function pick(d: Date) {
    onChange(ymd(d));
    setOpen(false);
  }

  return (
    <div className={styles.wrap} ref={ref}>
      <button
        type="button"
        className={styles.trigger}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        {icon && <CalendarIcon size={16} className={styles.triggerIcon} />}
        <span className={selected !== null ? styles.value : styles.placeholder}>
          {selected !== null ? label : placeholder}
        </span>
      </button>

      {open && (
        <div className={styles.popover} role="dialog">
          <div className={styles.head}>
            <span className={styles.monthLabel}>{monthLabel}</span>
            <div className={styles.nav}>
              <button
                type="button"
                onClick={() =>
                  setAnchor(
                    new Date(anchor.getFullYear(), anchor.getMonth() - 1, 1),
                  )
                }
                aria-label={strings.agendaPrev}
              >
                <ChevronLeft size={16} />
              </button>
              <button
                type="button"
                onClick={() =>
                  setAnchor(
                    new Date(anchor.getFullYear(), anchor.getMonth() + 1, 1),
                  )
                }
                aria-label={strings.agendaNext}
              >
                <ChevronRight size={16} />
              </button>
            </div>
          </div>

          <div className={styles.weekdays}>
            {weekdays.map((w, i) => (
              <span key={i}>{w}</span>
            ))}
          </div>
          <div className={styles.grid}>
            {days.map((d, i) => {
              const isOther = d.getMonth() !== month;
              const isToday = sameDay(d, today);
              const isSelected = selected !== null && sameDay(d, selected);
              return (
                <button
                  key={i}
                  type="button"
                  className={`${styles.day} ${isOther ? styles.dayOther : ""} ${isToday ? styles.dayToday : ""} ${isSelected ? styles.daySelected : ""}`}
                  onClick={() => pick(d)}
                >
                  {d.getDate()}
                </button>
              );
            })}
          </div>

          <div className={styles.foot}>
            <button
              type="button"
              className={styles.footBtn}
              onClick={() => {
                onChange("");
                setOpen(false);
              }}
            >
              {strings.datePickerClear}
            </button>
            <button
              type="button"
              className={styles.footBtn}
              onClick={() => {
                setAnchor(new Date());
                pick(new Date());
              }}
            >
              {strings.datePickerToday}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
