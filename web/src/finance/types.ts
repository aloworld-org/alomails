// The shapes the `/finance` API answers with (alo Finance, ADR 0035, wave B4).
// One interface per JSON object the server sends, named as the server names it
// — no field this file invented, because a derived field is a field the screen
// and the server can disagree about.
//
// Every amount is an integer count of **cents** and every VAT rate an integer
// count of **basis points**; neither is ever a float, and neither is ever
// summed here. `netCents` arrives computed for exactly that reason
// (`docs/design/finance.md` § Expenses).

/** Where a claim is in the flow. The server's word, never re-derived here from
 *  the timestamps beside it — that is how a screen and a server end up
 *  disagreeing about whether somebody was paid. */
export type ExpenseStatus = "draft" | "submitted" | "approved" | "rejected" | "reimbursed";

/** Whose money paid. It decides what the approval books, and whether anybody is
 *  owed anything afterwards. */
export type ExpenseMethod = "personal" | "card" | "cash";

/** One expense claim, as its claimant reads it back.
 *
 * `editable` and `owesTheEmployee` are the server's own answers to "may this
 * still be changed" and "does approving it leave a debt": both are shown rather
 * than worked out from `status` and `method` here, so a rule that moves on the
 * server moves on the screen with it. */
export interface Expense {
  id: string;
  /** `YYYY-MM-DD`, the day the money left — the claimant's day, in their zone. */
  spentOn: string;
  categoryId: string | null;
  merchant: string;
  description: string;
  grossCents: number;
  vatCents: number;
  /** Gross less VAT, computed by the server. */
  netCents: number;
  vatRateBp: number | null;
  /** ISO 4217, uppercase. */
  currency: string;
  method: ExpenseMethod;
  projectId: string | null;
  receiptNodeId: string | null;
  status: ExpenseStatus;
  /** Whether the claim is still the claimant's own to change. */
  editable: boolean;
  /** Whether approving it leaves the company owing the claimant. */
  owesTheEmployee: boolean;
  /** RFC 3339, or `null` while it is a draft. */
  submittedAt: string | null;
  decidedBy: string | null;
  decidedAt: string | null;
  /** What the approver wrote. Empty when they wrote nothing. */
  decisionNote: string;
  /** `YYYY-MM-DD` the money moved, or `null`. */
  reimbursedOn: string | null;
  createdAt: string;
  updatedAt: string;
}

/** A claim on an approver's screen: the same claim, plus the three facts only a
 *  cross-user read carries — whose it is, their address, and the name of what
 *  it books to. */
export interface PendingExpense extends Expense {
  userId: string;
  userEmail: string;
  categoryName: string | null;
}

/** What the claim form sends. Absent fields keep the stored value on a `PATCH`,
 *  so a form that shows every field states every field it owns.
 *
 *  `currency` is omitted when the claimant did not name one: the workspace's
 *  own currency is the server's default, and a client that filled one in would
 *  be deciding what money a receipt is in. */
export interface ExpenseDraft {
  spentOn: string;
  merchant: string;
  description: string;
  grossCents: number;
  vatCents: number;
  vatRateBp: number | null;
  method: ExpenseMethod;
  projectId: string | null;
  currency?: string;
}
