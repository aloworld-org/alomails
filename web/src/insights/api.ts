// The client for the `/insights` HTTP surface (alo Insights, ADR 0037, wave
// BI1.05).
//
// Its own small client, for the reason billing and CRM each have one: these are
// plain REST routes with none of JMAP's session or method-call envelope. It
// uses the same authenticated fetch, so there is one session and not two, and
// it fails through the shared `platform/rest` shape so a server sentence
// reaches a user the same way in every module.
//
// It holds NO arithmetic and NO validation. Every figure on an Insights screen
// is a `value` the server computed in the same functions the printed invoice
// and the VAT return use (`docs/design/insights.md` § Where money may be added
// up); this file moves JSON, and the screens format what came back.
//
// Only the calls this wave's screens actually make are here — a client method
// nothing calls is a contract nobody checks. Evaluating a bare ad-hoc spec
// (`POST /insights/eval`) is still absent for that reason: the ask has its own
// route, and the builder that would need `eval` is not built.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type {
  AskProposal,
  Dashboard,
  Gallery,
  GalleryEntry,
  Series,
  Tile,
  TileEdit,
  TilePin,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed Insights request, carrying the server's own `Problem` detail. */
export class InsightsError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "InsightsError");
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function insightsMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** A board and the tiles pinned to it, in layout order. */
export interface BoardContents {
  dashboard: Dashboard;
  tiles: Tile[];
}

/** The tenant's boards, the tiles on them, and the figures each tile asks for.
 *  One instance per auth context. */
export class InsightsApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** The tenant's boards, oldest first — the order the tab strip shows them
   *  in, so the seeded Business overview stays the first tab.
   *
   *  This read is also what **seeds** that overview on a tenant's first visit,
   *  which is why it carries the language: the server writes the board's name
   *  and its captions once, in the language of whoever opened Insights first,
   *  and they are the tenant's own words from that moment on. */
  dashboards(): Promise<Dashboard[]> {
    return this.#read<{ dashboards?: Dashboard[] }>(
      `/insights/dashboards?lang=${encodeURIComponent(getLocale())}`,
    ).then((r) => r.dashboards ?? []);
  }

  /** The prebuilt questions a reader can pin, and which of them the Business
   *  overview is built from. The entries carry no words: the client translates
   *  each `key`, because English from a server is English for everybody. */
  gallery(): Promise<Gallery> {
    return this.#read<{ entries?: GalleryEntry[]; overview?: string[] }>(
      "/insights/gallery",
    ).then((r) => ({ entries: r.entries ?? [], overview: r.overview ?? [] }));
  }

  /** A new, empty board. */
  createDashboard(name: string): Promise<Dashboard> {
    return this.#write<{ dashboard: Dashboard }>("POST", "/insights/dashboards", { name }).then(
      (r) => r.dashboard,
    );
  }

  /** One board with its tiles. A board of another tenant is the same `404` an
   *  id that never existed gets. */
  board(id: string): Promise<BoardContents> {
    return this.#read<BoardContents>(`/insights/dashboards/${encodeURIComponent(id)}`);
  }

  /** Renames a board. A seeded board renames like any other. */
  renameDashboard(id: string, name: string): Promise<Dashboard> {
    return this.#write<{ dashboard: Dashboard }>(
      "PATCH",
      `/insights/dashboards/${encodeURIComponent(id)}`,
      { name },
    ).then((r) => r.dashboard);
  }

  /** Deletes a board and the tiles on it. Nothing is lost that the invoices and
   *  deals underneath do not still hold — a board is a view, never a record. */
  async deleteDashboard(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/insights/dashboards/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /** Pins a question to a board, at the end of its layout. The spec is
   *  validated by the server's write gate — the client never decides that a
   *  chart is askable. */
  createTile(dashboardId: string, pin: TilePin): Promise<Tile> {
    return this.#write<{ tile: Tile }>(
      "POST",
      `/insights/dashboards/${encodeURIComponent(dashboardId)}/tiles`,
      pin,
    ).then((r) => r.tile);
  }

  /** Retitles or resizes a tile. It cannot move one: that is its own request,
   *  so saving an edit can never rearrange a board. */
  updateTile(id: string, edit: TileEdit): Promise<Tile> {
    return this.#write<{ tile: Tile }>(
      "PATCH",
      `/insights/tiles/${encodeURIComponent(id)}`,
      edit,
    ).then((r) => r.tile);
  }

  /** Moves a tile to `position` — fractional, so one tile changes and the rest
   *  of the board stays where it was. */
  moveTile(id: string, position: number): Promise<Tile> {
    return this.#write<{ tile: Tile }>("POST", `/insights/tiles/${encodeURIComponent(id)}/move`, {
      position,
    }).then((r) => r.tile);
  }

  /** Unpins a tile. */
  async deleteTile(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/insights/tiles/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /** The figures for one stored tile, evaluated from the tenant's documents on
   *  every read — nothing computed is cached anywhere, deliberately. */
  tileData(id: string): Promise<Series> {
    return this.#read<Series>(`/insights/tiles/${encodeURIComponent(id)}/data`);
  }

  /** Asks the assistant for a chart (BI1.07). The answer is a **proposal**: the
   *  server stores nothing, and the chart becomes a tile only if the reader
   *  pins it — with `spec` handed back exactly as it came, so the write gate
   *  validates the same question that was previewed.
   *
   *  A workspace with no model configured fails with `503`; the caller says so
   *  and the rest of Insights carries on. */
  ask(question: string): Promise<AskProposal> {
    return this.#write<AskProposal>("POST", "/insights/ask", { q: question });
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

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new InsightsError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) throw new InsightsError(res.status, await problemDetail(res));
    return (await res.json()) as T;
  }
}

/** The Insights client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useInsightsApi(): InsightsApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new InsightsApi(authorizedFetch), [authorizedFetch]);
}
