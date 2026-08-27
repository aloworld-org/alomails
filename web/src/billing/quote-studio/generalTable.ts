import type { GeneralTable } from "./QuoteStudioBlock";

export function generalTableHasContent(block: GeneralTable): boolean {
  return block.rows.some((row) =>
    block.columns.some((column) => (row.cells[column.id] ?? "").trim() !== ""),
  );
}
