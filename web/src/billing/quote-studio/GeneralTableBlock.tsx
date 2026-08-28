import { Plus, Trash2 } from "lucide-react";

import { Select } from "../../ds";
import { strings } from "../../i18n";
import { generalTableHasContent } from "./generalTable";
import { InlineRichTextContent } from "./InlineRichTextContent";
import { InlineRichTextEditor } from "./InlineRichTextEditor";
import type { GeneralTable } from "./QuoteStudioBlock";
import { RichTextContent } from "./RichTextContent";

interface GeneralTableBlockProps {
  block: GeneralTable;
  readOnly: boolean;
  onChange: (patch: Partial<GeneralTable>) => void;
}

export function GeneralTableBlock({ block, readOnly, onChange }: GeneralTableBlockProps) {
  const setColumnCount = (count: number) => {
    const columns = block.columns.slice(0, count);
    while (columns.length < count) {
      const number = columns.length + 1;
      columns.push({ id: crypto.randomUUID(), label: strings.quoteStudioColumnNumber(number) });
    }
    onChange({
      columns,
      rows: block.rows.map((row) => ({
        ...row,
        cells: Object.fromEntries(columns.map((column) => [column.id, row.cells[column.id] ?? ""])),
      })),
    });
  };

  const removeColumn = (id: string) => {
    if (block.columns.length === 1) return;
    onChange({
      columns: block.columns.filter((column) => column.id !== id),
      rows: block.rows.map((row) => {
        const cells = { ...row.cells };
        delete cells[id];
        return { ...row, cells };
      }),
    });
  };

  const addRow = () => onChange({
    rows: [...block.rows, {
      id: crypto.randomUUID(),
      cells: Object.fromEntries(block.columns.map((column) => [column.id, ""])),
    }],
  });

  if (readOnly && !generalTableHasContent(block)) return null;

  if (readOnly) {
    return (
      <div className="overflow-x-auto rounded-xl border border-default">
        <table className="w-full border-collapse text-left text-sm">
          <thead className="bg-[var(--quote-table-header)]">
            <tr>{block.columns.map((column) => (
              <th key={column.id} className="px-4 py-3 font-semibold"><InlineRichTextContent value={column.label} /></th>
            ))}</tr>
          </thead>
          <tbody>{block.rows.map((row) => (
            <tr key={row.id} className="border-t border-default">
              {block.columns.map((column) => (
                <td key={column.id} className="px-4 py-3 align-top"><RichTextContent value={row.cells[column.id] ?? ""} /></td>
              ))}
            </tr>
          ))}</tbody>
        </table>
      </div>
    );
  }

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-primary">{strings.quoteStudioInformationTable}</h3>
          <p className="mt-1 text-xs text-secondary">{strings.quoteStudioInformationTableHelp}</p>
        </div>
        <label className="grid min-w-40 gap-1 text-xs font-semibold text-secondary">
          {strings.quoteStudioColumns}
          <Select fullWidth value={String(block.columns.length)} aria-label={strings.quoteStudioTableColumnCount} onChange={(event) => setColumnCount(Number(event.target.value))}>
            {[1, 2, 3, 4, 5, 6].map((count) => <option key={count} value={count}>{strings.quoteStudioColumnCount(count)}</option>)}
          </Select>
        </label>
      </div>
      <div className="overflow-x-auto rounded-xl border border-default">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-raised/50">
            <tr>
              {block.columns.map((column, columnIndex) => (
                <th key={column.id} className="group/table-column min-w-44 border-r border-default p-2 last:border-r-0">
                  <div className="mb-1 flex min-h-10 items-center justify-end">
                    <button type="button" className="grid size-9 shrink-0 place-items-center rounded-lg text-secondary opacity-0 transition-[color,background-color,opacity] hover:bg-danger-tint hover:text-danger focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 group-hover/table-column:opacity-100 group-focus-within/table-column:opacity-100 disabled:cursor-not-allowed disabled:opacity-35 max-md:opacity-100" aria-label={strings.quoteStudioRemoveColumnA11y(column.label || strings.quoteStudioColumnNumber(columnIndex + 1))} disabled={block.columns.length === 1} onClick={() => removeColumn(column.id)}>
                      <Trash2 className="size-4" aria-hidden="true" />
                    </button>
                  </div>
                  <InlineRichTextEditor value={column.label} aria-label={strings.quoteStudioColumnNameA11y(columnIndex + 1)} placeholder={strings.quoteStudioColumnNumber(columnIndex + 1)} onChange={(label) => onChange({ columns: block.columns.map((item) => item.id === column.id ? { ...item, label } : item) })} />
                </th>
              ))}
            </tr>
          </thead>
          {block.rows.map((row, rowIndex) => (
            <tbody key={row.id} className="group/table-row">
              <tr className="border-t border-default">
                <td colSpan={block.columns.length} className="px-2 pt-2">
                  <div className="flex min-h-10 items-center justify-end">
                    <button type="button" className="grid size-9 place-items-center rounded-lg text-secondary opacity-0 transition-[color,background-color,opacity] hover:bg-danger-tint hover:text-danger focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 group-hover/table-row:opacity-100 group-focus-within/table-row:opacity-100 max-md:opacity-100" aria-label={strings.quoteStudioRemoveRowA11y(rowIndex + 1)} onClick={() => onChange({ rows: block.rows.filter((item) => item.id !== row.id) })}>
                      <Trash2 className="size-4" aria-hidden="true" />
                    </button>
                  </div>
                </td>
              </tr>
              <tr>
                {block.columns.map((column, columnIndex) => (
                  <td key={column.id} className="border-r border-default p-2 last:border-r-0">
                    <InlineRichTextEditor value={row.cells[column.id] ?? ""} aria-label={strings.quoteStudioTableCellA11y(column.label || strings.quoteStudioColumnNumber(columnIndex + 1), rowIndex + 1)} placeholder={strings.quoteStudioEnterValue} onChange={(value) => onChange({ rows: block.rows.map((item) => item.id === row.id ? { ...item, cells: { ...item.cells, [column.id]: value } } : item) })} />
                  </td>
                ))}
              </tr>
            </tbody>
          ))}
        </table>
        {block.rows.length === 0 && <div className="px-5 py-8 text-center text-sm text-secondary">{strings.quoteStudioAddFirstRow}</div>}
      </div>
      <button type="button" className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent" onClick={addRow}>
        <Plus className="size-4" aria-hidden="true" /> {strings.quoteStudioAddRowBelow}
      </button>
    </div>
  );
}
