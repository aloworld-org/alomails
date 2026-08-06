// Best-effort import of real Office files into alo's native editors (ADR 0033).
// Stage 1 covers spreadsheets: a real `.xlsx` (OOXML — a zip of XML) becomes a
// Univer workbook snapshot, the same JSON an alo Sheet stores. This is a one-way
// import: cell values, types, and sheet structure carry over; styles, formulas'
// definitions, charts, and exact layout do not. The original file is never
// touched — the caller keeps it in Drive and creates a new sheet from this.
//
// No third-party spreadsheet engine and no server round-trip: fflate unzips, the
// browser's DOMParser reads the XML. Everything runs client-side.
import { unzipSync, strFromU8 } from "fflate";

/** A Univer workbook snapshot — an opaque JSON object alo Sheet persists verbatim. */
export type Snapshot = Record<string, unknown>;

// Univer's CellValueType (from @univerjs/core).
const T_STRING = 1;
const T_NUMBER = 2;
const T_BOOLEAN = 3;

type CellValue = string | number | boolean;
type Cell = { v: CellValue; t: number };

/** "A" → 0, "Z" → 25, "AA" → 26, … (base-26, letters only, already upper-cased). */
function columnToIndex(letters: string): number {
  let n = 0;
  for (let i = 0; i < letters.length; i++) {
    n = n * 26 + (letters.charCodeAt(i) - 64);
  }
  return n - 1;
}

/** An A1-style cell reference ("AB12") → zero-based [row, col], or null if malformed. */
function parseRef(ref: string): [number, number] | null {
  const m = /^([A-Za-z]+)(\d+)$/.exec(ref);
  if (m === null) return null;
  const letters = m[1];
  const digits = m[2];
  if (letters === undefined || digits === undefined) return null;
  const col = columnToIndex(letters.toUpperCase());
  const row = Number.parseInt(digits, 10) - 1;
  if (row < 0 || col < 0) return null;
  return [row, col];
}

function firstText(parent: Element, tag: string): string | null {
  const el = parent.getElementsByTagName(tag)[0];
  return el ? (el.textContent ?? "") : null;
}

/** Concatenate every `<t>` under an element (rich-text runs join into one string). */
function joinText(el: Element): string {
  return Array.from(el.getElementsByTagName("t"))
    .map((t) => t.textContent ?? "")
    .join("");
}

/** Resolve one worksheet's `<c>` cells into a Univer `cellData` map. */
function readSheet(
  doc: Document,
  shared: readonly string[],
): { cellData: Record<number, Record<number, Cell>>; rows: number; cols: number } {
  const cellData: Record<number, Record<number, Cell>> = {};
  let maxRow = 0;
  let maxCol = 0;

  for (const c of Array.from(doc.getElementsByTagName("c"))) {
    const ref = c.getAttribute("r");
    if (ref === null) continue;
    const pos = parseRef(ref);
    if (pos === null) continue;
    const [row, col] = pos;
    const type = c.getAttribute("t"); // "s" | "str" | "inlineStr" | "b" | "e" | "n" | null

    let value: CellValue | null = null;
    let t = T_STRING;

    if (type === "s") {
      const idx = Number.parseInt(firstText(c, "v") ?? "", 10);
      value = Number.isNaN(idx) ? "" : (shared[idx] ?? "");
    } else if (type === "inlineStr") {
      const is = c.getElementsByTagName("is")[0];
      value = is ? joinText(is) : "";
    } else if (type === "str") {
      value = firstText(c, "v") ?? "";
    } else if (type === "b") {
      value = (firstText(c, "v") ?? "0") === "1";
      t = T_BOOLEAN;
    } else if (type === "e") {
      value = firstText(c, "v") ?? "#ERROR"; // surface the cached error text
    } else {
      // Number (the default when `t` is absent). A formula cell carries its
      // cached result in `<v>`; we take that.
      const raw = firstText(c, "v");
      if (raw === null || raw === "") continue;
      const num = Number(raw);
      if (Number.isNaN(num)) {
        value = raw;
      } else {
        value = num;
        t = T_NUMBER;
      }
    }

    if (value === null) continue;
    (cellData[row] ??= {})[col] = { v: value, t };
    if (row > maxRow) maxRow = row;
    if (col > maxCol) maxCol = col;
  }

  return { cellData, rows: maxRow + 1, cols: maxCol + 1 };
}

/**
 * Convert `.xlsx` bytes into a Univer workbook snapshot. Never throws on odd but
 * well-formed input — missing parts degrade to an empty sheet. Throws only if the
 * bytes are not a readable zip (the caller treats that as an import failure).
 */
export function xlsxToUniverSnapshot(bytes: Uint8Array, bookName: string): Snapshot {
  const files = unzipSync(bytes);
  const parser = new DOMParser();
  const xml = (path: string): Document | null => {
    const buf = files[path];
    if (buf === undefined) return null;
    return parser.parseFromString(strFromU8(buf), "application/xml");
  };

  // 1. Shared strings, indexed by position (`<sst><si>…</si></sst>`).
  const shared: string[] = [];
  const sst = xml("xl/sharedStrings.xml");
  if (sst !== null) {
    for (const si of Array.from(sst.getElementsByTagName("si"))) {
      shared.push(joinText(si));
    }
  }

  // 2. Sheet order + names (workbook.xml) mapped to their part paths (…rels).
  const rels = xml("xl/_rels/workbook.xml.rels");
  const relTarget = new Map<string, string>();
  if (rels !== null) {
    for (const r of Array.from(rels.getElementsByTagName("Relationship"))) {
      const id = r.getAttribute("Id");
      const target = r.getAttribute("Target");
      if (id !== null && target !== null) {
        relTarget.set(id, target.replace(/^\/?xl\//, "").replace(/^\//, ""));
      }
    }
  }

  const RELS_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
  const refs: { name: string; path: string }[] = [];
  const wb = xml("xl/workbook.xml");
  if (wb !== null) {
    Array.from(wb.getElementsByTagName("sheet")).forEach((s, i) => {
      const name = s.getAttribute("name") ?? `Sheet${i + 1}`;
      const rid = s.getAttribute("r:id") ?? s.getAttributeNS(RELS_NS, "id");
      const target = rid !== null ? relTarget.get(rid) : undefined;
      refs.push({ name, path: target !== undefined ? `xl/${target}` : "" });
    });
  }
  // Fallback: if the workbook part was unreadable, glob the worksheet parts.
  if (refs.length === 0) {
    Object.keys(files)
      .filter((k) => /^xl\/worksheets\/sheet\d+\.xml$/.test(k))
      .sort()
      .forEach((path, i) => refs.push({ name: `Sheet${i + 1}`, path }));
  }

  // 3. Build the snapshot.
  const sheetOrder: string[] = [];
  const sheets: Record<string, unknown> = {};
  refs.forEach((ref, i) => {
    const id = `sheet-${i + 1}`;
    sheetOrder.push(id);
    const doc = ref.path !== "" ? xml(ref.path) : null;
    const { cellData, rows, cols } =
      doc !== null ? readSheet(doc, shared) : { cellData: {}, rows: 1, cols: 1 };
    sheets[id] = {
      id,
      name: ref.name,
      cellData,
      rowCount: Math.max(rows, 100),
      columnCount: Math.max(cols, 26),
    };
  });

  if (sheetOrder.length === 0) {
    const id = "sheet-1";
    sheetOrder.push(id);
    sheets[id] = { id, name: "Sheet1", cellData: {}, rowCount: 100, columnCount: 26 };
  }

  return {
    id: bookName.length > 0 ? bookName : "workbook",
    name: bookName.length > 0 ? bookName : "Workbook",
    appVersion: "0.25.1",
    locale: "enUS",
    sheetOrder,
    sheets,
  };
}
