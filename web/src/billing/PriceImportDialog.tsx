import { useRef, useState } from "react";
import { Check, FileSpreadsheet, Image, Upload, X } from "lucide-react";

import { Button, ChoicePicker, Modal } from "../ds";
import { billingMessage, useBillingApi } from "./api";
import { importNumber, readPriceImport, suggestColumn, type ImportCell } from "./priceImport";
import type { BillingProduct, ProductDraft } from "./types";

type Candidate = { name: string; unit: string; price: number; vat: number; sku: string; include: boolean; problem?: string };
type Mapping = { name: number | null; unit: number | null; price: number | null; vat: number | null; sku: number | null };

function fileDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("Image could not be read."));
    reader.onerror = () => reject(new Error("Image could not be read."));
    reader.readAsDataURL(file);
  });
}

function cell(row: ImportCell[], column: number | null): ImportCell { return column === null ? "" : (row[column] ?? ""); }

function mappedRows(rows: ImportCell[][], mapping: Mapping): Candidate[] {
  return rows.map((row) => {
    const name = String(cell(row, mapping.name)).trim();
    const price = importNumber(cell(row, mapping.price));
    const vat = importNumber(cell(row, mapping.vat)) ?? 0;
    const problem = name === "" ? "Missing name" : price === null || price < 0 ? "Invalid price" : vat < 0 ? "Invalid VAT" : undefined;
    return { name, unit: String(cell(row, mapping.unit)).trim(), price: price ?? 0, vat, sku: String(cell(row, mapping.sku)).trim(), include: problem === undefined, ...(problem === undefined ? {} : { problem }) };
  });
}

export function PriceImportDialog({ existing, onClose, onImported }: { existing: BillingProduct[]; onClose: () => void; onImported: () => void }) {
  const api = useBillingApi();
  const input = useRef<HTMLInputElement>(null);
  const [fileName, setFileName] = useState("");
  const [headers, setHeaders] = useState<string[]>([]);
  const [sourceRows, setSourceRows] = useState<ImportCell[][]>([]);
  const [mapping, setMapping] = useState<Mapping>({ name: null, unit: null, price: null, vat: null, sku: null });
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<number | null>(null);

  async function choose(file: File) {
    setBusy(true); setError(null); setDone(null); setFileName(file.name);
    try {
      if (file.type.startsWith("image/")) {
        const rows = await api.extractPriceListImage(await fileDataUrl(file));
        setHeaders([]); setSourceRows([]);
        setCandidates(rows.map((row) => ({ name: row.name.trim(), unit: row.unit.trim(), price: row.unitPrice, vat: row.vatRate, sku: row.sku.trim(), include: true })));
      } else {
        const table = await readPriceImport(file);
        const next: Mapping = {
          name: suggestColumn(table.headers, "name"), unit: suggestColumn(table.headers, "unit"),
          price: suggestColumn(table.headers, "unitPrice"), vat: suggestColumn(table.headers, "vat"), sku: suggestColumn(table.headers, "sku"),
        };
        setHeaders(table.headers); setSourceRows(table.rows); setMapping(next); setCandidates(mappedRows(table.rows, next));
      }
    } catch (reason) {
      setError(billingMessage(reason, "We could not read that price list. Try CSV, Excel, PNG, JPEG or WebP."));
      setCandidates([]);
    } finally { setBusy(false); }
  }

  function remap(field: keyof Mapping, value: string) {
    const next = { ...mapping, [field]: value === "skip" ? null : Number(value) };
    setMapping(next); setCandidates(mappedRows(sourceRows, next));
  }

  const existingNames = new Set(existing.map((product) => product.name.trim().toLocaleLowerCase()));
  const ready = candidates.filter((row) => row.include && row.problem === undefined && !existingNames.has(row.name.toLocaleLowerCase()));

  async function runImport() {
    setBusy(true); setError(null);
    try {
      for (const row of ready) {
        const draft: ProductDraft = { name: row.name, unit: row.unit, unitPriceCents: Math.round(row.price * 100), vatRateBp: Math.round(row.vat * 100), ...(row.sku === "" ? {} : { sku: row.sku }) };
        await api.createProduct(draft);
      }
      setDone(ready.length);
    } catch (reason) { setError(billingMessage(reason, "The import stopped before every item could be saved.")); }
    finally { setBusy(false); }
  }

  const columnOptions = [{ value: "skip", label: "Not in this file" }, ...headers.map((header, index) => ({ value: String(index), label: header }))];
  return (
    <Modal title="Import price list" icon={<Upload className="size-5" />} onClose={onClose} wide tall
      actions={<button type="button" className="inline-flex size-9 items-center justify-center rounded-lg text-tertiary hover:!bg-accent-soft hover:!text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" aria-label="Close" onClick={onClose}><X className="size-4" /></button>}
      footer={<div className="ml-auto flex gap-3"><Button variant="ghost" onClick={onClose}>{done === null ? "Cancel" : "Close"}</Button>{done === null && candidates.length > 0 && <Button disabled={busy || ready.length === 0} onClick={() => void runImport()}>Import {ready.length} items</Button>}{done !== null && <Button onClick={onImported}>View price list</Button>}</div>}>
      <input ref={input} className="sr-only" type="file" accept=".csv,.xlsx,image/png,image/jpeg,image/webp" onChange={(event) => { const file = event.target.files?.[0]; if (file) void choose(file); }} />
      {candidates.length === 0 && done === null && (
        <button type="button" className="flex min-h-72 flex-1 flex-col items-center justify-center rounded-2xl border-2 border-dashed border-default !bg-raised/30 p-8 text-center transition-colors hover:!border-accent hover:!bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={() => input.current?.click()} onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); const file = event.dataTransfer.files[0]; if (file) void choose(file); }}>
          <span className="inline-flex size-14 items-center justify-center rounded-xl bg-accent-soft text-accent"><Upload className="size-6" /></span>
          <span className="mt-5 text-lg font-semibold text-primary">Drop a price list here</span>
          <span className="mt-2 max-w-lg text-sm leading-relaxed text-secondary">Excel and CSV are read instantly in your browser. For a photo or screenshot, alo AI extracts the rows for your review.</span>
          <span className="mt-5 inline-flex items-center gap-4 text-xs font-medium text-tertiary"><span className="inline-flex items-center gap-1.5"><FileSpreadsheet className="size-4" />CSV · XLSX</span><span className="inline-flex items-center gap-1.5"><Image className="size-4" />PNG · JPEG · WebP</span></span>
          <span className="mt-5 rounded-lg bg-surface px-4 py-2 text-sm font-semibold text-accent shadow-sm">Choose a file</span>
        </button>
      )}
      {busy && <div className="flex min-h-56 items-center justify-center text-sm text-secondary">Reading {fileName || "price list"}…</div>}
      {error !== null && <p className="rounded-xl border border-danger/20 bg-danger-tint px-4 py-3 text-sm text-danger" role="alert">{error}</p>}
      {!busy && done === null && candidates.length > 0 && <>
        <section className="flex items-center gap-3 rounded-xl border border-default bg-raised/40 px-4 py-3"><Check className="size-5 shrink-0 text-success" /><div className="min-w-0"><p className="text-sm font-semibold text-primary">{fileName}</p><p className="mt-1 text-xs text-secondary">{candidates.length} rows found. Review the mapping and exclude anything you do not want.</p></div><Button className="ml-auto" variant="ghost" size="sm" onClick={() => input.current?.click()}>Replace file</Button></section>
        {headers.length > 0 && <section><h3 className="text-sm font-semibold text-primary">Match your columns</h3><div className="mt-3 grid grid-cols-5 gap-3 max-lg:grid-cols-2">{(["name", "unit", "price", "vat", "sku"] as const).map((field) => <label key={field} className="flex flex-col gap-2"><span className="text-xs font-semibold uppercase tracking-wide text-tertiary">{field === "price" ? "Unit price" : field}</span><ChoicePicker value={mapping[field] === null ? "skip" : String(mapping[field])} label={`${field} column`} placeholder="Choose a column" options={columnOptions} onChange={(value) => remap(field, value)} /></label>)}</div></section>}
        <section className="min-h-0 overflow-auto rounded-xl border border-default bg-surface"><table className="w-full border-collapse text-sm"><thead className="sticky top-0 bg-raised"><tr><th className="px-4 py-3 text-left">Import</th><th className="px-4 py-3 text-left">Name</th><th className="px-4 py-3 text-left">Unit</th><th className="px-4 py-3 text-right">Price</th><th className="px-4 py-3 text-right">VAT</th><th className="px-4 py-3 text-left">Status</th></tr></thead><tbody>{candidates.map((row, index) => { const duplicate = existingNames.has(row.name.toLocaleLowerCase()); return <tr key={`${row.name}-${index}`} className="border-t border-subtle"><td className="px-4 py-3"><input type="checkbox" className="size-4 accent-[var(--accent)]" checked={row.include && !duplicate && row.problem === undefined} disabled={duplicate || row.problem !== undefined} onChange={(event) => setCandidates((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, include: event.target.checked } : item))} aria-label={`Import ${row.name || `row ${index + 1}`}`} /></td><td className="px-4 py-3 font-medium text-primary">{row.name || "—"}</td><td className="px-4 py-3 text-secondary">{row.unit || "—"}</td><td className="px-4 py-3 text-right tabular-nums">{row.price.toFixed(2)}</td><td className="px-4 py-3 text-right tabular-nums">{row.vat}%</td><td className="px-4 py-3 text-xs"><span className={duplicate || row.problem ? "text-danger" : "text-success"}>{duplicate ? "Already exists" : row.problem ?? "Ready"}</span></td></tr>; })}</tbody></table></section>
      </>}
      {done !== null && <div className="flex min-h-64 flex-col items-center justify-center text-center"><span className="inline-flex size-14 items-center justify-center rounded-full bg-success-tint text-success"><Check className="size-7" /></span><h3 className="mt-5 text-lg font-semibold text-primary">{done} price-list items imported</h3><p className="mt-2 text-sm text-secondary">They are ready to use in quotes, invoices and shared price connections.</p></div>}
    </Modal>
  );
}
