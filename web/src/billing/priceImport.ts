import { xlsxToUniverSnapshot } from "../drive/importOffice";

export type ImportCell = string | number | boolean;
export type ImportTable = { headers: string[]; rows: ImportCell[][] };

function csvRows(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index]!;
    if (char === '"') {
      if (quoted && text[index + 1] === '"') { cell += '"'; index += 1; }
      else quoted = !quoted;
    } else if (char === "," && !quoted) {
      row.push(cell.trim()); cell = "";
    } else if ((char === "\n" || char === "\r") && !quoted) {
      if (char === "\r" && text[index + 1] === "\n") index += 1;
      row.push(cell.trim());
      if (row.some((value) => value !== "")) rows.push(row);
      row = []; cell = "";
    } else cell += char;
  }
  row.push(cell.trim());
  if (row.some((value) => value !== "")) rows.push(row);
  return rows;
}

function snapshotRows(snapshot: Record<string, unknown>): ImportCell[][] {
  const order = snapshot.sheetOrder as string[] | undefined;
  const sheets = snapshot.sheets as Record<string, { cellData?: Record<string, Record<string, { v?: ImportCell }>> }> | undefined;
  const sheet = order?.[0] === undefined ? undefined : sheets?.[order[0]];
  const data = sheet?.cellData ?? {};
  const rowNumbers = Object.keys(data).map(Number).filter(Number.isFinite).sort((a, b) => a - b);
  if (rowNumbers.length === 0) return [];
  const maxColumn = Math.max(0, ...Object.values(data).flatMap((row) => Object.keys(row).map(Number).filter(Number.isFinite)));
  return rowNumbers.map((rowNumber) => Array.from({ length: maxColumn + 1 }, (_, column) => data[String(rowNumber)]?.[String(column)]?.v ?? ""));
}

function table(rows: ImportCell[][]): ImportTable {
  if (rows.length === 0) return { headers: [], rows: [] };
  const width = Math.max(...rows.map((row) => row.length));
  const first = rows[0] ?? [];
  const headers = Array.from({ length: width }, (_, index) => String(first[index] ?? `Column ${index + 1}`).trim() || `Column ${index + 1}`);
  return { headers, rows: rows.slice(1).filter((row) => row.some((cell) => String(cell).trim() !== "")) };
}

function readFile(file: File, mode: "text"): Promise<string>;
function readFile(file: File, mode: "buffer"): Promise<ArrayBuffer>;
function readFile(file: File, mode: "text" | "buffer"): Promise<string | ArrayBuffer> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (mode === "text" && typeof reader.result === "string") resolve(reader.result);
      else if (mode === "buffer" && reader.result instanceof ArrayBuffer) resolve(reader.result);
      else reject(new Error("File could not be read."));
    };
    reader.onerror = () => reject(new Error("File could not be read."));
    if (mode === "text") reader.readAsText(file);
    else reader.readAsArrayBuffer(file);
  });
}

export async function readPriceImport(file: File): Promise<ImportTable> {
  const extension = file.name.split(".").pop()?.toLocaleLowerCase();
  if (extension === "csv") return table(csvRows(await readFile(file, "text")));
  if (extension === "xlsx") {
    const snapshot = xlsxToUniverSnapshot(new Uint8Array(await readFile(file, "buffer")), file.name);
    return table(snapshotRows(snapshot));
  }
  throw new Error("Choose a CSV or Excel (.xlsx) file.");
}

const ALIASES: Record<string, string[]> = {
  name: ["name", "product", "item", "description", "product name"],
  unit: ["unit", "uom", "measure"],
  unitPrice: ["unit price", "price", "selling price", "sales price", "rate"],
  vat: ["vat", "vat rate", "tax", "tax rate"],
  sku: ["sku", "item code", "product code", "code"],
};

export function suggestColumn(headers: string[], field: keyof typeof ALIASES): number | null {
  const normalized = headers.map((header) => header.trim().toLocaleLowerCase());
  const index = normalized.findIndex((header) => ALIASES[field]!.includes(header));
  return index < 0 ? null : index;
}

export function importNumber(value: ImportCell): number | null {
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  const cleaned = String(value).trim().replace(/[%€$£\s]/g, "");
  if (cleaned === "") return null;
  const normalized = cleaned.includes(",") && cleaned.includes(".")
    ? cleaned.lastIndexOf(",") > cleaned.lastIndexOf(".") ? cleaned.replace(/\./g, "").replace(",", ".") : cleaned.replace(/,/g, "")
    : cleaned.replace(",", ".");
  const number = Number(normalized);
  return Number.isFinite(number) ? number : null;
}
