// Local-time date helpers for the calendar grid. Events cross the wire as UTC
// RFC 3339 instants; the UI works entirely in the viewer's local time and
// converts at the edges (parse on read, `toISOString` on write). The week
// starts Monday (European default).

export const HOURS_IN_DAY = 24;

export function startOfDay(d: Date): Date {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  return x;
}

export function addDays(d: Date, n: number): Date {
  const x = new Date(d);
  x.setDate(x.getDate() + n);
  return x;
}

export function addMonths(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth() + n, 1);
}

export function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** Monday-based start of the week containing `d`. */
export function startOfWeek(d: Date): Date {
  const x = startOfDay(d);
  const mondayIndex = (x.getDay() + 6) % 7; // Sun=0 → 6, Mon=1 → 0
  return addDays(x, -mondayIndex);
}

export function startOfMonth(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), 1);
}

/** The 42 days (6 weeks, Monday-first) that fill a month grid. */
export function monthGridDays(anchor: Date): Date[] {
  const first = startOfWeek(startOfMonth(anchor));
  return Array.from({ length: 42 }, (_, i) => addDays(first, i));
}

/** The 7 days (Monday-first) of the week containing `anchor`. */
export function weekDays(anchor: Date): Date[] {
  const first = startOfWeek(anchor);
  return Array.from({ length: 7 }, (_, i) => addDays(first, i));
}

/** Whether an event (its parsed local start/end) touches the local day `day`.
 *  `end` is exclusive, so an event ending exactly at midnight doesn't spill. */
export function eventOnDay(start: Date, end: Date, day: Date): boolean {
  const dayStart = startOfDay(day).getTime();
  const dayEnd = dayStart + 86400000;
  return start.getTime() < dayEnd && end.getTime() > dayStart;
}

/** Format an `<input type="datetime-local">` value from a local Date. */
export function toLocalInput(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(
    d.getMinutes(),
  )}`;
}

/** Format an `<input type="date">` value from a local Date. */
export function toDateInput(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Fractional hour-of-day (e.g. 13.5 for 13:30) for time-grid positioning. */
export function hourFraction(d: Date): number {
  return d.getHours() + d.getMinutes() / 60;
}
