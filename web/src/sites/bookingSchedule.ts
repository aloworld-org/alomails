// The arithmetic-free part of a booking service: turning the week an owner
// types into the shape the route speaks, and back (S2.13c).
//
// Nothing here rules on anything. Which windows overlap, which duration fits,
// which time zone exists and which free times a visitor is finally offered are
// all the server's — the store validates a write and computes availability, and
// its refusal sentence is what the screen shows. What lives here is the
// translation between "09:00" in a time field and the minutes-from-midnight the
// wire carries, and the words a week is read in.
import { getLocale } from "../i18n";
import type {
  SiteBooking,
  SiteBookingDraft,
  SiteBookingField,
  SiteBookingWindow,
} from "./types";

/** ISO weekdays in the order a week is read: 1 = Monday … 7 = Sunday. */
export const WEEKDAYS: readonly number[] = [1, 2, 3, 4, 5, 6, 7];

/** The weekday's name in the reader's own language. 2024-01-01 was a Monday,
 *  so the reference date is that Monday plus the ISO weekday minus one. */
export function weekdayName(weekday: number): string {
  const monday = new Date(2024, 0, 1);
  const day = new Date(monday.getTime());
  day.setDate(monday.getDate() + (weekday - 1));
  return new Intl.DateTimeFormat(getLocale(), { weekday: "long" }).format(day);
}

/** Minutes from midnight as a `<input type="time">` value ("09:00"). Values
 *  outside the day are clamped rather than wrapped: a window that reached here
 *  broken is shown at its edge, never silently moved to another hour. */
export function timeValue(minutes: number): string {
  const clamped = Math.min(Math.max(Math.trunc(minutes), 0), 24 * 60);
  const hours = Math.floor(clamped / 60);
  const rest = clamped % 60;
  return `${String(hours).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

/** A time field's value as minutes from midnight, or null when the field is
 *  empty or not a time at all — the caller keeps the window it had rather than
 *  writing a zero nobody typed. */
export function timeMinutes(value: string): number | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (match === null) return null;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (hours > 24 || minutes > 59) return null;
  const total = hours * 60 + minutes;
  return total > 24 * 60 ? null : total;
}

/** One window as a person reads it: "Monday 09:00–17:00". */
export function windowLabel(window: SiteBookingWindow): string {
  return `${weekdayName(window.weekday)} ${timeValue(window.startMinute)}–${timeValue(
    window.endMinute,
  )}`;
}

/** The working week most services start from — weekday mornings and
 *  afternoons in one block — so a first service is one name away from being
 *  offerable rather than a blank timetable. */
export function defaultHours(): SiteBookingWindow[] {
  return [1, 2, 3, 4, 5].map((weekday) => ({
    weekday,
    startMinute: 9 * 60,
    endMinute: 17 * 60,
  }));
}

/** The zone the browser is in, which is the zone an owner writing opening
 *  hours almost always means. The server rules on whether it knows the name. */
export function browserTimeZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "Europe/Brussels";
  } catch {
    return "Europe/Brussels";
  }
}

/** A stable key suggested from a question's label — lowercase words joined by
 *  underscores. A suggestion only: the field stays editable, and the server
 *  owns the rule (a key it refuses comes back as a sentence naming it). */
export function suggestFieldKey(label: string): string {
  return label
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 40);
}

/** A new service, ready to be named: the working week, this browser's zone, a
 *  half hour, and no extra questions — name and email are always asked. */
export function emptyBookingDraft(calendarId: string): SiteBookingDraft {
  return {
    name: "",
    description: "",
    calendarId,
    timeZone: browserTimeZone(),
    durationMinutes: 30,
    bufferMinutes: 0,
    noticeMinutes: 120,
    horizonDays: 60,
    location: "",
    hours: defaultHours(),
    fields: [],
    active: true,
  };
}

/** A stored service as the form edits it. Absent optional text becomes an
 *  empty field, which is what the route reads back as absent. */
export function bookingDraft(booking: SiteBooking): SiteBookingDraft {
  return {
    name: booking.name,
    description: booking.description ?? "",
    calendarId: booking.calendarId,
    timeZone: booking.timeZone,
    durationMinutes: booking.durationMinutes,
    bufferMinutes: booking.bufferMinutes,
    noticeMinutes: booking.noticeMinutes,
    horizonDays: booking.horizonDays,
    location: booking.location ?? "",
    hours: booking.hours.map((window) => ({ ...window })),
    fields: booking.fields.map((field) => ({ ...field, options: [...field.options] })),
    active: booking.active,
  };
}

/** A blank extra question. */
export function blankBookingField(): SiteBookingField {
  return { key: "", label: "", kind: "text", required: false, options: [] };
}

/** A blank opening window on the given day, at the working hours the default
 *  week uses, so adding a Saturday is one click and not four fields. */
export function blankWindow(weekday: number): SiteBookingWindow {
  return { weekday, startMinute: 9 * 60, endMinute: 17 * 60 };
}
