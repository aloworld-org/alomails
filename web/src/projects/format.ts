// Reading what the server stored: a duration in minutes, a day, a week, a rate,
// and the word for where a week stands.
//
// Every function here **formats one stored integer**. Nothing is summed and
// nothing is converted: a week's total, a project's hours and a budget
// proportion all arrive computed (`docs/design/projects.md` § Minutes), and a
// browser that re-derived one would be the second opinion an employee disputes.
//
// The one arithmetic this file does do is minutes → "7h 30m", which is a
// *rendering* of a single integer and not a sum. It is integer division, never
// a float: 0.1 + 0.2 has no place anywhere near a timesheet.
import { formatAmount } from "../billing";
import { getLocale, strings } from "../i18n";
import type { WeekStatus } from "./types";

/** Minutes in a day, for the week grid's own bounds. */
export const MINUTES_PER_DAY = 1440;

/**
 * A duration as a person reads it: `7h 30m`, `45m`, `8h`, `—` for nothing.
 *
 * Integer division of one stored integer — never a decimal number of hours,
 * which is how "1.75" ends up on a screen next to "1h 45m" on another and
 * somebody has to work out whether they agree.
 */
export function durationLabel(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes === 0) return strings.projectsNoTime;
  const sign = minutes < 0 ? "-" : "";
  const total = Math.abs(Math.trunc(minutes));
  const hours = Math.floor(total / 60);
  const rest = total % 60;
  if (hours === 0) return `${sign}${strings.projectsMinutesShort(rest)}`;
  if (rest === 0) return `${sign}${strings.projectsHoursShort(hours)}`;
  return `${sign}${strings.projectsHoursShort(hours)} ${strings.projectsMinutesShort(rest)}`;
}

/**
 * A duration in the form the minutes field takes and gives back: `7:30`.
 *
 * A grid cell is typed into as well as read from, and `h`/`m` letters are not
 * typeable at speed. Kept beside [`durationLabel`] so the two never drift.
 */
export function durationInput(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) return "";
  const total = Math.trunc(minutes);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/**
 * Reads what somebody typed into a duration field, in minutes, or `null` when
 * it is not a duration.
 *
 * Four spellings, because a timesheet is filled in at speed and all four are
 * what people actually type:
 *
 * | typed | means | why |
 * |---|---|---|
 * | `90` | ninety minutes | a bare number on a timesheet is minutes |
 * | `1:30` | ninety minutes | the clock spelling |
 * | `1,5` / `1.5` | ninety minutes | a decimal is always *hours* — nobody means one and a half minutes |
 * | `2h` | two hours | the `h` says hours, and it is the one suffix people type |
 *
 * A bare `2` is two minutes and `2h` is two hours: the letter is the whole
 * difference, which is why it changes the scale instead of being stripped and
 * ignored. The decimal form is the only one that can land between minutes, and
 * it **rounds to the nearest minute** rather than refusing — 0.1h is six
 * minutes exactly, and a form that argued about a third of an hour would be
 * arguing with the person who did the work.
 *
 * Anything else — letters, a negative, nothing, more than a day — is `null`,
 * and the form says so rather than storing a guess.
 */
export function parseDuration(raw: string): number | null {
  const typed = raw.trim().toLowerCase().replace(/\s+/g, "");
  const inHours = typed.endsWith("h");
  const text = inHours ? typed.slice(0, -1) : typed;
  if (text === "") return null;
  const colon = text.indexOf(":");
  if (colon >= 0) {
    // `1:30` is already hours and minutes; a trailing `h` on it says nothing new.
    const hours = Number(text.slice(0, colon) === "" ? "0" : text.slice(0, colon));
    const rest = text.slice(colon + 1);
    if (!/^[0-9]{1,2}$/.test(rest) || !Number.isInteger(hours) || hours < 0) return null;
    return bounded(hours * 60 + Number(rest));
  }
  if (/^[0-9]+$/.test(text)) return bounded(inHours ? Number(text) * 60 : Number(text));
  if (/^[0-9]*[.,][0-9]+$/.test(text)) {
    return bounded(Math.round(Number(text.replace(",", ".")) * 60));
  }
  return null;
}

/** A duration is a real part of one day, or it is not a duration. The server
 *  holds the same bound and refuses again; this is only so a person is told
 *  before they lose what they typed. */
function bounded(minutes: number): number | null {
  return Number.isInteger(minutes) && minutes > 0 && minutes <= MINUTES_PER_DAY ? minutes : null;
}

/**
 * A day the server wrote as `YYYY-MM-DD`.
 *
 * Built from its three numbers rather than parsed as a date string, because
 * `new Date("2026-09-30")` is an *instant* at UTC midnight — which reads as the
 * 29th for anybody west of Greenwich. A day somebody worked must survive being
 * shown back to them.
 */
export function dayLabel(day: string, options?: Intl.DateTimeFormatOptions): string {
  const parsed = dayValue(day);
  if (parsed === null) return day;
  return parsed.toLocaleDateString(
    getLocale(),
    options ?? { day: "numeric", month: "short", year: "numeric" },
  );
}

/** The same `YYYY-MM-DD`, as a local `Date` at midnight — the only conversion
 *  in the module, and the reason is in [`dayLabel`]. `null` when the text is
 *  not a day. */
export function dayValue(day: string): Date | null {
  const [y, m, d] = day.split("-").map(Number);
  if (y === undefined || m === undefined || d === undefined) return null;
  if (!Number.isInteger(y) || !Number.isInteger(m) || !Number.isInteger(d)) return null;
  const at = new Date(y, m - 1, d);
  return Number.isNaN(at.getTime()) ? null : at;
}

/** A local `Date` as the `YYYY-MM-DD` the API takes. Never an ISO instant: the
 *  day belongs to the worker's own zone, not to UTC. */
export function dayString(at: Date): string {
  const month = String(at.getMonth() + 1).padStart(2, "0");
  const day = String(at.getDate()).padStart(2, "0");
  return `${at.getFullYear()}-${month}-${day}`;
}

/** The Monday of the week `at` falls in, as `YYYY-MM-DD`. ISO weeks: Monday is
 *  the first day, which is what the server addresses a week by. */
export function mondayOf(at: Date): string {
  const monday = new Date(at.getFullYear(), at.getMonth(), at.getDate());
  // `getDay()` is 0 on Sunday, which is six days *after* its Monday.
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  return dayString(monday);
}

/** The seven days of the week starting at `monday`, as `YYYY-MM-DD`. */
export function weekDays(monday: string): string[] {
  const start = dayValue(monday);
  if (start === null) return [];
  return Array.from({ length: 7 }, (_, index) => {
    const at = new Date(start.getFullYear(), start.getMonth(), start.getDate() + index);
    return dayString(at);
  });
}

/** The Monday `weeks` weeks away from `monday`, as `YYYY-MM-DD`. */
export function shiftWeek(monday: string, weeks: number): string {
  const start = dayValue(monday);
  if (start === null) return monday;
  return dayString(new Date(start.getFullYear(), start.getMonth(), start.getDate() + weeks * 7));
}

/** An instant the server wrote (RFC 3339), read in the interface language. */
export function momentLabel(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return at.toLocaleString(getLocale(), { dateStyle: "medium", timeStyle: "short" });
}

/** An hourly rate the server stored, in its engagement's currency. `null` reads
 *  as "nobody has priced this" — never as zero, which would read as free. */
export function rateLabel(rateCents: number | null, currency: string): string {
  if (rateCents === null) return strings.projectsUnpriced;
  return strings.projectsPerHour(formatAmount(rateCents, getLocale(), currency));
}

/** A budget the server stored, in the engagement's currency. */
export function amountLabel(cents: number, currency: string): string {
  return formatAmount(cents, getLocale(), currency);
}

/** Basis points as a whole percentage, for the budget bar's label. Rounded for
 *  reading only — the bar itself is drawn from the basis points. */
export function percentLabel(basisPoints: number): string {
  return strings.projectsPercent(Math.round(basisPoints / 100));
}

/** What the server says a week's status is — never re-derived here from the
 *  timestamps, which is exactly how a screen and a server end up disagreeing
 *  about whether somebody's week was approved. */
export function weekStatusLabel(status: WeekStatus): string {
  if (status === "submitted") return strings.projectsWeekSubmitted;
  if (status === "approved") return strings.projectsWeekApproved;
  if (status === "rejected") return strings.projectsWeekRejected;
  return strings.projectsWeekOpen;
}

/** How long a running clock has been running, in whole minutes. The only clock
 *  arithmetic in the module, and it is a display of elapsed time rather than a
 *  duration anybody is billed for: the minutes that reach an entry are the
 *  server's, written when the clock is stopped. */
export function elapsedMinutes(startedAt: string, now: number): number {
  const started = new Date(startedAt).getTime();
  if (Number.isNaN(started)) return 0;
  return Math.max(0, Math.floor((now - started) / 60_000));
}
