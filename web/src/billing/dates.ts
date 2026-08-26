// The one place a billing document's dates are turned into something to read.
//
// An issue date and a due date are calendar days, not instants: the server
// sends them as `YYYY-MM-DD` and they mean that day everywhere, in every time
// zone. Parsing one with `new Date("2026-08-07")` and formatting it locally is
// how a document ends up dated the 6th for a reader west of Greenwich, so the
// parts are read as text and handed to `Intl` as a UTC date that is then
// formatted in UTC — the day that comes out is always the day that went in.

/** A calendar day as sent by the server, or `null` on a draft. */
export type DocumentDate = string | null;

/** `YYYY-MM-DD`, and nothing else — anything the server has not sent in that
 *  shape is shown verbatim rather than guessed at. */
const ISO_DAY = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * A calendar day for reading, in `locale`'s convention (7 Aug 2026 →
 * "7 Aug 2026", "7 août 2026", …). `null` — a draft's missing issue date —
 * formats as `fallback`, which the caller words ("—", "not issued yet").
 */
export function formatDocumentDate(date: DocumentDate, locale: string, fallback = ""): string {
  if (date === null) return fallback;
  const parts = ISO_DAY.exec(date);
  if (parts === null) return date;
  const [, year, month, day] = parts;
  const utc = Date.UTC(Number(year), Number(month) - 1, Number(day));
  if (Number.isNaN(utc)) return date;
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(utc));
}

/** A stored audit instant rendered as a compact date in lists. The complete
 * local date and time is available to pointer users without making a dense
 * document table wider than it needs to be. */
export function formatAuditDate(instant: string, locale: string): string {
  const date = new Date(instant);
  if (Number.isNaN(date.getTime())) return instant;
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

export function formatAuditDateTime(instant: string, locale: string): string {
  const date = new Date(instant);
  if (Number.isNaN(date.getTime())) return instant;
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}
