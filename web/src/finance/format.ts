// Reading what the server stored: an amount, a day, an instant, and the words
// for where a claim stands and whose money paid.
//
// Every function here **formats one stored value**. Nothing is summed, nothing
// is converted and nothing is derived from two fields at once: a claim's net,
// its state and whether it may still be edited all arrive computed
// (`docs/design/finance.md`), and a browser that re-derived one would be the
// second opinion an employee disputes on payday.
//
// The amount formatter and the day formatter are Billing's — money was first
// typed and printed there, and a second module that shows an amount reads them
// from that module rather than growing a second, slightly different one.
import { formatAmount, formatDocumentDate } from "../billing";
import { getLocale, strings } from "../i18n";
import type { ExpenseMethod, ExpenseStatus } from "./types";

/** An amount the server stored, in the currency it stored beside it. */
export function amountLabel(cents: number, currency: string): string {
  return formatAmount(cents, getLocale(), currency);
}

/** A `YYYY-MM-DD` day for reading. Formatted as a calendar day and never as an
 *  instant: a purchase made on the 1st must not read as the 31st for a reader
 *  west of Greenwich, which is exactly what parsing it as a timestamp does. */
export function dayLabel(day: string | null, fallback = ""): string {
  return formatDocumentDate(day, getLocale(), fallback);
}

/** An instant the server wrote (RFC 3339), read in the interface language. */
export function momentLabel(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return at.toLocaleString(getLocale(), { dateStyle: "medium", timeStyle: "short" });
}

/** Today in the reader's own zone, as `YYYY-MM-DD` — what a claim form and a
 *  payback form open on. Local, never UTC: "today" is a fact about where the
 *  person is, and it is the day the money moved. */
export function today(now = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

/** What to call a status. An unknown one — a state the server learned before
 *  this client did — is shown verbatim rather than blanked. */
export function statusLabel(status: ExpenseStatus): string {
  switch (status) {
    case "draft":
      return strings.financeStatusDraft;
    case "submitted":
      return strings.financeStatusSubmitted;
    case "approved":
      return strings.financeStatusApproved;
    case "rejected":
      return strings.financeStatusRejected;
    case "reimbursed":
      return strings.financeStatusReimbursed;
    default:
      return status;
  }
}

/** How loudly a status reads: a draft is quiet, a refusal is loud, money paid
 *  back is done. */
export function statusTone(status: ExpenseStatus): "info" | "good" | "bad" | "quiet" {
  switch (status) {
    case "submitted":
      return "info";
    case "approved":
    case "reimbursed":
      return "good";
    case "rejected":
      return "bad";
    default:
      return "quiet";
  }
}

/** Whose money paid, in words. */
export function methodLabel(method: ExpenseMethod): string {
  switch (method) {
    case "personal":
      return strings.financeMethodPersonal;
    case "card":
      return strings.financeMethodCard;
    case "cash":
      return strings.financeMethodCash;
    default:
      return method;
  }
}
