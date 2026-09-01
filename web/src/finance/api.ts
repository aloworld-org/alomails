// The client for the `/finance` HTTP surface (alo Finance, ADR 0035, wave B4).
//
// Its own small client, for the same reason Billing, CRM and Projects each have
// one: `/finance` is a plain REST surface with none of JMAP's session,
// capabilities or method-call envelope. It uses the same authenticated fetch
// (bearer + refresh handled by the auth layer), so there is one session and not
// four, and it fails through the shared `platform/rest` shape so a server
// sentence reaches a user the same way in every module.
//
// It holds NO validation, NO arithmetic and NO money. Amounts are integer cents
// in and integer cents out; the net of a claim, the state it is in and whether
// it may still be edited all arrive from the server. The screens send what was
// typed and show what came back (`docs/design/finance.md`).
//
// Two doors, and the split is the module's whole tenancy story:
//
// - The **claimant's** methods carry no user id anywhere. A person's claims name
//   restaurants, pharmacies and occasions; the account door binds the user on
//   every statement, so reaching a colleague's claims is unrepresentable here
//   rather than merely refused.
// - The **approver's** methods are cross-user and say so in their own doc
//   comments. The server gates them on admin-or-accountant and answers `403` —
//   this client never decides who may call them, it only reports what happened.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type {
  AccountDraft,
  AgedReport,
  AgedSide,
  BalanceSheet,
  BankImportOptions,
  BankImportReport,
  BankLine,
  BankLineStatus,
  BankStatement,
  BankSuggestions,
  CashForecast,
  Chart,
  ChartAccount,
  ConfirmedMatch,
  Expense,
  ExpenseDraft,
  ExpenseStatus,
  PendingExpense,
  PlReport,
  VatReturn,
} from "./types";

/** The two days a report is asked over, as the server takes them. Both are
 *  required there — a report that quietly defaulted to a period would put a
 *  figure under a heading nobody asked for — so neither is defaulted here. */
function period(from: string, to: string): string {
  return new URLSearchParams({ from, to }).toString();
}

/** One day, encoded for a query string. */
function day(on: string): string {
  return encodeURIComponent(on);
}

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed Finance request, carrying the server's own `Problem` detail. */
export class FinanceError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "FinanceError");
  }
}

/**
 * The import refused, **with the report that says why**.
 *
 * A file with one unreadable row imports nothing, and the server answers `422`
 * carrying the same report a preview would have shown — naming every broken
 * line and the rule it broke. A refusal a person cannot act on is the one thing
 * an importer must never answer (`finance_bank.rs`), so the client keeps the
 * report on the error rather than reducing the whole thing to a sentence.
 */
export class BankImportRefused extends FinanceError {
  readonly report: BankImportReport;

  constructor(status: number, detail: string | null, report: BankImportReport) {
    super(status, detail);
    this.name = "BankImportRefused";
    this.report = report;
  }
}

/** The upload's query string: the account, the two conventions and which column
 *  of the file is which. A blank value is an unstated one — the server reads ""
 *  as "not mapped", and sending it would be mapping a column called "". */
function importQuery(options: BankImportOptions): string {
  const query = new URLSearchParams();
  const put = (key: string, value: string | null | undefined) => {
    const stated = (value ?? "").trim();
    if (stated !== "") query.set(key, stated);
  };
  put("format", options.format);
  put("account", options.account);
  put("currency", options.currency);
  put("dates", options.dates);
  put("decimal", options.decimal);
  const mapping = options.mapping ?? {};
  put("date", mapping.date);
  put("valueDate", mapping.valueDate);
  put("amount", mapping.amount);
  put("debit", mapping.debit);
  put("credit", mapping.credit);
  put("sign", mapping.sign);
  put("currencyColumn", mapping.currencyColumn);
  put("counterparty", mapping.counterparty);
  put("iban", mapping.iban);
  put("remittance", mapping.remittance);
  put("reference", mapping.reference);
  const rendered = query.toString();
  return rendered === "" ? "" : `?${rendered}`;
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function financeMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** The caller's own claims, and — on the approver's door — the two queues a
 *  decision empties. One instance per auth context. */
export class FinanceApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  // ---- the claimant's own claims -----------------------------------------

  /** The caller's own claims in an inclusive `YYYY-MM-DD` period, newest
   *  purchase first. `status` narrows it to one state; the server refuses a
   *  period longer than a year rather than truncating it. */
  expenses(from: string, to: string, status?: ExpenseStatus): Promise<Expense[]> {
    const query = new URLSearchParams({ from, to });
    if (status !== undefined) query.set("status", status);
    return this.#read<{ expenses?: Expense[] }>(`/finance/expenses?${query.toString()}`).then(
      (r) => r.expenses ?? [],
    );
  }

  /** Records a claim of the caller's own. It starts as a draft: nothing is in
   *  anybody's queue until it is handed in. */
  createExpense(draft: ExpenseDraft): Promise<Expense> {
    return this.#write<{ expense: Expense }>("POST", "/finance/expenses", draft).then(
      (r) => r.expense,
    );
  }

  /** Corrects a claim that is still the caller's own. A claim somebody is
   *  deciding is frozen — the server answers `409`, and withdrawing is the way
   *  back. */
  updateExpense(id: string, draft: ExpenseDraft): Promise<Expense> {
    return this.#write<{ expense: Expense }>(
      "PATCH",
      `/finance/expenses/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.expense);
  }

  /** Removes a claim nobody has acted on. */
  async deleteExpense(id: string): Promise<void> {
    await this.#discard(`/finance/expenses/${encodeURIComponent(id)}`);
  }

  /** Hands a claim in for a decision, which freezes it. */
  submitExpense(id: string): Promise<Expense> {
    return this.#act(id, "submit");
  }

  /** Takes a claim back out of the queue so it can be corrected. */
  withdrawExpense(id: string): Promise<Expense> {
    return this.#act(id, "withdraw");
  }

  /** Agrees with the category the agent suggested: it becomes the claim's own
   *  (B4.14a). The server applies every rule picking one by hand is subject to,
   *  so this can refuse — with its own sentence. */
  acceptExpenseCategory(id: string): Promise<Expense> {
    return this.#act(id, "category/accept");
  }

  /** Declines it. Nothing suggests a category for that claim again; the person
   *  can still pick one themselves. */
  declineExpenseCategory(id: string): Promise<Expense> {
    return this.#act(id, "category/decline");
  }

  // ---- the approver's door -----------------------------------------------

  /** **Admin or accountant:** every claim of this tenant awaiting a decision,
   *  oldest purchase first. Anybody else gets the server's `403`. */
  pendingExpenses(): Promise<PendingExpense[]> {
    return this.#read<{ expenses?: PendingExpense[] }>("/finance/expenses/pending").then(
      (r) => r.expenses ?? [],
    );
  }

  /** **Admin or accountant:** every claim this tenant has approved and still
   *  owes an employee for, oldest decision first. A claim a company card paid is
   *  approved and is not here — nobody is owed anything on it. */
  reimbursableExpenses(): Promise<PendingExpense[]> {
    return this.#read<{ expenses?: PendingExpense[] }>("/finance/expenses/reimbursable").then(
      (r) => r.expenses ?? [],
    );
  }

  /** **Admin or accountant:** the cost is the company's. */
  approveExpense(id: string, note?: string): Promise<Expense> {
    return this.#act(id, "approve", note === undefined || note === "" ? {} : { note });
  }

  /** **Admin or accountant:** the claim goes back to its claimant, editable, so
   *  they can correct it and hand it in again. The note is what they read. */
  rejectExpense(id: string, note?: string): Promise<Expense> {
    return this.#act(id, "reject", note === undefined || note === "" ? {} : { note });
  }

  /** **Admin or accountant:** the money has been paid back. `reimbursedOn` is
   *  the day it actually moved and is required — it is the day the payment
   *  books on, and a day taken from a container's clock is a posting in the
   *  wrong period. */
  reimburseExpense(id: string, reimbursedOn: string): Promise<Expense> {
    return this.#act(id, "reimburse", { reimbursedOn });
  }

  // ---- the bank, and the pile it leaves ----------------------------------
  //
  // **Admin or accountant**, every one of them: a statement is the whole
  // company's money moving past a bookkeeper, and matching a line records a
  // payment and moves the books. The server answers `403` to anybody else, and
  // this client — like the approver's door above — never decides who may call.

  /** What this file **would** stage. Writes nothing, and is the answer a person
   *  corrects their column mapping against before committing to it. */
  previewBankImport(file: Blob, options: BankImportOptions = {}): Promise<BankImportReport> {
    return this.#upload(`/finance/imports/bank/preview${importQuery(options)}`, file);
  }

  /** Stages the file. Duplicate transactions are skipped and counted; a file
   *  with an unreadable row imports **nothing** and throws
   *  {@link BankImportRefused} carrying the report that names the rows. */
  importBankFile(file: Blob, options: BankImportOptions = {}): Promise<BankImportReport> {
    return this.#upload(`/finance/imports/bank${importQuery(options)}`, file);
  }

  /** What has been imported, most recent period first. */
  bankStatements(): Promise<BankStatement[]> {
    return this.#read<{ statements?: BankStatement[] }>("/finance/bank/statements").then(
      (r) => r.statements ?? [],
    );
  }

  /** The staged lines, oldest first — the order a bookkeeper works a month in.
   *  Both narrowings are optional and neither is an existence oracle: an
   *  unknown statement matches nothing and answers an empty list. */
  bankLines(filter: { statement?: string; status?: BankLineStatus } = {}): Promise<BankLine[]> {
    const query = new URLSearchParams();
    if (filter.statement !== undefined) query.set("statement", filter.statement);
    if (filter.status !== undefined) query.set("status", filter.status);
    const rendered = query.toString();
    return this.#read<{ lines?: BankLine[] }>(
      `/finance/bank/lines${rendered === "" ? "" : `?${rendered}`}`,
    ).then((r) => r.lines ?? []);
  }

  /** Every unmatched line with what the two guessing stages think it is. A
   *  read: it writes nothing, and is worth exactly as much as the person
   *  looking at it (ADR 0023). */
  bankSuggestions(statement?: string): Promise<BankSuggestions> {
    const query = statement === undefined ? "" : `?statement=${encodeURIComponent(statement)}`;
    return this.#read<{ suggestions: BankSuggestions }>(
      `/finance/bank/suggestions${query}`,
    ).then((r) => r.suggestions);
  }

  /** **This line settled that invoice.** `amountCents` is what the person saw
   *  attributed on their screen: the server compares it with what the bank said
   *  the line moves rather than believing it, so a stale screen is a refusal
   *  instead of a payment for the wrong money. `ruleId` is the learned rule
   *  whose suggestion they took, when they took one. */
  matchBankLine(
    lineId: string,
    invoiceId: string,
    amountCents: number,
    ruleId?: string | null,
  ): Promise<ConfirmedMatch> {
    const body: { invoiceId: string; amountCents: number; ruleId?: string } = {
      invoiceId,
      amountCents,
    };
    if (ruleId !== undefined && ruleId !== null && ruleId !== "") body.ruleId = ruleId;
    return this.#write<{ match: ConfirmedMatch }>(
      "POST",
      `/finance/bank/lines/${encodeURIComponent(lineId)}/match`,
      body,
    ).then((r) => r.match);
  }

  /** Takes a match back: the payment goes, the entry is reversed by an entry of
   *  its own, and the line returns to the pile. */
  async unmatchBankLine(lineId: string): Promise<void> {
    await this.#write<unknown>(
      "POST",
      `/finance/bank/lines/${encodeURIComponent(lineId)}/unmatch`,
      {},
    );
  }

  /** **This line is not ours to book**, with the reason. The reason is required
   *  by the server: a line set aside without one is a line the next person
   *  cannot judge. */
  ignoreBankLine(lineId: string, reason: string): Promise<BankLine> {
    return this.#write<{ line: BankLine }>(
      "POST",
      `/finance/bank/lines/${encodeURIComponent(lineId)}/ignore`,
      { reason },
    ).then((r) => r.line);
  }

  /** Back in the pile, with the reason cleared. */
  unignoreBankLine(lineId: string): Promise<BankLine> {
    return this.#write<{ line: BankLine }>(
      "POST",
      `/finance/bank/lines/${encodeURIComponent(lineId)}/unignore`,
      {},
    ).then((r) => r.line);
  }

  cashForecast(options: { on: string; horizon: 30 | 60 | 90; receivableDelay?: number; payableDelay?: number }): Promise<CashForecast> {
    const query = new URLSearchParams({ on: options.on, horizon: String(options.horizon) });
    query.set("receivableDelay", String(options.receivableDelay ?? 0));
    query.set("payableDelay", String(options.payableDelay ?? 0));
    return this.#read<{ forecast: CashForecast }>(`/finance/forecast?${query.toString()}`).then((response) => response.forecast);
  }

  // ---- the chart of accounts ---------------------------------------------
  //
  // **Admin or accountant**, reads included: the chart says what the company
  // owes, is owed and earns, and the list is also what seeds it on first use.

  /**
   * The tenant's chart in code order, **seeding the default one on first use**.
   *
   * It carries the language for that reason and only that one: the server
   * writes the twenty default accounts once, in the language of whoever opened
   * the chart first, and they are ordinary tenant data from that moment on
   * (`finance_chart_names.rs`).
   *
   * `from`/`to` add each account's movement over that period, folded from the
   * journal by the server. Both or neither — the server refuses half a period
   * rather than folding open-ended.
   */
  chart(options: { includeInactive?: boolean; from?: string; to?: string } = {}): Promise<Chart> {
    const query = new URLSearchParams({ lang: getLocale() });
    if (options.includeInactive === true) query.set("includeInactive", "true");
    if (options.from !== undefined && options.from !== "") query.set("from", options.from);
    if (options.to !== undefined && options.to !== "") query.set("to", options.to);
    return this.#read<{ accounts?: ChartAccount[]; seeded?: boolean; currency?: string | null }>(
      `/finance/accounts?${query.toString()}`,
    ).then((r) => ({
      accounts: r.accounts ?? [],
      seeded: r.seeded === true,
      currency: r.currency ?? null,
    }));
  }

  /** Adds the tenant's own line to their own chart. */
  createAccount(draft: AccountDraft): Promise<ChartAccount> {
    return this.#write<{ account: ChartAccount }>("POST", "/finance/accounts", draft).then(
      (r) => r.account,
    );
  }

  /**
   * Renames, recodes, reclassifies, rehooks or retires an account — a seeded
   * one included, because a tenant whose accountant wants `1400` for
   * receivables must be able to say so.
   *
   * Only what is sent changes. The posting rules follow the account's role, so
   * a code is safe to change and a role is not something a rename may drop.
   */
  updateAccount(id: string, draft: AccountDraft): Promise<ChartAccount> {
    return this.#write<{ account: ChartAccount }>(
      "PATCH",
      `/finance/accounts/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.account);
  }

  /** Removes a custom account that never carried a posting. The server answers
   *  `409` for a seeded one and for one with history — deactivating is what a
   *  tenant who has stopped using an account actually wants. */
  async deleteAccount(id: string): Promise<void> {
    await this.#discard(`/finance/accounts/${encodeURIComponent(id)}`);
  }

  // ---- the four reports --------------------------------------------------
  //
  // Each is served twice by the server — the JSON for the screen and a `.csv`
  // of the same read for the accountant's own tooling — so a file somebody
  // opens and a table somebody is looking at cannot disagree about a cent.
  // Both are fetched with the session's token: the routes are authenticated,
  // and a plain `<a href>` would download a `401`.

  /** What the business earned and spent between two days, with the comparative
   *  period the server chose beside every figure. */
  plReport(from: string, to: string): Promise<PlReport> {
    return this.#read<{ report: PlReport }>(`/finance/reports/pl?${period(from, to)}`).then(
      (r) => r.report,
    );
  }

  /** The same report as a file. */
  plReportCsv(from: string, to: string): Promise<string> {
    return this.#text(`/finance/reports/pl.csv?${period(from, to)}`);
  }

  /** What the business owns, owes and is worth on one day. */
  balanceSheet(on: string): Promise<BalanceSheet> {
    return this.#read<{ report: BalanceSheet }>(`/finance/reports/balance?on=${day(on)}`).then(
      (r) => r.report,
    );
  }

  /** The same sheet as a file. */
  balanceSheetCsv(on: string): Promise<string> {
    return this.#text(`/finance/reports/balance.csv?on=${day(on)}`);
  }

  /** Who owes us, or whom we owe, by how overdue it is. The side is required by
   *  the server: defaulting it would put what we owe under a heading that says
   *  what we are owed. */
  agedReport(on: string, side: AgedSide): Promise<AgedReport> {
    return this.#read<{ report: AgedReport }>(
      `/finance/reports/aged?on=${day(on)}&side=${side}`,
    ).then((r) => r.report);
  }

  /** The same listing as a file. */
  agedReportCsv(on: string, side: AgedSide): Promise<string> {
    return this.#text(`/finance/reports/aged.csv?on=${day(on)}&side=${side}`);
  }

  /** The VAT-return figures — the journal's, including the purchase side no
   *  invoice of ours knows about. */
  vatReturn(from: string, to: string): Promise<VatReturn> {
    return this.#read<{ report: VatReturn }>(`/finance/reports/vat?${period(from, to)}`).then(
      (r) => r.report,
    );
  }

  /** The same return as a file. */
  vatReturnCsv(from: string, to: string): Promise<string> {
    return this.#text(`/finance/reports/vat.csv?${period(from, to)}`);
  }

  // ---- plumbing ----------------------------------------------------------

  /** A file as the body — the shape both import doors take, because what a
   *  person has is a file and escaping a spreadsheet into a JSON string first
   *  would be a worse surface for no gain. */
  async #upload(path: string, file: Blob): Promise<BankImportReport> {
    const res = await this.#send(path, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: file,
    });
    if (res.ok) return ((await res.json()) as { import: BankImportReport }).import;
    throw await importFailure(res);
  }

  /** One verb on one claim. Every one of them answers with the claim as it now
   *  stands, so a screen never has to guess what a decision did to it. */
  #act(id: string, verb: string, body: unknown = {}): Promise<Expense> {
    return this.#write<{ expense: Expense }>(
      "POST",
      `/finance/expenses/${encodeURIComponent(id)}/${verb}`,
      body,
    ).then((r) => r.expense);
  }

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "GET" }));
  }

  /** A response whose body is a file rather than a resource — a report's CSV.
   *  Read as text and handed to the caller, who saves it under the name the
   *  server would have used. */
  async #text(path: string): Promise<string> {
    const res = await this.#send(path, { method: "GET" });
    if (!res.ok) throw await failure(res);
    return res.text();
  }

  async #write<T>(method: string, path: string, body: unknown): Promise<T> {
    return this.#json<T>(
      await this.#send(path, {
        method,
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      }),
    );
  }

  /** A `DELETE`, whose success is `204` and therefore has no body to read. */
  async #discard(path: string): Promise<void> {
    const res = await this.#send(path, { method: "DELETE" });
    if (!res.ok) throw await failure(res);
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new FinanceError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) throw await failure(res);
    return (await res.json()) as T;
  }
}

/** The refusal a failed response carries: the server's own sentence, verbatim,
 *  or nothing when the body was not the `Problem` shape. */
async function failure(res: Response): Promise<FinanceError> {
  return new FinanceError(res.status, await problemDetail(res));
}

/** The refusal an upload carries. The body is read **once**, so the report and
 *  the sentence come out of the same read: a second `res.json()` on a consumed
 *  body would throw and lose both. */
async function importFailure(res: Response): Promise<FinanceError> {
  const body = (await res.json().catch(() => ({}))) as {
    detail?: unknown;
    import?: unknown;
  };
  const detail = typeof body.detail === "string" ? body.detail : null;
  // The report rides on a `422` and on nothing else — a `403`, a `413` or a
  // proxy's HTML page has none, and inventing an empty one would tell a person
  // their file was read when it never was.
  return typeof body.import === "object" && body.import !== null
    ? new BankImportRefused(res.status, detail, body.import as BankImportReport)
    : new FinanceError(res.status, detail);
}

/** The Finance client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useFinanceApi(): FinanceApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new FinanceApi(authorizedFetch), [authorizedFetch]);
}
