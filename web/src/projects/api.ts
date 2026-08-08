// The client for the `/projects` HTTP surface (alo Projects, ADR 0035, wave
// B3).
//
// Its own small client, for the same reason Billing and CRM each have one:
// Projects is a plain REST surface with none of JMAP's session, capabilities or
// method-call envelope. It uses the same authenticated fetch (bearer + refresh
// handled by the auth layer), so there is one session and not three, and it
// fails through the shared `platform/rest` shape so a server sentence reaches a
// user the same way in every module.
//
// It holds NO validation, NO arithmetic and NO money. Durations are integer
// minutes in and integer minutes out; rates and budgets are integer cents the
// server stored; every *sum* — a week's total, a project's hours, a budget
// proportion — is read from the server's answer. The screens send what was
// typed and show what came back (`docs/design/projects.md`).
//
// One shape is deliberately not here: nothing on the personal surface takes a
// user id. A person's hours are personal data and the account door binds the
// user on every statement, so reaching a colleague's diary is unrepresentable
// rather than merely refused. The one cross-person read — the approvals inbox —
// is on the admin door and says so in its own method.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { API_BASE } from "../platform/runtime";
import { RestError, restMessage } from "../platform/rest";
import type {
  PendingWeek,
  Project,
  ProjectClient,
  ProjectClientDraft,
  RunningTimer,
  TimeEntry,
  TimeEntryDraft,
  TimeTotals,
  TimesheetWeek,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed Projects request, carrying the server's own `Problem` detail. */
export class ProjectsError extends RestError {
  /** The timer that was already running, when the server refused a start
   *  because of it (`409`). The refusal carries it so the widget can offer to
   *  stop that one rather than ask the user what happened. */
  readonly runningTimer: RunningTimer | null;

  constructor(status: number, detail: string | null, runningTimer: RunningTimer | null = null) {
    super(status, detail, "ProjectsError");
    this.runningTimer = runningTimer;
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function projectsMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** What a stop of the clock produced: the entry it wrote, how long it really
 *  ran, and whether that had to be capped. */
export interface StoppedTimer {
  entry: TimeEntry;
  elapsedMinutes: number;
  /** True when a clock ran past a full day and the entry was written at the
   *  ceiling — somebody went home without stopping it, and `elapsedMinutes`
   *  says how long it really ran so they can correct it. */
  cappedAtDayLimit: boolean;
}

/** The caller's own hours over a period, with the server's totals beside
 *  them. */
export interface TimePeriod {
  entries: TimeEntry[];
  totals: TimeTotals;
}

/** The tenant's engagements, the caller's own hours and weeks, and — on the
 *  admin door — the weeks waiting for a decision. One instance per auth
 *  context. */
export class ProjectsApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  // ---- engagements -------------------------------------------------------

  /** Every project this caller can see, as client work: the board, its client
   *  facts (`null` for internal work) and its hours to date. */
  projects(): Promise<Project[]> {
    return this.#read<{ projects?: Project[] }>("/projects").then((r) => r.projects ?? []);
  }

  /** One engagement. Another tenant's project, a colleague's private board and
   *  an id that never existed are all the same `404`. */
  project(id: string): Promise<Project> {
    return this.#read<{ project: Project }>(`/projects/${encodeURIComponent(id)}`).then(
      (r) => r.project,
    );
  }

  /** Makes a project client work, or replaces the facts that already say so.
   *  Idempotent and whole-record: what the form does not state is cleared. */
  setClient(projectId: string, draft: ProjectClientDraft): Promise<ProjectClient> {
    return this.#write<{ client: ProjectClient }>(
      "PUT",
      `/projects/clients/${encodeURIComponent(projectId)}`,
      draft,
    ).then((r) => r.client);
  }

  /** Makes a project internal work again. The hours stay — what is deleted is
   *  the claim that they are billable to somebody. */
  async clearClient(projectId: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/projects/clients/${encodeURIComponent(projectId)}`, { method: "DELETE" }),
    );
  }

  // ---- the clock ---------------------------------------------------------

  /** The caller's running clock, or `null`. "No timer" is the ordinary state of
   *  a workspace, so it is an answer and not a refusal. */
  timer(): Promise<RunningTimer | null> {
    return this.#read<{ timer: RunningTimer | null }>("/projects/timer").then((r) => r.timer);
  }

  /** Starts the clock on a project. Starting while one runs is a `409` carrying
   *  the running timer on [`ProjectsError.runningTimer`] — never an implicit
   *  stop, because stopping writes a billable fact nobody asked for. */
  startTimer(start: {
    projectId: string;
    taskId?: string;
    billable?: boolean;
    note?: string;
  }): Promise<RunningTimer> {
    return this.#write<{ timer: RunningTimer }>("POST", "/projects/timer/start", start).then(
      (r) => r.timer,
    );
  }

  /** Stops the clock, which is what writes the entry. `workDate` is the day in
   *  the worker's own zone; absent, the server falls back to the day the clock
   *  started — never to the server's own idea of today. */
  stopTimer(workDate?: string): Promise<StoppedTimer> {
    return this.#write<StoppedTimer>(
      "POST",
      "/projects/timer/stop",
      workDate === undefined ? {} : { workDate },
    );
  }

  // ---- the caller's own hours -------------------------------------------

  /** The caller's own entries in an inclusive `YYYY-MM-DD` range, with the
   *  server's minute totals. */
  time(from: string, to: string, projectId?: string): Promise<TimePeriod> {
    const query = new URLSearchParams({ from, to });
    if (projectId !== undefined && projectId !== "") query.set("projectId", projectId);
    return this.#read<TimePeriod>(`/projects/time?${query.toString()}`);
  }

  /** Records work done away from the clock. The answer is the stored record —
   *  including the rate the server snapshotted, which the caller could not have
   *  computed. */
  logTime(draft: TimeEntryDraft): Promise<TimeEntry> {
    return this.#write<{ entry: TimeEntry }>("POST", "/projects/time", draft).then((r) => r.entry);
  }

  /** Corrects one of the caller's own entries, while its week is unlocked. A
   *  whole record: the server's edit shape is "the entry now says this". */
  updateTime(id: string, draft: TimeEntryDraft): Promise<TimeEntry> {
    return this.#write<{ entry: TimeEntry }>(
      "PATCH",
      `/projects/time/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.entry);
  }

  /** Removes one of the caller's own entries. */
  async deleteTime(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/projects/time/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  // ---- the caller's own weeks -------------------------------------------

  /** The caller's own weeks and their status over a range. A week nobody has
   *  submitted has no row yet and is simply absent — `open` is the default the
   *  screen draws, not a record the server invents. */
  weeks(from: string, to: string): Promise<TimesheetWeek[]> {
    const query = new URLSearchParams({ from, to });
    return this.#read<{ weeks?: TimesheetWeek[] }>(`/projects/weeks?${query.toString()}`).then(
      (r) => r.weeks ?? [],
    );
  }

  /** Hands a week to an approver. Addressed by its **Monday**, because a week
   *  nobody has submitted has no id to address. */
  submitWeek(monday: string): Promise<TimesheetWeek> {
    return this.#write<{ week: TimesheetWeek }>(
      "POST",
      `/projects/weeks/${encodeURIComponent(monday)}/submit`,
      {},
    ).then((r) => r.week);
  }

  /** Takes a week back while nobody has decided about it. */
  withdrawWeek(monday: string): Promise<TimesheetWeek> {
    return this.#write<{ week: TimesheetWeek }>(
      "POST",
      `/projects/weeks/${encodeURIComponent(monday)}/withdraw`,
      {},
    ).then((r) => r.week);
  }

  // ---- the admin door ----------------------------------------------------

  /** **Admin only:** every submitted week of every user, oldest first. A caller
   *  who is not a tenant admin gets a `403` — the one place in this module a
   *  refusal is not a `404`, because the inbox's existence is not a secret. */
  approvals(): Promise<PendingWeek[]> {
    return this.#read<{ weeks?: PendingWeek[] }>("/projects/approvals").then((r) => r.weeks ?? []);
  }

  /** **Admin only:** approves a submitted week. Its hours stay locked and
   *  become billable. */
  approveWeek(id: string, note?: string): Promise<TimesheetWeek> {
    return this.#decide(id, "approve", note);
  }

  /** **Admin only:** rejects a submitted week, which unlocks it so the person
   *  can correct it. The note is what they will read. */
  rejectWeek(id: string, note?: string): Promise<TimesheetWeek> {
    return this.#decide(id, "reject", note);
  }

  /** **Admin only:** the way back from a decision — reopens an approved week
   *  so it can be corrected and submitted again. */
  reopenWeek(id: string): Promise<TimesheetWeek> {
    return this.#decide(id, "reopen");
  }

  #decide(id: string, verb: string, note?: string): Promise<TimesheetWeek> {
    return this.#write<{ week: TimesheetWeek }>(
      "POST",
      `/projects/approvals/${encodeURIComponent(id)}/${verb}`,
      note === undefined || note === "" ? {} : { note },
    ).then((r) => r.week);
  }

  // ---- plumbing ----------------------------------------------------------

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

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new ProjectsError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) {
      // A `409` from the start route carries the running timer beside the
      // sentence. Read once: the body is consumed either way, and a failure to
      // find the extra leaves the plain refusal intact rather than becoming a
      // second, worse failure.
      const body = (await res.json().catch(() => ({}))) as {
        detail?: unknown;
        timer?: RunningTimer | null;
      };
      const detail = typeof body.detail === "string" ? body.detail : null;
      throw new ProjectsError(res.status, detail, body.timer ?? null);
    }
    return (await res.json()) as T;
  }
}

/** The Projects client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useProjectsApi(): ProjectsApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new ProjectsApi(authorizedFetch), [authorizedFetch]);
}
