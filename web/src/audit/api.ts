// The client for `GET /audit` (alo audit trail, ADR 0035, wave B2.13).
//
// Its own tiny client rather than a method on the billing or CRM one, because
// the trail belongs to neither: a record's history is asked for the same way
// whatever module the record lives in, and the next module to grow a history
// tab should not have to add a third copy of this call.
//
// It reads and never writes — there is no way to add an entry from a browser,
// which is the point. Entries appear because something happened.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type { AuditEntry, AuditSubject } from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed audit request, carrying the server's own `Problem` detail. */
export class AuditError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "AuditError");
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function auditMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** One record's history, newest first. */
export class AuditApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /**
   * The acts recorded against one record, newest first.
   *
   * A record nobody has touched — and a record belonging to another tenant —
   * both answer with an empty list. The screen shows "nothing yet" for both,
   * which is the only thing it could honestly say about either.
   */
  async history({ entityType, entityId }: AuditSubject, limit = 50): Promise<AuditEntry[]> {
    const query = new URLSearchParams({
      entity: `${entityType}:${entityId}`,
      limit: String(limit),
    });
    let res: Response;
    try {
      res = await this.#fetch(`${API_BASE}/api/audit?${query.toString()}`, { method: "GET" });
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new AuditError(0, null);
    }
    if (!res.ok) throw new AuditError(res.status, await problemDetail(res));
    const body = (await res.json()) as { entries?: AuditEntry[] };
    return body.entries ?? [];
  }
}

/** The audit client bound to the current session. Memoized per auth context, so
 *  a re-render never re-creates it and effects keyed on it do not loop. */
export function useAuditApi(): AuditApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new AuditApi(authorizedFetch), [authorizedFetch]);
}
