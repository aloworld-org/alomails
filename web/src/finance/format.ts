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
import type {
  AccountRole,
  AccountType,
  BankLineStatus,
  BankSource,
  ExpenseMethod,
  ExpenseStatus,
  MatchEvidence,
} from "./types";

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

// ---- the chart of accounts ------------------------------------------------

/** What kind of thing an account holds, in words. An unknown kind — a category
 *  the server learned before this client did — is shown verbatim rather than
 *  blanked, exactly as a status is. */
export function accountTypeLabel(kind: AccountType): string {
  switch (kind) {
    case "asset":
      return strings.financeAccountTypeAsset;
    case "liability":
      return strings.financeAccountTypeLiability;
    case "equity":
      return strings.financeAccountTypeEquity;
    case "income":
      return strings.financeAccountTypeIncome;
    case "expense":
      return strings.financeAccountTypeExpense;
    default:
      return kind;
  }
}

/**
 * The job an account does in a posting rule, in words.
 *
 * These are the sentences that make the chart editable by somebody who is not
 * an accountant: "the bank account money actually moves through" is what a role
 * *means*, and `bank` is only what it is called on the wire.
 */
export function accountRoleLabel(role: AccountRole): string {
  switch (role) {
    case "ar":
      return strings.financeRoleAr;
    case "ap":
      return strings.financeRoleAp;
    case "bank":
      return strings.financeRoleBank;
    case "cash":
      return strings.financeRoleCash;
    case "vat_output":
      return strings.financeRoleVatOutput;
    case "vat_input":
      return strings.financeRoleVatInput;
    case "revenue":
      return strings.financeRoleRevenue;
    case "expense_default":
      return strings.financeRoleExpenseDefault;
    case "employee_payable":
      return strings.financeRoleEmployeePayable;
    case "fx_diff":
      return strings.financeRoleFxDiff;
    case "rounding":
      return strings.financeRoleRounding;
    case "opening_balance":
      return strings.financeRoleOpeningBalance;
    case "suspense":
      return strings.financeRoleSuspense;
    default:
      return role;
  }
}

// ---- the bank -------------------------------------------------------------

/** Which reader understood the file, in words a bank customer recognises. */
export function sourceLabel(source: BankSource): string {
  switch (source) {
    case "camt":
      return strings.financeBankSourceCamt;
    case "mt940":
      return strings.financeBankSourceMt940;
    case "csv":
      return strings.financeBankSourceCsv;
    default:
      return source;
  }
}

/** Where a staged line stands. */
export function lineStatusLabel(status: BankLineStatus): string {
  switch (status) {
    case "unmatched":
      return strings.financeBankUnmatched;
    case "matched":
      return strings.financeBankMatched;
    case "ignored":
      return strings.financeBankIgnored;
    default:
      return status;
  }
}

/** How loudly a staged line's state reads. */
export function lineStatusTone(status: BankLineStatus): "info" | "good" | "bad" | "quiet" {
  switch (status) {
    case "matched":
      return "good";
    case "ignored":
      return "quiet";
    default:
      return "info";
  }
}

/**
 * The sentence behind one piece of evidence, in the reader's own language.
 *
 * The server sends a **token and its numbers** and never a sentence, precisely
 * so that this function can exist (`finance_bank_match.rs`). A token this client
 * has not learned yet — a stage the server grew first — is skipped rather than
 * printed raw: an untranslated identifier in a list of reasons reads as a bug,
 * and the guess is still shown with the reasons that did translate.
 */
export function evidenceLabel(evidence: MatchEvidence, currency: string): string | null {
  switch (evidence.kind) {
    case "numberQuoted":
      return strings.financeBankWhyNumberQuoted;
    case "ruleSaved":
      return strings.financeBankWhyRuleSaved;
    case "customerNamed":
      // Basis points, like every rate the suite stores: 8_500 is 85%.
      return strings.financeBankWhyCustomerNamed(Math.round(evidence.similarityBp / 100));
    case "wholeAmount":
      return strings.financeBankWhyWholeAmount;
    case "onlyDocumentForTheAmount":
      return strings.financeBankWhyOnlyDocument;
    case "nearDue":
      // The server's sign convention: negative is before the day it was due.
      return evidence.days < 0
        ? strings.financeBankWhyBeforeDue(-evidence.days)
        : strings.financeBankWhyAfterDue(evidence.days);
    case "partPayment":
      return strings.financeBankWhyPartPayment(amountLabel(evidence.remainingCents, currency));
    default:
      return null;
  }
}
