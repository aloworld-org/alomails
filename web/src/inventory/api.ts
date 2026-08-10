// The client for the `/inventory` HTTP surface (alo Inventory, ADR 0035,
// wave B5).
//
// Its own small client, for the same reason Billing, CRM, Projects and Finance
// each have one: `/inventory` is a plain REST surface with none of JMAP's
// session, capabilities or method-call envelope. It uses the same authenticated
// fetch (bearer + refresh handled by the auth layer), so there is one session
// and not five, and it fails through the shared `platform/rest` shape so a
// server sentence reaches a user the same way in every module.
//
// **It holds no arithmetic at all.** On-hand is the server's fold of the
// movement ledger; the reference value is the server's; the total is the sum of
// exactly the rows the server returned. Quantities are integer milli-units in
// and out, money is integer cents, and nothing here divides either.
//
// **There is no write door for a quantity, and there never will be.** On-hand
// is derived the way a balance is derived from postings, and the only thing
// that changes it is a movement (`docs/design/inventory.md` § Locations and the
// move ledger). The catalog's own writes go through Billing's client, because
// the record they edit is Billing's row.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type { InvLocation, InvMove, InvSupplier, StockRead } from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed Inventory request, carrying the server's own `Problem` detail. */
export class InventoryError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "InventoryError");
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function inventoryMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** What narrows a stock read. All of them absent is "what is on the shelves,
 *  right now" — which is the question a stock screen opens on. */
export interface StockFilter {
  /** One product across every place. */
  productId?: string;
  /** One place across every product. */
  locationId?: string;
  /** Add the four virtual counterparties. A screen that does this must not
   *  also present the total as a stock value: it sums to roughly zero, which
   *  is a correct reading of a closed ledger and a nonsense reading of a
   *  warehouse. */
  includeVirtual?: boolean;
  /** Keep the rows that have fallen back to zero — what one product's own
   *  history page wants, and what a shelf list does not. */
  includeZero?: boolean;
}

/** What narrows a read of the movement ledger. */
export interface MoveFilter {
  productId?: string;
  /** Matches **either end**: "what happened at this warehouse" is one
   *  question, not two. */
  locationId?: string;
  /** RFC 3339 instants. The server refuses text that is not one rather than
   *  quietly answering "everything". */
  from?: string;
  to?: string;
  limit?: number;
}

/** Places, on-hand, and the ledger behind it. One instance per auth context. */
export class InventoryApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /**
   * The tenant's places in code order, **seeding the starting set on the first
   * read a tenant ever makes**.
   *
   * It carries the interface language for that reason and only that one: the
   * server names the seeded locations once, in the language of whoever opened
   * the screen first, and they are ordinary tenant data from that moment on
   * (`inventory_location_names.rs`). A later language change renames nothing,
   * which is right — a warehouse called "Hoofdmagazijn" keeps its name.
   */
  locations(includeArchived = false): Promise<InvLocation[]> {
    const query = new URLSearchParams({ lang: getLocale() });
    if (includeArchived) query.set("includeArchived", "1");
    return this.#read<{ locations?: InvLocation[] }>(
      `/inventory/locations?${query.toString()}`,
    ).then((r) => r.locations ?? []);
  }

  /** What is where, with the server's sum of exactly the rows it returned. */
  stock(filter: StockFilter = {}): Promise<StockRead> {
    const query = new URLSearchParams();
    if (filter.productId !== undefined) query.set("productId", filter.productId);
    if (filter.locationId !== undefined) query.set("locationId", filter.locationId);
    if (filter.includeVirtual === true) query.set("includeVirtual", "1");
    if (filter.includeZero === true) query.set("includeZero", "1");
    const rendered = query.toString();
    return this.#read<{ stock?: StockRead["stock"]; totalValueCents?: number }>(
      `/inventory/stock${rendered === "" ? "" : `?${rendered}`}`,
    ).then((r) => ({ stock: r.stock ?? [], totalValueCents: r.totalValueCents ?? 0 }));
  }

  /** The movement ledger, newest first. The server caps the page and says what
   *  cap it applied; a screen shows what it was given and says so. */
  moves(filter: MoveFilter = {}): Promise<{ moves: InvMove[]; limit: number }> {
    const query = new URLSearchParams();
    if (filter.productId !== undefined) query.set("productId", filter.productId);
    if (filter.locationId !== undefined) query.set("locationId", filter.locationId);
    if (filter.from !== undefined) query.set("from", filter.from);
    if (filter.to !== undefined) query.set("to", filter.to);
    if (filter.limit !== undefined) query.set("limit", String(filter.limit));
    const rendered = query.toString();
    return this.#read<{ moves?: InvMove[]; limit?: number }>(
      `/inventory/moves${rendered === "" ? "" : `?${rendered}`}`,
    ).then((r) => ({ moves: r.moves ?? [], limit: r.limit ?? 0 }));
  }

  /** Who this tenant buys from, for the catalog's default-supplier picker.
   *  Active ones only: attaching new business to an archived supplier is a
   *  refusal on the server, and offering a choice that always fails is worse
   *  than not offering it. */
  suppliers(): Promise<InvSupplier[]> {
    return this.#read<{ suppliers?: InvSupplier[] }>("/inventory/suppliers").then(
      (r) => r.suppliers ?? [],
    );
  }

  // ---- plumbing ----------------------------------------------------------

  async #read<T>(path: string): Promise<T> {
    const res = await this.#send(path, { method: "GET" });
    if (!res.ok) throw await failure(res);
    return (await res.json()) as T;
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new InventoryError(0, null);
    }
  }
}

/** The refusal a failed response carries: the server's own sentence, verbatim,
 *  or nothing when the body was not the `Problem` shape. */
async function failure(res: Response): Promise<InventoryError> {
  return new InventoryError(res.status, await problemDetail(res));
}

/** The Inventory client bound to the current session. Memoized per auth
 *  context, so a re-render never re-creates it and effects keyed on it do not
 *  loop. */
export function useInventoryApi(): InventoryApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new InventoryApi(authorizedFetch), [authorizedFetch]);
}
