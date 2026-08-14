// The section palette, as the editor reads it (ADR 0042 §4, S3.01d).
//
// The server answers one tile per section type, each either a section built
// out of THIS website's own content or the reason there is nothing of the
// owner's to put in that block yet. Nothing is composed here: a tile's section
// travels back to `POST …/sections` exactly as it arrived, so what the palette
// showed is what the page stores.
//
// The wire is read defensively, the way the layout declaration is: a tile
// this build cannot make sense of is dropped, and a palette that fails to load
// at all costs the seeded previews, never the ability to add a section — the
// prop form is always the fallback.
import type { Section, SectionKind } from "./sections";
import { SECTION_KINDS } from "./sections";

/** What a block is missing before it can be seeded from the site itself. */
export type PaletteNeed =
  | "writing"
  | "picture"
  | "catalog"
  | "collection"
  | "booking"
  | "code";

const NEEDS: readonly PaletteNeed[] = [
  "writing",
  "picture",
  "catalog",
  "collection",
  "booking",
  "code",
];

/** One offer in the palette. */
export interface PaletteTile {
  kind: SectionKind;
  /** The section as the server seeded it, or `null` when it could not be. */
  section: Section | null;
  /** Why not, when there is no section. */
  needs: PaletteNeed | null;
}

/** Where a dropped or chosen tile lands: an insertion index in `0…count`,
 *  clamped, so a stale position (the page changed under an open palette) puts
 *  the block at the end rather than being refused. */
export function insertionIndex(count: number, wanted: number): number {
  if (!Number.isInteger(wanted) || wanted < 0) return count;
  return Math.min(wanted, count);
}

/** The palette every editor can offer without a server: one tile per section
 *  type, none of them seeded. What a failed request, an older server, or a
 *  language other than the site's own falls back to. */
export function unseededPalette(): PaletteTile[] {
  return SECTION_KINDS.map((kind) => ({ kind, section: null, needs: null }));
}

/** Reads a `GET …/palette` body, dropping anything unrecognizable. */
export function readPalette(value: unknown): PaletteTile[] {
  const items = (value as { items?: unknown } | null)?.items;
  if (!Array.isArray(items)) return unseededPalette();
  const tiles: PaletteTile[] = [];
  for (const raw of items) {
    const tile = readTile(raw);
    if (tile !== null) tiles.push(tile);
  }
  return tiles.length === 0 ? unseededPalette() : tiles;
}

function readTile(raw: unknown): PaletteTile | null {
  if (typeof raw !== "object" || raw === null) return null;
  const item = raw as Record<string, unknown>;
  const kind = item["kind"];
  if (typeof kind !== "string") return null;
  if (!SECTION_KINDS.includes(kind as SectionKind)) return null;
  const section = item["section"];
  const ready =
    item["ready"] === true && typeof section === "object" && section !== null;
  const needs = item["needs"];
  return {
    kind: kind as SectionKind,
    section: ready ? (section as Section) : null,
    needs:
      !ready && typeof needs === "string" && NEEDS.includes(needs as PaletteNeed)
        ? (needs as PaletteNeed)
        : null,
  };
}
