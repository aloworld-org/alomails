import { useRef, useState } from "react";
import { Check, FileSpreadsheet, Image, Upload, X } from "lucide-react";

import { Button, ChoicePicker, Modal } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { importNumber, readPriceImport, suggestColumn, type ImportCell } from "./priceImport";
import type { BillingProduct, ProductDraft } from "./types";

type Candidate = { name: string; unit: string; price: number; vat: number; sku: string; include: boolean; problem?: string };
type Mapping = { name: number | null; unit: number | null; price: number | null; vat: number | null; sku: number | null };

function fileDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error(strings.billingImportImageUnreadable));
    reader.onerror = () => reject(new Error(strings.billingImportImageUnreadable));
    reader.readAsDataURL(file);
  });
}

function cell(row: ImportCell[], column: number | null): ImportCell { return column === null ? "" : (row[column] ?? ""); }

function mappedRows(rows: ImportCell[][], mapping: Mapping): Candidate[] {
  return rows.map((row) => {
    const name = String(cell(row, mapping.name)).trim();
    const price = importNumber(cell(row, mapping.price));
    const vat = importNumber(cell(row, mapping.vat)) ?? 0;
    const problem = name === "" ? strings.billingImportMissingName : price === null || price < 0 ? strings.billingImportInvalidPrice : vat < 0 ? strings.billingImportInvalidVat : undefined;
    return { name, unit: String(cell(row, mapping.unit)).trim(), price: price ?? 0, vat, sku: String(cell(row, mapping.sku)).trim(), include: problem === undefined, ...(problem === undefined ? {} : { problem }) };
  });
}

export function PriceImportDialog({ existing, onClose, onImported }: { existing: BillingProduct[]; onClose: () => void; onImported: () => void }) {
  const api = useBillingApi();
  const locale = useLocale();
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
      setError(billingMessage(reason, strings.billingImportReadFailed));
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
    } catch (reason) { setError(billingMessage(reason, strings.billingImportSaveFailed)); }
    finally { setBusy(false); }
  }

  const columnOptions = [{ value: "skip", label: strings.billingImportNotInFile }, ...headers.map((header, index) => ({ value: String(index), label: header }))];
  return (
    <Modal title={strings.billingImportTitle} icon={<Upload className="size-5" />} onClose={onClose} wide tall
      actions={<button type="button" className="inline-flex size-9 items-center justify-center rounded-lg text-tertiary hover:!bg-accent-soft hover:!text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" aria-label={strings.close} onClick={onClose}><X className="size-4" /></button>}
      footer={<div className="ml-auto flex gap-3"><Button variant="ghost" onClick={onClose}>{done === null ? strings.billingCancel : strings.close}</Button>{done === null && candidates.length > 0 && <Button disabled={busy || ready.length === 0} onClick={() => void runImport()}>{strings.billingImportItems(ready.length)}</Button>}{done !== null && <Button onClick={onImported}>{strings.billingImportViewPriceList}</Button>}</div>}>
      <input ref={input} className="sr-only" type="file" accept=".csv,.xlsx,image/png,image/jpeg,image/webp" onChange={(event) => { const file = event.target.files?.[0]; if (file) void choose(file); }} />
      {candidates.length === 0 && done === null && (
        <button type="button" className="flex min-h-72 flex-1 flex-col items-center justify-center rounded-2xl border-2 border-dashed border-default !bg-raised/30 p-8 text-center transition-colors hover:!border-accent hover:!bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={() => input.current?.click()} onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); const file = event.dataTransfer.files[0]; if (file) void choose(file); }}>
          <span className="inline-flex size-14 items-center justify-center rounded-xl bg-accent-soft text-accent"><Upload className="size-6" /></span>
          <span className="mt-5 text-lg font-semibold text-primary">{strings.billingImportDropTitle}</span>
          <span className="mt-2 max-w-lg text-sm leading-relaxed text-secondary">{strings.billingImportDropHelp}</span>
          <span className="mt-5 inline-flex items-center gap-4 text-xs font-medium text-tertiary"><span className="inline-flex items-center gap-1.5"><FileSpreadsheet className="size-4" />{strings.billingImportSpreadsheetFormats}</span><span className="inline-flex items-center gap-1.5"><Image className="size-4" />{strings.billingImportImageFormats}</span></span>
          <span className="mt-5 rounded-lg bg-surface px-4 py-2 text-sm font-semibold text-accent shadow-sm">{strings.billingImportChooseFile}</span>
        </button>
      )}
      {busy && <div className="flex min-h-56 items-center justify-center text-sm text-secondary">{strings.billingImportReading(fileName || strings.billingPriceList)}</div>}
      {error !== null && <p className="rounded-xl border border-danger/20 bg-danger-tint px-4 py-3 text-sm text-danger" role="alert">{error}</p>}
      {!busy && done === null && candidates.length > 0 && <>
        <section className="flex items-center gap-3 rounded-xl border border-default bg-raised/40 px-4 py-3"><Check className="size-5 shrink-0 text-success" /><div className="min-w-0"><p className="text-sm font-semibold text-primary">{fileName}</p><p className="mt-1 text-xs text-secondary">{strings.billingImportRowsFound(candidates.length)}</p></div><Button className="ml-auto" variant="ghost" size="sm" onClick={() => input.current?.click()}>{strings.billingImportReplaceFile}</Button></section>
        {headers.length > 0 && <section><h3 className="text-sm font-semibold text-primary">{strings.billingImportMatchColumns}</h3><div className="mt-3 grid grid-cols-5 gap-3 max-lg:grid-cols-2">{(["name", "unit", "price", "vat", "sku"] as const).map((field) => { const fieldLabel = field === "name" ? strings.billingFieldName : field === "unit" ? strings.billingColUnit : field === "price" ? strings.billingColUnitPrice : field === "vat" ? strings.billingColVat : strings.billingImportSku; return <label key={field} className="flex flex-col gap-2"><span className="text-xs font-semibold uppercase tracking-wide text-tertiary">{fieldLabel}</span><ChoicePicker value={mapping[field] === null ? "skip" : String(mapping[field])} label={strings.billingImportColumnLabel(fieldLabel)} placeholder={strings.billingImportChooseColumn} options={columnOptions} onChange={(value) => remap(field, value)} /></label>; })}</div></section>}
        <section className="min-h-0 overflow-auto rounded-xl border border-default bg-surface"><table className="w-full border-collapse text-sm"><thead className="sticky top-0 bg-raised"><tr><th className="px-4 py-3 text-left">{strings.billingImportColumn}</th><th className="px-4 py-3 text-left">{strings.billingFieldName}</th><th className="px-4 py-3 text-left">{strings.billingColUnit}</th><th className="px-4 py-3 text-right">{strings.billingColUnitPrice}</th><th className="px-4 py-3 text-right">{strings.billingColVat}</th><th className="px-4 py-3 text-left">{strings.billingColStatus}</th></tr></thead><tbody>{candidates.map((row, index) => { const duplicate = existingNames.has(row.name.toLocaleLowerCase()); return <tr key={`${row.name}-${index}`} className="border-t border-subtle"><td className="px-4 py-3"><input type="checkbox" className="size-4 accent-[var(--accent)]" checked={row.include && !duplicate && row.problem === undefined} disabled={duplicate || row.problem !== undefined} onChange={(event) => setCandidates((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, include: event.target.checked } : item))} aria-label={strings.billingImportIncludeRow(row.name || strings.billingImportRow(index + 1))} /></td><td className="px-4 py-3 font-medium text-primary">{row.name || "—"}</td><td className="px-4 py-3 text-secondary">{row.unit || "—"}</td><td className="px-4 py-3 text-right tabular-nums">{new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(row.price)}</td><td className="px-4 py-3 text-right tabular-nums">{new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 2 }).format(row.vat / 100)}</td><td className="px-4 py-3 text-xs"><span className={duplicate || row.problem ? "text-danger" : "text-success"}>{duplicate ? strings.billingImportAlreadyExists : row.problem ?? strings.billingImportReady}</span></td></tr>; })}</tbody></table></section>
      </>}
      {done !== null && <div className="flex min-h-64 flex-col items-center justify-center text-center"><span className="inline-flex size-14 items-center justify-center rounded-full bg-success-tint text-success"><Check className="size-7" /></span><h3 className="mt-5 text-lg font-semibold text-primary">{strings.billingImportComplete(done)}</h3><p className="mt-2 text-sm text-secondary">{strings.billingImportCompleteHelp}</p></div>}
    </Modal>
  );
}
