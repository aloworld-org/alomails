// Reading leave: the words for a state, the arithmetic a calendar grid needs,
// and which of the four verbs a row may offer.
//
// Everything here is pure, and everything here is deliberately **not** a rule.
// Two lines are worth stating because the whole module leans on them:
//
//   - **No figure is computed.** A balance arrives in minutes and in tenths of
//     a day, both from the server, folded over the person's own working pattern
//     and the tenant's public holidays. This file turns `125` into "12.5 days"
//     and does no other arithmetic on it — dividing minutes by a working day
//     guessed in a browser is how one person gets told two different numbers
//     about their own holiday.
//   - **No permission is decided.** `canWithdraw`, `canCancel` and `canDecide`
//     choose what to *draw*. Every one of those acts travels a route that asks
//     the same question again server-side (`hr_leave_door.rs`), so being wrong
//     here shows a control that gets refused with the server's own sentence —
//     never a control that gets away with something. They exist because a
//     screen that offers every verb on every row teaches nothing about which
//     ones are yours (`docs/design/ux-principles.md`).
//
// The clock is the same shape of decision. `GET /hr/leave-balances` echoes the
// day it folded to, and that day is the server's own — so "has this absence
// already begun" is asked against the calendar the refusal would come from,
// not against the reader's device. [`browserToday`] is the fallback for the one
// caller who has no balance to read, and it decides nothing but a default.
import { getLocale, strings } from "../i18n";
import type { ChipTone } from "./parts";
import type { HrAbsenceDay, HrAbsentPerson, HrHoliday, HrLeaveRequest } from "./types";

/** A day written the way every `/hr` route writes one. */
function iso(date: Date): string {
  const month = String(date.getUTCMonth() + 1).padStart(2, "0");
  const day = String(date.getUTCDate()).padStart(2, "0");
  return `${date.getUTCFullYear()}-${month}-${day}`;
}

/** The reader's own day, as `YYYY-MM-DD`.
 *
 *  Used for two things only: which month the absence calendar opens on, and the
 *  clock when the caller has no balance to take the server's day from. Never
 *  for a figure and never for a refusal. */
export function browserToday(): string {
  const now = new Date();
  return iso(new Date(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate())));
}

/** Tenths of a day, as a person reads them: `125` → "12.5 days", `10` → "1 day".
 *
 *  A whole number of days loses its decimal, because "1.0 days off" is a
 *  spreadsheet talking. */
export function daysLabel(tenths: number): string {
  if (tenths === 10) return strings.hrOneDay;
  // Written by the locale, not by `String(12.5)`: half a day is "12,5" in
  // French and the figure beside it is a person's holiday.
  return strings.hrDaysOf(
    (tenths / 10).toLocaleString(getLocale(), { maximumFractionDigits: 1 }),
  );
}

/** What a state is called. A word from a newer server is shown verbatim rather
 *  than dropped: a row with no state at all reads as a bug. */
export function leaveStatusLabel(status: string): string {
  switch (status) {
    case "requested":
      return strings.hrLeaveRequested;
    case "approved":
      return strings.hrLeaveApproved;
    case "rejected":
      return strings.hrLeaveRejected;
    case "withdrawn":
      return strings.hrLeaveWithdrawn;
    case "cancelled":
      return strings.hrLeaveCancelled;
    default:
      return status;
  }
}

/** How a state is coloured: approved is good, refused and taken-back are not,
 *  waiting is neither. */
export function leaveStatusTone(status: string): ChipTone {
  if (status === "approved") return "good";
  if (status === "rejected" || status === "cancelled" || status === "withdrawn") return "bad";
  return "info";
}

/** Taking back a request: its owner, and only while nobody has decided it. */
export function canWithdraw(request: HrLeaveRequest, me: string | null): boolean {
  return request.status === "requested" && me !== null && request.employeeId === me;
}

/** Giving approved leave back. Anybody who can see the row may do it — the
 *  person, their manager, HR — but never after it has started: the fact that
 *  somebody was absent last Tuesday is corrected by HR with a reason, not
 *  erased by a button. */
export function canCancel(request: HrLeaveRequest, today: string): boolean {
  return request.status === "approved" && request.fromDay > today;
}

/** Deciding somebody's request. Waiting, and not the reader's own: leave is
 *  never approved by the person taking it (an admin excepted, and an admin
 *  reaching for it here gets the server's sentence rather than a hidden
 *  button). */
export function canDecide(request: HrLeaveRequest, me: string | null): boolean {
  return request.status === "requested" && request.employeeId !== me;
}

/** The days of the window, in the order they are asked about. */
export function absenceIndex(days: HrAbsenceDay[]): Map<string, HrAbsentPerson[]> {
  return new Map(days.map((entry) => [entry.day, entry.people]));
}

/** The tenant's non-working days, by day. */
export function holidayIndex(holidays: HrHoliday[]): Map<string, string> {
  return new Map(holidays.map((day) => [day.date, day.name]));
}

/** Everybody away at any point inside a window, once each, in the order they
 *  first appear — what the request form shows beside the dates somebody is
 *  choosing, so "can I take this week" is answered by looking. */
export function peopleAway(days: HrAbsenceDay[], exclude: string | null): HrAbsentPerson[] {
  const seen = new Set<string>();
  const people: HrAbsentPerson[] = [];
  for (const day of days) {
    for (const person of day.people) {
      if (person.employeeId === exclude || seen.has(person.employeeId)) continue;
      seen.add(person.employeeId);
      people.push(person);
    }
  }
  return people;
}

/** A month, as the address writes it: `2026-08`. */
export function monthOf(day: string): string {
  return day.slice(0, 7);
}

/** The month `delta` months from this one, wrapping the year. */
export function shiftMonth(month: string, delta: number): string {
  const [year, index] = split(month);
  const moved = new Date(Date.UTC(year, index - 1 + delta, 1));
  return iso(moved).slice(0, 7);
}

/** The month's name and year, in the interface language. */
export function monthLabel(month: string): string {
  const [year, index] = split(month);
  return new Date(Date.UTC(year, index - 1, 1)).toLocaleDateString(getLocale(), {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
}

/** The six Monday-first weeks that cover a month, as `YYYY-MM-DD` days.
 *
 *  Always six, so the grid does not change height between months — a calendar
 *  whose rows move as you page through it is a calendar you lose your place
 *  in. */
export function monthWeeks(month: string): string[][] {
  const [year, index] = split(month);
  const first = new Date(Date.UTC(year, index - 1, 1));
  const start = new Date(first);
  // Monday-first: JavaScript counts Sunday as 0, and Europe does not.
  start.setUTCDate(1 - ((first.getUTCDay() + 6) % 7));
  return Array.from({ length: 6 }, (_, week) =>
    Array.from({ length: 7 }, (_, weekday) => {
      const day = new Date(start);
      day.setUTCDate(start.getUTCDate() + week * 7 + weekday);
      return iso(day);
    }),
  );
}

/** The seven weekday names a Monday-first grid is headed with. */
export function weekdayNames(): string[] {
  // 2024-01-01 was a Monday; any Monday would do.
  return Array.from({ length: 7 }, (_, i) =>
    new Date(Date.UTC(2024, 0, 1 + i)).toLocaleDateString(getLocale(), {
      weekday: "short",
      timeZone: "UTC",
    }),
  );
}

/** Saturday or Sunday — toned down in the grid, because a company that is shut
 *  on Saturday should not read as a company where everybody is away. */
export function isWeekend(day: string): boolean {
  const [year, month, date] = day.split("-").map(Number);
  const weekday = new Date(Date.UTC(year ?? 0, (month ?? 1) - 1, date ?? 1)).getUTCDay();
  return weekday === 0 || weekday === 6;
}

/** The distinct years a set of days falls in — one holiday read each, so a grid
 *  spanning New Year marks both sides of it. */
export function yearsOf(days: string[]): number[] {
  const years = new Set(days.map((day) => Number(day.slice(0, 4))));
  return [...years].sort((a, b) => a - b);
}

/** `2026-08` → `[2026, 8]`. */
function split(month: string): [number, number] {
  const [year, index] = month.split("-").map(Number);
  return [year ?? 1970, index ?? 1];
}
