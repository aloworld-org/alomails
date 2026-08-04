// Per-calendar display colours. Each calendar carries an optional stored hex
// (`Calendar.color`); when it has none, we assign one from a fixed palette by
// order, so a user's calendars stay visually distinct (Google/Outlook-style
// category hues). These are DATA colours — event category hues, not the app's
// design-system chrome — so concrete values live here rather than in tokens.
import type { Calendar } from "../jmap";

const PALETTE = [
  "#e76f51", // terracotta (the brand accent — the personal calendar)
  "#4b83c4", // blue
  "#2e8b57", // green
  "#9b6dd6", // violet
  "#e0a63b", // amber
  "#d1568f", // magenta
  "#3aa8a0", // teal
];

/** Map every calendar id to its display colour (stored hex, else palette). */
export function calendarColorMap(calendars: Calendar[]): Map<string, string> {
  const map = new Map<string, string>();
  calendars.forEach((c, i) => {
    map.set(c.id, c.color ?? PALETTE[i % PALETTE.length] ?? "#e76f51");
  });
  return map;
}
