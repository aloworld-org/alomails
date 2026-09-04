// Pure presentation helpers for mail rows. Kept separate from components so
// they are unit-testable without a DOM. No locale is hardcoded — Intl uses the
// runtime locale (European formats by default in the EU).
import { KEYWORD_SEEN, type EmailHeaders } from "../jmap";
import { strings } from "../i18n";

/** Display name for a message's sender. */
export function senderName(headers: Pick<EmailHeaders, "from">): string {
  const first = headers.from?.[0];
  if (first === undefined) return strings.mailUnknownSender;
  if (first.name !== null && first.name.trim().length > 0) return first.name;
  return first.email;
}

/**
 * Display name for a message's **recipient** — the name a Sent, Drafts
 * or Scheduled row shows.
 *
 * In those folders the sender is always the account owner, so showing it
 * tells the reader nothing and hides the one name they came to find: a
 * Sent list rendered with [`senderName`] is a column of your own name,
 * and a message you just sent looks missing.
 *
 * Falls back to the first `cc` when there is no `to` (a message addressed
 * only by carbon copy is unusual but real), and to a placeholder when
 * there is neither — a draft with no recipient yet is an ordinary state,
 * not an error.
 *
 * Only the first recipient is shown, matching [`senderName`]'s shape. A
 * message to several people shows the first of them.
 */
export function recipientName(headers: Pick<EmailHeaders, "to" | "cc">): string {
  const first = headers.to?.[0] ?? headers.cc?.[0];
  if (first === undefined) return strings.mailNoRecipient;
  if (first.name !== null && first.name.trim().length > 0) return first.name;
  return first.email;
}

/** Subject with a placeholder for empty subjects. */
export function subjectOr(headers: Pick<EmailHeaders, "subject">): string {
  const s = headers.subject;
  return s !== null && s.trim().length > 0 ? s : strings.mailNoSubject;
}

/** Unread = the $seen keyword is absent. */
export function isUnread(headers: Pick<EmailHeaders, "keywords">): boolean {
  return headers.keywords[KEYWORD_SEEN] !== true;
}

/** Human-readable byte size (locale-aware number, binary units). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 ? value : Math.round(value * 10) / 10;
  return `${rounded.toLocaleString()} ${units[unit]}`;
}

/** Compact, locale-aware timestamp: time today, day+month this year, else
 * day+month+year. `now` is injectable for deterministic tests. */
export function formatDate(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  if (sameDay) {
    return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(date);
  }
  if (date.getFullYear() === now.getFullYear()) {
    return new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" }).format(date);
  }
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

/** Preserve the backend's useful reason for a human-facing recovery message. */
export function mailErrorReason(error: unknown): string | null {
  if (error instanceof Error && error.message.trim().length > 0) return error.message.trim();
  if (typeof error === "string" && error.trim().length > 0) return error.trim();
  return null;
}
