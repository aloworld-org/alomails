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

// ---- the bank (B4.08) and the reconciliation screen (B4.09) ---------------
//
// Same rule as above, and it matters more here: not one field below is derived.
// What a line moves, what an invoice still owes, why a guess was made and how
// strongly — all of it arrives computed, because the screen that re-derived any
// of it would be a second opinion about somebody's money.

/** Which reader understood the file. */
export type BankSource = "camt" | "mt940" | "csv";

/** Where a staged line stands. */
export type BankLineStatus = "unmatched" | "matched" | "ignored";

/** One imported statement — the header the Bank tab lists imports by. */
export interface BankStatement {
  id: string;
  accountIban: string;
  currency: string;
  source: BankSource;
  /** The bank's own name for the statement; empty when the file gave none. */
  statementRef: string | null;
  openingBalanceCents: number | null;
  closingBalanceCents: number | null;
  /** `YYYY-MM-DD`. */
  fromDate: string | null;
  toDate: string | null;
  importedBy: string;
  /** RFC 3339. */
  importedAt: string;
  lineCount: number;
}

/** One staged transaction, as the reader understood the file. */
export interface BankLine {
  id: string;
  statementId: string;
  lineNo: number;
  bookedOn: string | null;
  valueOn: string | null;
  /** Signed integer cents: positive is money in. */
  amountCents: number;
  currency: string;
  counterpartyName: string | null;
  counterpartyIban: string | null;
  /** What was written on the payment. */
  remittance: string | null;
  bankRef: string | null;
  status: BankLineStatus;
  /** Why it is not ours to book; `null` unless it is ignored. */
  ignoredReason: string | null;
  createdAt: string;
}

/** A row of the file the reader could not turn into a transaction. It names the
 *  line and the rule, never the row's contents (Law 1). */
export interface BankRowError {
  /** The line of the file, when the reader could tell which. */
  line: number | null;
  /** The server's own sentence for what is wrong with it. */
  rule: string;
}

/** One transaction as a preview shows it, before anything is staged. */
export interface BankSampleLine {
  line: number | null;
  bookedOn: string | null;
  valueOn: string | null;
  amountCents: number;
  currency: string;
  counterpartyName: string | null;
  counterpartyIban: string | null;
  remittance: string | null;
  bankRef: string | null;
}

/** Which column of a CSV is which. Every value is a **column name from the
 *  file's header**; `null` is a field the reader did not map. The server sends
 *  back what it guessed, and the screen sends it forward again. */
export interface BankCsvMapping {
  date: string | null;
  valueDate: string | null;
  amount: string | null;
  debit: string | null;
  credit: string | null;
  sign: string | null;
  currencyColumn: string | null;
  counterparty: string | null;
  iban: string | null;
  remittance: string | null;
  reference: string | null;
}

/** What a file would stage, or did: how it was read, what it holds, and what
 *  cannot be read at all. The same shape answers the dry run, the commit and
 *  the `422` that refuses one. */
export interface BankImportReport {
  /** Whether anything was written. `false` on a preview *and* on a refusal. */
  committed: boolean;
  source: BankSource;
  encoding: string | null;
  delimiter: string | null;
  /** The file's own header, in file order — what a mapping picks from. */
  columns: string[];
  mapping: BankCsvMapping;
  dates: string;
  decimal: string;
  totalRows: number;
  counts: {
    /** Readable transactions in the file. */
    lines: number;
    /** Rows that are not transactions (a header repeat, a balance line). */
    skipped: number;
    errors: number;
    /** Written; `null` when nothing was. */
    staged: number | null;
    /** Skipped because this exact transaction is already staged. */
    duplicates: number | null;
    /** Read, but not yet booked by the bank. */
    unbooked: number | null;
  };
  account: string | null;
  currency: string | null;
  period: { from: string | null; to: string | null } | null;
  sample: BankSampleLine[];
  /** Whether `sample` is a sample rather than the whole file. */
  sampleTruncated: boolean;
  skippedLines: number[];
  errors: BankRowError[];
  /** The stored statement — present only on a commit that succeeded. */
  statement: BankStatement | null;
}

/** What the upload states about a file: the account it belongs to and, for a
 *  CSV, the conventions and the mapping. All of it is ignored for a CAMT.053 or
 *  an MT940, which state these things themselves. */
export interface BankImportOptions {
  format?: BankSource | "";
  account?: string;
  currency?: string;
  dates?: string;
  decimal?: string;
  mapping?: Partial<BankCsvMapping>;
}

/** Why the guessing stage thinks a line settled a document: a **token and its
 *  numbers**, never a sentence. The screen writes the sentence, in the reader's
 *  own language — see `finance_bank_match.rs`. */
export type MatchEvidence =
  | { kind: "numberQuoted" }
  | { kind: "ruleSaved"; ruleId: string; matchOn: string }
  | { kind: "customerNamed"; similarityBp: number }
  | { kind: "wholeAmount" }
  | { kind: "onlyDocumentForTheAmount" }
  | { kind: "nearDue"; days: number }
  | { kind: "partPayment"; remainingCents: number };

/** The certain guess: the payer quoted our own number and sent what it owes. */
export interface ExactMatch {
  invoiceId: string;
  number: string;
  amountCents: number;
  daysAfterIssue: number;
}

/** A ranked guess, with what it is based on. */
export interface LikelyMatch {
  invoiceId: string;
  number: string;
  amountCents: number;
  /** What the document still owes, computed by the server. */
  outstandingCents: number;
  customerId: string;
  daysAfterIssue: number;
  score: number;
  evidence: MatchEvidence[];
  /** The learned rule that proposed it, when one did. */
  ruleId: string | null;
}

/** One unmatched line with what it might be. */
export interface LineSuggestions {
  line: BankLine;
  exact: ExactMatch[];
  likely: LikelyMatch[];
}

/** The whole read, caps included — a short list has to be able to say it is
 *  short, or a bookkeeper concludes there is nothing left to match. */
export interface BankSuggestions {
  lines: LineSuggestions[];
  numbersCapped: boolean;
  ledgerCapped: boolean;
}

/** What a settlement did. */
export interface ConfirmedMatch {
  id: string;
  lineId: string;
  targetKind: string;
  targetId: string;
  amountCents: number;
  paymentId: string | null;
  entryId: string | null;
  ruleId: string | null;
  confirmedBy: string;
  confirmedAt: string;
  invoiceEntryId: string;
  /** Whether this act is what put the invoice itself into the books. */
  invoiceBookedNow: boolean;
}

// ---- the chart of accounts (B4.13c) ---------------------------------------

/** What kind of thing an account holds — the five categories every
 *  double-entry chart has had since Pacioli, and what decides whether a line
 *  belongs on the balance sheet or in the result. */
export type AccountType = "asset" | "liability" | "equity" | "income" | "expense";

/** The job an account does in a posting rule, as opposed to the number an
 *  accountant calls it by. A closed set the server owns; at most one account
 *  per tenant holds each. */
export type AccountRole =
  | "ar"
  | "ap"
  | "bank"
  | "cash"
  | "vat_output"
  | "vat_input"
  | "revenue"
  | "expense_default"
  | "employee_payable"
  | "fx_diff"
  | "rounding"
  | "opening_balance"
  | "suspense";

/** One account of the tenant's chart.
 *
 *  The four movement fields are the **journal's**, present only when the list
 *  was asked for over a period and `null` otherwise: a zero where nothing was
 *  read would be a claim about the books nobody made. */
export interface ChartAccount {
  id: string;
  code: string;
  name: string;
  type: AccountType;
  role: AccountRole | null;
  /** Whether it takes new postings and offers itself in the pickers. */
  active: boolean;
  /** Whether alo seeded it — renameable and recodeable, never deletable. */
  system: boolean;
  balanceCents: number | null;
  debitCents: number | null;
  creditCents: number | null;
  postings: number | null;
  createdAt: string;
  updatedAt: string;
}

/** The chart as the list door answers it. `seeded` is true when this very read
 *  is what wrote the default chart, which is what lets the screen say where
 *  twenty accounts nobody typed came from. */
export interface Chart {
  accounts: ChartAccount[];
  seeded: boolean;
  /** The accounting currency the movements are in, or `null` when no period was
   *  asked for and there are therefore no movements to state it about. */
  currency: string | null;
}

/** What a create or an edit sends. Every field is optional because a `PATCH`
 *  merges: what is absent is left exactly as it was, which is what stops a
 *  rename clearing the role and unhooking a posting rule. `role: ""` is how a
 *  form says "an ordinary account". */
export interface AccountDraft {
  code?: string;
  name?: string;
  type?: AccountType;
  role?: AccountRole | "";
  active?: boolean;
}

// ---- the four reports (B4.11, drawn at B4.13c) -----------------------------

/** One account's line in the profit and loss, with the same account's figure
 *  for the period before it. */
export interface PlLine {
  accountId: string;
  code: string;
  name: string;
  type: AccountType;
  amountCents: number;
  previousCents: number;
  postings: number;
}

/** What the business earned and spent between two days, and the comparative
 *  period the **server** chose — never one the browser derived. */
export interface PlReport {
  from: string;
  to: string;
  previousFrom: string;
  previousTo: string;
  currency: string;
  income: PlLine[];
  expense: PlLine[];
  incomeCents: number;
  expenseCents: number;
  resultCents: number;
  previousIncomeCents: number;
  previousExpenseCents: number;
  previousResultCents: number;
}

/** One account's balance on the sheet. */
export interface BalanceLine {
  accountId: string;
  code: string;
  name: string;
  type: AccountType;
  role: AccountRole | null;
  amountCents: number;
  postings: number;
}

/** What the business owns, owes and is worth on one day.
 *
 *  `balances` is stated by the server and shown by the screen: a sheet that
 *  does not balance prints exactly like one that does, so the screen says so
 *  out loud rather than rounding the difference into equity. */
export interface BalanceSheet {
  on: string;
  currency: string;
  assets: BalanceLine[];
  liabilities: BalanceLine[];
  equity: BalanceLine[];
  assetCents: number;
  liabilityCents: number;
  equityCents: number;
  /** Income less expense to the date, which no entry has closed into equity. */
  resultCents: number;
  liabilityEquityCents: number;
  differenceCents: number;
  balances: boolean;
}

/** Which side of the ledger an ageing is of. Stated, never guessed: the two are
 *  chased by different people. */
export type AgedSide = "receivable" | "payable";

/** The five bands, in the accounting currency. */
export interface AgedBuckets {
  currentCents: number;
  d1_30Cents: number;
  d31_60Cents: number;
  d61_90Cents: number;
  d90_plusCents: number;
  totalCents: number;
}

/** One document still open on the day the listing stands. */
export interface AgedDocument {
  documentId: string;
  number: string;
  issueDate: string;
  dueDate: string;
  daysOverdue: number;
  bucket: string;
  currency: string;
  openCents: number;
  /** The same amount in the accounting currency, or `null` when it cannot be
   *  restated honestly — such a document is in no band. */
  baseOpenCents: number | null;
  creditNote: boolean;
}

/** One counterparty's ageing, and the documents behind it. */
export interface AgedParty {
  partyId: string;
  name: string;
  buckets: AgedBuckets;
  unconvertedCount: number;
  documents: AgedDocument[];
}

/** Who owes us, or whom we owe, by how overdue it is. */
export interface AgedReport {
  on: string;
  side: AgedSide;
  currency: string;
  parties: AgedParty[];
  buckets: AgedBuckets;
  /** Documents in no band at all, because they could not be restated. */
  unconvertedCount: number;
  documentCount: number;
}

/** One rate of one side of the VAT return. */
export interface VatReturnRate {
  rateBp: number;
  baseCents: number;
  vatCents: number;
}

/** One side of the return: the rates, their totals, and what is on neither. */
export interface VatReturnSide {
  rates: VatReturnRate[];
  baseCents: number;
  vatCents: number;
  unratedBaseCents: number;
  unratedVatCents: number;
}

/** The figures a VAT return is filed from — the **journal's**, which is what
 *  makes them include the purchase side no invoice of ours knows about.
 *
 *  `netPayableCents` is positive when the tenant owes the authority and
 *  negative when it is owed a refund; the screen says which in words. */
export interface VatReturn {
  from: string;
  to: string;
  currency: string;
  output: VatReturnSide;
  input: VatReturnSide;
  netPayableCents: number;
}
