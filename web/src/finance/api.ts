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
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type { Expense, ExpenseDraft, ExpenseStatus, PendingExpense } from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed Finance request, carrying the server's own `Problem` detail. */
export class FinanceError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "FinanceError");
  }
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

  // ---- plumbing ----------------------------------------------------------

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
      return await this.#fetch(`${API_BASE}${path}`, init);
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

/** The Finance client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useFinanceApi(): FinanceApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new FinanceApi(authorizedFetch), [authorizedFetch]);
}
