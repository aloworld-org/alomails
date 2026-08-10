// The sidebar mini-month navigator: a compact month grid whose day picks drive
// the main view's anchor, with its own prev/next month arrows.
import { useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

import { getLocale } from "../i18n";
import { addMonths, monthGridDays, sameDay, startOfMonth } from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  anchor: Date;
  today: Date;
  onPick: (day: Date) => void;
}

export function MiniMonth({ anchor, today, onPick }: Props) {
  const locale = getLocale();
  const [shown, setShown] = useState<Date>(startOfMonth(anchor));
  const month = shown.getMonth();
  const days = monthGridDays(shown);
  const title = new Intl.DateTimeFormat(locale, {
    month: "long",
    year: "numeric",
  }).format(shown);
  const wFmt = new Intl.DateTimeFormat(locale, { weekday: "narrow" });
  const header = Array.from({ length: 7 }, (_, i) =>
    wFmt.format(new Date(2024, 0, 1 + i)),
  );

  return (
    <div className={styles.mini}>
      <div className={styles.miniHead}>
        <span>{title}</span>
        <div className={styles.miniNav}>
          <button
            onClick={() => setShown((d) => addMonths(d, -1))}
            aria-label="previous month"
          >
            <ChevronLeft size={15} />
          </button>
          <button
            onClick={() => setShown((d) => addMonths(d, 1))}
            aria-label="next month"
          >
            <ChevronRight size={15} />
          </button>
        </div>
      </div>
      <div className={styles.miniGrid}>
        {header.map((w, i) => (
          <span key={i} className={styles.miniWeekday}>
            {w}
          </span>
        ))}
        {days.map((day) => {
          const cls = [
            styles.miniDay,
            day.getMonth() !== month ? styles.miniOther : "",
            sameDay(day, today) ? styles.miniToday : "",
            sameDay(day, anchor) ? styles.miniSelected : "",
          ]
            .filter(Boolean)
            .join(" ");
          return (
            <button
              key={day.toISOString()}
              className={cls}
              onClick={() => onPick(day)}
            >
              {day.getDate()}
            </button>
          );
        })}
      </div>
    </div>
  );
}
