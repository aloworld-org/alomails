// A branded date picker: a field-styled trigger plus a calendar popover in the
// app's own palette — our replacement for the raw <input type="date">, whose
// native popup can't be themed. Value is an ISO date (YYYY-MM-DD) or "".
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule; the one
// piece of it that was leaning on source order is the day cell, and it is
// written down on `dayState` below.
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Calendar as CalendarIcon,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";

import { getLocale, strings } from "../i18n";
import { cx } from "./cx";

/** The trigger reads as a field, because that is what it stands in for — and
 *  after the D1.55 wave check it measures like one too. It had been 44px tall
 *  where every other field is 40 (`--control-height`), and drew its focus as a
 *  border in the accent plus a 13% ring of its own, where `ds/Input` draws an
 *  outline: a date field in a form was visibly a different control from the
 *  text field above it. Both are now the field's, so a row of inputs lines up
 *  and one focus treatment covers the whole design system. */
const TRIGGER =
  "flex items-center gap-2 w-full min-w-0 px-3 h-control " +
  "border border-default rounded-md bg-surface text-primary text-left " +
  "transition-colors duration-[var(--duration-fast)] " +
  "ease-standard hover:border-strong " +
  "focus-visible:outline-2 focus-visible:outline-accent " +
  "focus-visible:outline-offset-1 focus-visible:border-strong";

/** The calendar. `w-[264px]` is seven cells and the padding around them — one
 *  drawing's width — and `z-60` is where the stylesheet put it: above the
 *  overlay layer, because a date picker opens inside dialogs that are already
 *  on it. */
const POPOVER =
  "absolute top-full mt-1.5 left-0 z-60 w-[264px] p-3 " +
  "bg-surface border border-default rounded-lg shadow-lg";

/** A day cell. The state below carries the ink, the weight and the hover, all
 *  three of which the stylesheet resolved by source order: `.dayOther`, then
 *  `.dayToday`, then `.daySelected`, each one class, each beating the last by
 *  position. `.daySelected:hover` repeated the selected fill for the same
 *  reason — in utilities a `hover:` would outrank the plain fill instead, so
 *  the selected day simply carries no hover at all. */
const DAY =
  "aspect-square flex items-center justify-center rounded-full " +
  "text-sm tabular-nums " +
  "transition-[background-color,color] duration-[var(--duration-fast)] " +
  "ease-standard";

/** Ink, weight and hover as one exclusive choice, in the order the stylesheet
 *  resolved to. The dimming of a day outside the month is not part of it: it
 *  is an opacity, which nothing below overrode, so a selected day borrowed
 *  from the next month is drawn at 55% in both builds. */
function dayState(selected: boolean, today: boolean, other: boolean): string {
  if (selected) return "bg-accent text-on-accent font-semibold";
  return cx(
    "hover:bg-raised",
    today ? "text-accent font-bold" : other ? "text-tertiary" : "text-primary",
  );
}

/** The two footer actions: a link's ink on a quiet hover. */
const FOOT_BUTTON =
  "text-link text-sm font-medium py-1 px-1.5 rounded-sm hover:bg-raised";

/** The month arrows, on the container so the two buttons stay identical. */
const NAV =
  "flex gap-0.5 [&_button]:p-1 [&_button]:rounded-sm " +
  "[&_button]:text-secondary [&_button]:hover:bg-raised " +
  "[&_button]:hover:text-primary";

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
    <div className="relative" ref={ref}>
      <button
        type="button"
        className={TRIGGER}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        {icon && <CalendarIcon size={16} className="text-tertiary shrink-0" />}
        <span
          className={cx(
            "flex-1 text-sm",
            selected !== null
              ? "overflow-hidden text-ellipsis whitespace-nowrap"
              : "text-tertiary",
          )}
        >
          {selected !== null ? label : placeholder}
        </span>
      </button>

      {open && (
        <div className={POPOVER} role="dialog">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-semibold text-primary capitalize">
              {monthLabel}
            </span>
            <div className={NAV}>
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

          <div className="grid grid-cols-7 mb-1">
            {weekdays.map((w, i) => (
              <span
                key={i}
                // `text-[0.66rem]` is this row's own size: the seven heads have
                // to fit a 36px column in every locale we ship, and no step of
                // the type scale is between `--text-xs` and too small.
                className="text-center text-[0.66rem] font-medium text-tertiary uppercase"
              >
                {w}
              </span>
            ))}
          </div>
          <div className="grid grid-cols-7 gap-0.5">
            {days.map((d, i) => {
              const isOther = d.getMonth() !== month;
              const isToday = sameDay(d, today);
              const isSelected = selected !== null && sameDay(d, selected);
              return (
                <button
                  key={i}
                  type="button"
                  className={cx(
                    DAY,
                    isOther && "opacity-55",
                    dayState(isSelected, isToday, isOther),
                  )}
                  onClick={() => pick(d)}
                >
                  {d.getDate()}
                </button>
              );
            })}
          </div>

          <div className="flex items-center justify-between mt-2 pt-2 border-t border-subtle">
            <button
              type="button"
              className={FOOT_BUTTON}
              onClick={() => {
                onChange("");
                setOpen(false);
              }}
            >
              {strings.datePickerClear}
            </button>
            <button
              type="button"
              className={FOOT_BUTTON}
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
