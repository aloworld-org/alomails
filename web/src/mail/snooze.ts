// Snooze preset times. Each returns a Unix-seconds wake time; the server hides
// the conversation until then and returns it to the Inbox (unread).
import { strings } from "../i18n";

export interface SnoozePreset {
  key: string;
  label: string;
  /** Human wake time, e.g. "8:00 AM". */
  when: string;
  /** Wake time in Unix seconds. */
  at: number;
}

const secs = (d: Date): number => Math.floor(d.getTime() / 1000);

function timeLabel(d: Date): string {
  return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

function dayTimeLabel(d: Date): string {
  return d.toLocaleDateString(undefined, { weekday: "short" }) + " " + timeLabel(d);
}

/** The snooze presets, computed relative to `now`. */
export function snoozePresets(now: Date = new Date()): SnoozePreset[] {
  // Later today: +3 hours, on the hour.
  const later = new Date(now);
  later.setHours(now.getHours() + 3, 0, 0, 0);

  // Tomorrow morning, 08:00.
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  tomorrow.setHours(8, 0, 0, 0);

  // This weekend: Saturday 08:00 (next Saturday if today is Sat/Sun).
  const weekend = new Date(now);
  const toSat = (6 - now.getDay() + 7) % 7 || 7;
  weekend.setDate(now.getDate() + toSat);
  weekend.setHours(8, 0, 0, 0);

  // Next week: Monday 08:00.
  const nextWeek = new Date(now);
  const toMon = (1 - now.getDay() + 7) % 7 || 7;
  nextWeek.setDate(now.getDate() + toMon);
  nextWeek.setHours(8, 0, 0, 0);

  return [
    { key: "later", label: strings.snoozeLaterToday, when: timeLabel(later), at: secs(later) },
    { key: "tomorrow", label: strings.snoozeTomorrow, when: timeLabel(tomorrow), at: secs(tomorrow) },
    { key: "weekend", label: strings.snoozeWeekend, when: dayTimeLabel(weekend), at: secs(weekend) },
    { key: "nextweek", label: strings.snoozeNextWeek, when: dayTimeLabel(nextWeek), at: secs(nextWeek) },
  ];
}
