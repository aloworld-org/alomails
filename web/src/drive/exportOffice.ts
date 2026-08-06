// Export an alo Sheet back to a real `.xlsx` (ADR 0033), so a native sheet can be
// handed to the outside world as a genuine Excel file. The inverse of
// `importOffice.ts`: a Univer workbook snapshot becomes an OOXML zip. Best-effort
// and symmetric with the import — cell values, numbers, and booleans across all
// sheets are written; styles, formulas, and charts are not. Runs client-side
// (fflate zips; the XML is assembled by hand — no third-party spreadsheet engine).
import { zipSync, strToU8 } from "fflate";

// Univer's CellValueType (from @univerjs/core). STRING (1) is the default case,
// so only NUMBER and BOOLEAN need naming here.
const T_NUMBER = 2;
const T_BOOLEAN = 3;

type SnapshotCell = { v?: unknown; t?: number };
type SnapshotSheet = { name?: unknown; cellData?: Record<string, Record<string, SnapshotCell>> };
type Snapshot = {
  name?: unknown;
  sheetOrder?: unknown;
  sheets?: Record<string, SnapshotSheet>;
};

const MAIN_NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships";

/** 0 → "A", 25 → "Z", 26 → "AA", … (inverse of the import's columnToIndex). */
function indexToColumn(index: number): string {
  let n = index + 1;
  let s = "";
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

function escapeXml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/** Decide a cell's Excel representation from its Univer value + type. */
function classify(cell: SnapshotCell): { kind: "s" | "n" | "b"; text: string } | null {
  const v = cell.v;
  if (v === null || v === undefined || v === "") return null;
  const t = cell.t;
  if (t === T_BOOLEAN || typeof v === "boolean") {
    return { kind: "b", text: v === true || v === 1 || v === "1" ? "1" : "0" };
  }
  if (t === T_NUMBER || typeof v === "number") {
    const num = Number(v);
    if (!Number.isFinite(num)) return { kind: "s", text: String(v) };
    return { kind: "n", text: String(num) };
  }
  // A string cell (Univer's STRING type), or anything else stringified.
  return { kind: "s", text: String(v) };
}

const CONTENT_TYPES = (sheetCount: number): string => {
  const overrides: string[] = [];
  for (let i = 1; i <= sheetCount; i++) {
    overrides.push(
      `<Override PartName="/xl/worksheets/sheet${i}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`,
    );
  }
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
${overrides.join("\n")}
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>`;
};

const ROOT_RELS = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="${PKG_REL_NS}">
<Relationship Id="rId1" Type="${REL_NS}/officeDocument" Target="xl/workbook.xml"/>
</Relationships>`;

const STYLES = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="${MAIN_NS}">
<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
<fills count="1"><fill><patternFill patternType="none"/></fill></fills>
<borders count="1"><border/></borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>
</styleSheet>`;

/**
 * Convert a Univer workbook snapshot into `.xlsx` bytes. Never throws on a
 * well-formed snapshot; an empty or malformed snapshot yields a valid one-sheet
 * workbook so the download always produces a file Excel can open.
 */
export function univerSnapshotToXlsx(snapshot: Snapshot): Uint8Array {
  const sheetsMap = snapshot.sheets ?? {};
  const order = Array.isArray(snapshot.sheetOrder)
    ? (snapshot.sheetOrder as string[])
    : Object.keys(sheetsMap);
  const ids = order.filter((id) => sheetsMap[id] !== undefined);
  if (ids.length === 0) ids.push("__blank__");

  // Shared strings, deduplicated, in first-seen order.
  const sharedIndex = new Map<string, number>();
  const shared: string[] = [];
  const internString = (s: string): number => {
    const existing = sharedIndex.get(s);
    if (existing !== undefined) return existing;
    const idx = shared.length;
    sharedIndex.set(s, idx);
    shared.push(s);
    return idx;
  };

  const worksheetParts: string[] = [];
  const sheetEntries: { name: string }[] = [];

  ids.forEach((id, si) => {
    const sheet = sheetsMap[id];
    const name = typeof sheet?.name === "string" && sheet.name.length > 0 ? sheet.name : `Sheet${si + 1}`;
    sheetEntries.push({ name });

    const cellData = sheet?.cellData ?? {};
    const rowsXml: string[] = [];
    const rowKeys = Object.keys(cellData)
      .map((r) => Number.parseInt(r, 10))
      .filter((r) => Number.isInteger(r) && r >= 0)
      .sort((a, b) => a - b);

    for (const row of rowKeys) {
      const cols = cellData[String(row)] ?? {};
      const colKeys = Object.keys(cols)
        .map((c) => Number.parseInt(c, 10))
        .filter((c) => Number.isInteger(c) && c >= 0)
        .sort((a, b) => a - b);

      const cellsXml: string[] = [];
      for (const col of colKeys) {
        const cell = cols[String(col)];
        if (cell === undefined) continue;
        const info = classify(cell);
        if (info === null) continue;
        const ref = `${indexToColumn(col)}${row + 1}`;
        if (info.kind === "s") {
          cellsXml.push(`<c r="${ref}" t="s"><v>${internString(info.text)}</v></c>`);
        } else if (info.kind === "b") {
          cellsXml.push(`<c r="${ref}" t="b"><v>${info.text}</v></c>`);
        } else {
          cellsXml.push(`<c r="${ref}"><v>${info.text}</v></c>`);
        }
      }
      if (cellsXml.length > 0) {
        rowsXml.push(`<row r="${row + 1}">${cellsXml.join("")}</row>`);
      }
    }

    worksheetParts.push(
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="${MAIN_NS}"><sheetData>${rowsXml.join("")}</sheetData></worksheet>`,
    );
  });

  // workbook.xml — sheets reference worksheet relationships rId1..rIdN.
  const sheetTags = sheetEntries
    .map((s, i) => `<sheet name="${escapeXml(s.name)}" sheetId="${i + 1}" r:id="rId${i + 1}"/>`)
    .join("");
  const workbook = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="${MAIN_NS}" xmlns:r="${REL_NS}"><sheets>${sheetTags}</sheets></workbook>`;

  // workbook rels — worksheets first (matching r:id), then styles + sharedStrings.
  const relLines = sheetEntries.map(
    (_s, i) => `<Relationship Id="rId${i + 1}" Type="${REL_NS}/worksheet" Target="worksheets/sheet${i + 1}.xml"/>`,
  );
  relLines.push(
    `<Relationship Id="rId${sheetEntries.length + 1}" Type="${REL_NS}/styles" Target="styles.xml"/>`,
  );
  relLines.push(
    `<Relationship Id="rId${sheetEntries.length + 2}" Type="${REL_NS}/sharedStrings" Target="sharedStrings.xml"/>`,
  );
  const workbookRels = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="${PKG_REL_NS}">${relLines.join("")}</Relationships>`;

  const sharedStrings = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="${MAIN_NS}" count="${shared.length}" uniqueCount="${shared.length}">${shared
    .map((s) => `<si><t xml:space="preserve">${escapeXml(s)}</t></si>`)
    .join("")}</sst>`;

  const files: Record<string, Uint8Array> = {
    "[Content_Types].xml": strToU8(CONTENT_TYPES(sheetEntries.length)),
    "_rels/.rels": strToU8(ROOT_RELS),
    "xl/workbook.xml": strToU8(workbook),
    "xl/_rels/workbook.xml.rels": strToU8(workbookRels),
    "xl/styles.xml": strToU8(STYLES),
    "xl/sharedStrings.xml": strToU8(sharedStrings),
  };
  worksheetParts.forEach((xml, i) => {
    files[`xl/worksheets/sheet${i + 1}.xml`] = strToU8(xml);
  });

  return zipSync(files);
}
