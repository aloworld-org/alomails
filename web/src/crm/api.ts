// The client for the `/crm` HTTP surface (alo CRM, ADR 0035, wave B2).
//
// Its own small client, for the same reason billing has one: CRM is a plain
// REST surface with none of JMAP's session, capabilities or method-call
// envelope. It uses the same authenticated fetch (bearer + refresh handled by
// the auth layer), so there is one session, not two, and it fails through the
// shared `platform/rest` shape so a server sentence reaches a user the same way
// in both modules.
//
// It holds NO validation and NO arithmetic. Titles, currencies, values, lost
// reasons, filters and dates are all ruled on by the store — which the CRM
// agent (B2.10) also calls directly — and every *sum* over money is the
// server's (`docs/design/crm.md`). The screens send what was typed and show
// what came back.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import type { Task } from "../jmap";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type {
  ActivityKind,
  CrmDeal,
  CrmPipeline,
  CrmStage,
  DealActivity,
  DealBillingDocument,
  DealDraft,
  DealFilter,
  DealHandoff,
  DealProject,
  MailOpportunityDraft,
  DealThread,
  PipelineReport,
  RaisedDocument,
  ThreadSuggestion,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed CRM request, carrying the server's own `Problem` detail. */
export class CrmError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "CrmError");
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function crmMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** The query string a report asks with. All three are required by the server: a
 *  report that quietly defaulted to a period would put a figure under a heading
 *  nobody asked for. */
function reportQuery(pipelineId: string, from: string, to: string): string {
  return new URLSearchParams({ pipelineId, from, to }).toString();
}

/** A move: where the card lands, and — for a losing column only — why. */
export interface StageMove {
  stageId: string;
  position?: number;
  lostReason?: string;
}

/** The tenant's boards, columns and deals, and the log, next steps and
 *  conversations hanging off one. One instance per auth context. */
export class CrmApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /**
   * The tenant's boards, active first.
   *
   * This read is also what **seeds** a tenant's first board and its columns, in
   * the language the client asks for — so `?lang=` is sent from the interface
   * language, which is the only language anybody is actually looking at. The
   * names are ordinary user data from that moment on; `?lang=` on any later
   * read does nothing at all.
   */
  pipelines(): Promise<CrmPipeline[]> {
    return this.#read<{ pipelines?: CrmPipeline[] }>(
      `/crm/pipelines?lang=${encodeURIComponent(getLocale())}`,
    ).then((r) => r.pipelines ?? []);
  }

  /** The columns of one board, left to right. */
  stages(pipelineId: string): Promise<CrmStage[]> {
    return this.#read<{ stages?: CrmStage[] }>(
      `/crm/pipelines/${encodeURIComponent(pipelineId)}/stages`,
    ).then((r) => r.stages ?? []);
  }

  /**
   * The tenant's deals in board order, column by column, card by card.
   *
   * Every filter is sent to the server, which resolves the board and column ids
   * and refuses an unknown one with a `422` — a sales manager reading
   * "everything" when they asked for "mine" is a wrong number on a screen.
   */
  deals(filter: DealFilter = {}): Promise<CrmDeal[]> {
    const query = new URLSearchParams();
    if (filter.pipelineId !== undefined && filter.pipelineId !== "") {
      query.set("pipelineId", filter.pipelineId);
    }
    if (filter.stageId !== undefined && filter.stageId !== "") query.set("stageId", filter.stageId);
    if (filter.ownerUserId !== undefined && filter.ownerUserId !== "") {
      query.set("ownerUserId", filter.ownerUserId);
    }
    if (filter.state !== undefined) query.set("state", filter.state);
    const suffix = query.toString() === "" ? "" : `?${query.toString()}`;
    return this.#read<{ deals?: CrmDeal[] }>(`/crm/deals${suffix}`).then((r) => r.deals ?? []);
  }

  /** One deal, as it stands. A deal of another tenant is the same `404` an id
   *  that never existed gets. */
  deal(id: string): Promise<CrmDeal> {
    return this.#read<{ deal: CrmDeal }>(`/crm/deals/${encodeURIComponent(id)}`).then((r) => r.deal);
  }

  /** Raises a card in a column. The board and the column are required: a deal
   *  that names no board is not a deal. It is always created **open**, whatever
   *  the column's flags — closing is a move. */
  createDeal(draft: DealDraft): Promise<CrmDeal> {
    return this.#write<{ deal: CrmDeal }>("POST", "/crm/deals", draft).then((r) => r.deal);
  }

  /** Raises an opportunity from Mail with its source conversation linked in
   * the same server transaction. */
  createDealFromMail(draft: MailOpportunityDraft): Promise<CrmDeal> {
    return this.#write<{ deal: CrmDeal }>("POST", "/crm/deals", draft).then(
      (result) => result.deal,
    );
  }

  /** Edits a deal. Absent fields keep their stored value; `null` clears a
   *  nullable one. It cannot move, reposition or close the card. */
  updateDeal(id: string, draft: DealDraft): Promise<CrmDeal> {
    return this.#write<{ deal: CrmDeal }>(
      "PATCH",
      `/crm/deals/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.deal);
  }

  /**
   * Moves a card, in one server transaction that also writes the history row
   * and — when the column is flagged — the closing snapshot.
   *
   * A losing column requires a reason and every other column refuses one, so
   * the caller asks for it *before* the drag is committed.
   */
  moveDeal(id: string, move: StageMove): Promise<CrmDeal> {
    return this.#write<{ deal: CrmDeal }>(
      "POST",
      `/crm/deals/${encodeURIComponent(id)}/stage`,
      move,
    ).then((r) => r.deal);
  }

  /** Deletes a deal raised by mistake — the one CRM record that is removed
   *  rather than archived. Its log goes with it; its next steps stay in the
   *  task lists of the people who have to do them. */
  async deleteDeal(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/crm/deals/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /** One deal's log, by when each entry **happened**, newest first. */
  activities(dealId: string): Promise<DealActivity[]> {
    return this.#read<{ activities?: DealActivity[] }>(
      `/crm/deals/${encodeURIComponent(dealId)}/activities`,
    ).then((r) => r.activities ?? []);
  }

  /** Writes one entry. `happenedAt` is optional — an entry nobody dated
   *  happened now — and is an RFC 3339 instant when stated. */
  addActivity(
    dealId: string,
    entry: { kind: ActivityKind; body: string; happenedAt?: string },
  ): Promise<DealActivity> {
    return this.#write<{ activity: DealActivity }>(
      "POST",
      `/crm/deals/${encodeURIComponent(dealId)}/activities`,
      entry,
    ).then((r) => r.activity);
  }

  /** Removes an entry. Only its author may: anybody else is a `403`, shown as
   *  the server's own sentence rather than hidden as a `404` — the log is
   *  readable tenant-wide, so pretending the row is not there would be
   *  theatre. */
  async deleteActivity(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/crm/activities/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /** The deal's next steps as **this** reader may see them: the ones on team
   *  projects, plus anything assigned to them. They are real tasks. */
  nextSteps(dealId: string): Promise<Task[]> {
    return this.#read<{ nextSteps?: Task[] }>(
      `/crm/deals/${encodeURIComponent(dealId)}/next-steps`,
    ).then((r) => r.nextSteps ?? []);
  }

  /** Agrees what happens next, as a real task in the caller's own list (or the
   *  project they name). The source link back to the deal is written by the
   *  server, never by the caller. */
  addNextStep(dealId: string, step: { title: string; dueAt?: string }): Promise<Task> {
    return this.#write<{ nextStep: Task }>(
      "POST",
      `/crm/deals/${encodeURIComponent(dealId)}/next-steps`,
      step,
    ).then((r) => r.nextStep);
  }

  /** The conversations linked to a deal, most recently linked first. */
  threads(dealId: string): Promise<DealThread[]> {
    return this.#read<{ threads?: DealThread[] }>(
      `/crm/deals/${encodeURIComponent(dealId)}/threads`,
    ).then((r) => r.threads ?? []);
  }

  /** Candidate conversations, computed over the **requesting user's own** mail
   *  and never linked by the computing: each carries the address that matched
   *  and why, so the confirmation that follows is an informed one. */
  threadSuggestions(dealId: string): Promise<ThreadSuggestion[]> {
    return this.#read<{ suggestions?: ThreadSuggestion[] }>(
      `/crm/deals/${encodeURIComponent(dealId)}/thread-suggestions`,
    ).then((r) => r.suggestions ?? []);
  }

  /** Links a conversation the caller can already see. Idempotent: linking one
   *  twice is the same link. */
  linkThread(dealId: string, threadId: string): Promise<DealThread> {
    return this.#write<{ thread: DealThread }>(
      "POST",
      `/crm/deals/${encodeURIComponent(dealId)}/threads`,
      { threadId },
    ).then((r) => r.thread);
  }

  /** Removes a link. Open to the whole tenant, because a link left by a
   *  colleague who has since left would otherwise be permanent — and removing
   *  it destroys nothing, the link never held the mail. */
  async unlinkThread(dealId: string, threadId: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(
        `/crm/deals/${encodeURIComponent(dealId)}/threads/${encodeURIComponent(threadId)}`,
        { method: "DELETE" },
      ),
    );
  }

  /**
   * Raises a **draft** quote for a deal (B2.08), and answers it together with
   * the deal — which the server may have changed, because a lead with no
   * customer row gets one and it is written back onto the card.
   *
   * Nothing is issued and nothing is sent. What the document needs that a deal
   * does not carry — the VAT rate of its line, the country of a customer being
   * created — is `handoff`, and which of those is required is the server's
   * rule, answered as a `422` naming it.
   */
  raiseQuote(
    dealId: string,
    handoff: DealHandoff,
  ): Promise<{ quote: RaisedDocument; deal: CrmDeal }> {
    return this.#write(`POST`, `/crm/deals/${encodeURIComponent(dealId)}/quote`, handoff);
  }

  /** Raises a **draft** invoice for a deal. As [`raiseQuote`], and equally a
   *  draft: it consumes no number from the tenant's gapless sequence. */
  raiseInvoice(
    dealId: string,
    handoff: DealHandoff,
  ): Promise<{ invoice: RaisedDocument; deal: CrmDeal }> {
    return this.#write(`POST`, `/crm/deals/${encodeURIComponent(dealId)}/invoice`, handoff);
  }

  /** Billing documents explicitly raised from this opportunity. */
  billingDocuments(dealId: string): Promise<DealBillingDocument[]> {
    return this.#read<{ documents?: DealBillingDocument[] }>(
      `/crm/deals/${encodeURIComponent(dealId)}/documents`,
    ).then((result) => result.documents ?? []);
  }

  /** The delivery project created from this deal, or `null` before handoff. */
  dealProject(dealId: string): Promise<DealProject | null> {
    return this.#read<{ project?: DealProject | null }>(
      `/crm/deals/${encodeURIComponent(dealId)}/project`,
    ).then((result) => result.project ?? null);
  }

  /** Explicitly confirms won-deal conversion. The server makes retries safe. */
  createProject(
    dealId: string,
    draft: { name: string; color?: string; customerId?: string },
  ): Promise<DealProject> {
    return this.#write<{ project: DealProject }>(
      "POST",
      `/crm/deals/${encodeURIComponent(dealId)}/project`,
      draft,
    ).then((result) => result.project);
  }

  /**
   * Value by stage and win/loss for one board (B2.08).
   *
   * Every figure in it is the server's. The two halves are answered
   * differently and the screen says so: the stage rows are the open board as it
   * stands, the outcomes are what closed between the two days.
   */
  pipelineReport(pipelineId: string, from: string, to: string): Promise<PipelineReport> {
    return this.#read<{ report: PipelineReport }>(
      `/crm/reports/pipeline?${reportQuery(pipelineId, from, to)}`,
    ).then((r) => r.report);
  }

  /** The same figures as a CSV file. Fetched rather than linked, because the
   *  route is authenticated: a plain `<a href>` would save a `401` page. */
  pipelineReportCsv(pipelineId: string, from: string, to: string): Promise<string> {
    return this.#text(`/crm/reports/pipeline.csv?${reportQuery(pipelineId, from, to)}`);
  }

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "GET" }));
  }

  /** A `GET` whose body is not JSON. A failure still carries the server's
   *  `Problem` detail, which is JSON — the same error shape as everywhere. */
  async #text(path: string): Promise<string> {
    const res = await this.#send(path, { method: "GET" });
    if (!res.ok) throw new CrmError(res.status, await problemDetail(res));
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

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new CrmError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) throw new CrmError(res.status, await problemDetail(res));
    return (await res.json()) as T;
  }
}

/** The CRM client bound to the current session. Memoized per auth context, so a
 *  re-render never re-creates it and effects keyed on it do not loop. */
export function useCrmApi(): CrmApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new CrmApi(authorizedFetch), [authorizedFetch]);
}
