// What the `/insights` API sends, in the shape it sends it (ADR 0037, wave
// BI1.05) — `docs/design/insights.md` § The series that comes back.
//
// Two rules from that note are visible in these types, and both are load-
// bearing:
//
//   - **Every figure is an integer.** Money is cents, a count is a count, a
//     ratio is basis points, and which one a `value` is comes from `unit` —
//     never from the number itself. Nothing here is a float that a browser
//     could round differently from the server.
//   - **A label is an id or the tenant's own words.** The server never sends
//     English; a bucket from our closed vocabulary crosses as a catalog id the
//     client translates, a customer or stage name crosses verbatim because it
//     was never ours, and a VAT rate crosses as the number it is.
//
// A tile's `spec` is deliberately `unknown` here: the envelope is the store's
// contract, validated on write, and this wave only *renders* what a spec
// produced. The builder (BI1.06/BI1.07) is where a spec becomes something the
// client constructs, and where it gets a type worth having.

/** How an answer is drawn. */
export type Viz = "number" | "bar" | "line" | "pie" | "table";

/** What every value in a series is measured in. */
export type Unit = "money" | "count" | "percent_bp";

/** A board of pinned questions. */
export interface Dashboard {
  id: string;
  name: string;
  /** `business_overview` for the board we seed (BI1.06), `null` for a user's. */
  systemKey: string | null;
  seeded: boolean;
  createdBy: string | null;
  createdAt: string;
  updatedAt: string;
}

/** One question pinned to a board.
 *
 *  A tile whose stored spec this build cannot read still arrives — with
 *  `readable: false`, the raw envelope and `specError` — because a board that
 *  renders every chart but one is worth more than a board that refuses to
 *  render at all. */
export interface Tile {
  id: string;
  dashboardId: string;
  title: string;
  spec: unknown;
  readable: boolean;
  specError: string | null;
  /** The derived drawing of a readable tile; `null` when its spec is not. */
  viz: Viz | null;
  /** Fractional layout order — an ordering, never a quantity. */
  position: number;
  /** How many of the grid's four columns the tile takes. */
  span: number;
  createdAt: string;
  updatedAt: string;
}

/** What a bucket or a group is called on screen. */
export type SeriesLabel =
  | { kind: "catalog"; id: string }
  | { kind: "raw"; text: string }
  | { kind: "rate_bp"; bp: number };

/** One bucket and what was measured in it. A time bucket carries no label: its
 *  ISO key already says everything, and the client formats it. */
export interface SeriesPoint {
  bucket: string;
  label?: SeriesLabel;
  value: number;
}

/** One drawable line/bar set — one per currency when money could not honestly
 *  be restated into a single one. */
export interface SeriesGroup {
  key: string;
  label: SeriesLabel;
  points: SeriesPoint[];
}

/** How to read every value: the kind, and the one currency the whole answer is
 *  expressed in when there is one. */
export interface SeriesUnit {
  kind: Unit;
  currency?: string;
}

/** Something true about the answer that the numbers alone do not say. */
export interface SeriesNote {
  code: string;
  count: number;
}

/** A whole chart answer. `series` is the wire name of the drawable groups. */
export interface Series {
  unit: SeriesUnit;
  series: SeriesGroup[];
  notes: SeriesNote[];
  truncated: boolean;
}

/** The fields of a tile a reader may change from a board (BI1.05): what it is
 *  called and how wide it sits. Its question is the builder's business. */
export interface TileEdit {
  title?: string;
  span?: number;
}

/** Which part of the business a prebuilt question reads. */
export type GalleryModule = "billing" | "crm";

/** One ready-made chart the gallery offers (BI1.06).
 *
 *  It carries **no words**: `key` is what the client translates a title and a
 *  description from, because a server that sent English would be sending it to
 *  every language. `spec` is the question itself, handed back so pinning it is
 *  the ordinary tile request — the same write gate validates it either way. */
export interface GalleryEntry {
  key: string;
  module: GalleryModule;
  viz: Viz;
  span: number;
  spec: unknown;
}

/** The gallery, plus which of its entries the zero-setup Business overview is
 *  built from — so a board can say what it already has. */
export interface Gallery {
  entries: GalleryEntry[];
  overview: string[];
}

/** What pinning a question to a board sends: the caption the reader will see
 *  (in their language, never the server's), the question, and the width. */
export interface TilePin {
  title: string;
  spec: unknown;
  span: number;
}
