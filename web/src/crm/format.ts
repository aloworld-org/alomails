// Reading what the server stored: a deal's value, the day it is expected to
// close, the moment something happened, and the word for where it stands.
//
// Every function here formats ONE stored number or string for the interface
// language. Nothing is summed, converted or re-derived: the pipeline report is
// the server's and even it refuses to convert currencies
// (`docs/design/crm.md`), so a browser certainly does not.
import { formatAmount } from "../billing";
import { getLocale, strings } from "../i18n";
import type { CrmDeal, DealState } from "./types";

/** One deal's value, in its own currency, read in the interface language. */
export function dealValue(deal: CrmDeal): string {
  return formatAmount(deal.valueCents, getLocale(), deal.currency);
}

/**
 * A day the server wrote as `YYYY-MM-DD`.
 *
 * Built from its three numbers rather than parsed as a date string, because
 * `new Date("2026-09-30")` is an *instant* at UTC midnight — which reads as the
 * 29th for anybody west of Greenwich. A day a user chose must survive being
 * shown back to them.
 */
export function dayLabel(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  if (y === undefined || m === undefined || d === undefined || !Number.isFinite(y)) return day;
  return new Date(y, m - 1, d).toLocaleDateString(getLocale(), {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

/** An instant the server wrote (RFC 3339), read in the interface language. */
export function momentLabel(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return at.toLocaleString(getLocale(), { dateStyle: "medium", timeStyle: "short" });
}

/** The `YYYY-MM-DD` a date input wants, from an instant — used to prefill "due
 *  today" style fields without ever inventing a timezone. */
export function todayInputValue(now: Date): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

/** What the server says a deal's state is — never re-derived here from the
 *  column's flags, which is exactly how a client and a server end up
 *  disagreeing about whether something was won. */
export function stateLabel(state: DealState): string {
  if (state === "won") return strings.crmStateWon;
  if (state === "lost") return strings.crmStateLost;
  return strings.crmStateOpen;
}

/** What a log entry is called. */
export function kindLabel(kind: string): string {
  if (kind === "call") return strings.crmKindCall;
  if (kind === "meeting") return strings.crmKindMeeting;
  return strings.crmKindNote;
}
