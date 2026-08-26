// The line grid of a billing document — what is being billed or offered, one
// row at a time. The same grid serves an invoice and a quote: the lines of the
// two are the same object, and a quote that showed its money differently from
// the invoice it becomes would be a quote nobody could trust.
//
// Every row is text (`lineRows.ts` owns turning it into a line); the only
// number this file renders is the server's own: the `net` column is the net
// the API computed for that line, and it is shown **only while the draft is
// saved**. A net next to a quantity the server has not seen yet would be a
// figure the browser had made up, which is the one thing the billing surface
// never does.
import { useState } from "react";
import { GripVertical, PackageOpen, Plus, Trash2 } from "lucide-react";

import { ChoicePicker, IconButton, Input, Table, Td, Th, cx } from "../ds";
import { strings, useLocale } from "../i18n";
import { formatAmount, formatQty, formatRate } from "./money";
import { blankRow, isBlankRow, rowFromProduct, rowProblem } from "./lineRows";
import type { LineRow, RowProblem } from "./lineRows";
import type { BillingProduct, DocumentLine } from "./types";
import type { QuoteColumns } from "./QuoteContentStudio";
import styles from "./billingStyles";

/** What to say about the row's first problem. A blank description is the
 *  common one — it is how a freshly added row starts — so it is worded as an
 *  instruction rather than as a complaint. */
function problemMessage(problem: RowProblem): string {
  switch (problem) {
    case "description":
      return strings.billingLineNeedsDescription;
    case "qty":
      return strings.billingNotAQuantity;
    case "rate":
      return strings.billingNotARate;
    default:
      return strings.billingNotAnAmount;
  }
}

interface Props {
  rows: LineRow[];
  /** The price list to pick from; archived items are not offered. */
  products: BillingProduct[];
  /** The lines as the server last stored them, in print order. */
  savedLines: DocumentLine[];
  /** Whether those saved lines still describe what the rows say. */
  saved: boolean;
  currency: string;
  readOnly: boolean;
  columns?: QuoteColumns | undefined;
  title?: string | undefined;
  onChange: (rows: LineRow[]) => void;
  /** Called for a fresh row's key, so identity comes from the editor that owns
   *  the document rather than from a counter that resets on every render. */
  nextKey: () => string;
}

export function DocumentLines({
  rows,
  products,
  savedLines,
  saved,
  currency,
  readOnly,
  columns,
  title,
  onChange,
  nextKey,
}: Props) {
  const locale = useLocale();
  const [draggedKey, setDraggedKey] = useState<string | null>(null);
  const [dropTargetKey, setDropTargetKey] = useState<string | null>(null);

  // The server replaces the whole set in the order sent, so the n-th line it
  // stored is the n-th row that was worth sending. That pairing is what lets a
  // row show a net it did not compute — and, on a frozen document, show every
  // figure exactly as the document carries it.
  const stored = new Map<string, DocumentLine>();
  let taken = 0;
  for (const row of rows) {
    if (isBlankRow(row)) continue;
    const line = savedLines[taken];
    taken += 1;
    if (line !== undefined) stored.set(row.key, line);
  }

  function replace(index: number, row: LineRow) {
    onChange(rows.map((r, i) => (i === index ? row : r)));
  }

  function move(from: number, to: number) {
    if (
      from === to ||
      from < 0 ||
      to < 0 ||
      from >= rows.length ||
      to >= rows.length
    )
      return;
    const next = [...rows];
    const [row] = next.splice(from, 1);
    if (row === undefined) return;
    next.splice(to, 0, row);
    onChange(next);
  }

  if (!readOnly) {
    return (
      <section className="flex flex-col gap-4">
        <div>
          <h2 className="text-base font-semibold text-primary">
            {title ?? strings.billingLines}
          </h2>
          <p className="mt-1 text-sm text-secondary">
            Add, edit, remove, or drag products and services into the right
            order.
          </p>
        </div>
        {rows.length === 0 ? (
          <div className="flex min-h-40 flex-col items-center justify-center rounded-xl border border-dashed border-default bg-surface px-6 text-center">
            <PackageOpen className="size-6 text-accent" aria-hidden="true" />
            <p className="mt-3 font-semibold text-primary">
              {strings.billingNoLines}
            </p>
            <p className="mt-1 text-sm text-secondary">
              Add a service, product, fee, or discount to continue.
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            {rows.map((row, index) => {
              const problem = isBlankRow(row) ? null : rowProblem(row);
              const line = stored.get(row.key);
              return (
                <article
                  key={row.key}
                  className={cx(
                    "relative rounded-xl border border-default bg-surface p-4 pl-14 shadow-sm transition-all",
                    draggedKey === row.key &&
                      "scale-[0.995] border-accent bg-accent-soft/30 opacity-45 shadow-lg",
                    dropTargetKey === row.key &&
                      draggedKey !== row.key &&
                      "border-accent bg-accent-soft/40 ring-2 ring-accent/20",
                  )}
                  onDragEnter={() => {
                    if (draggedKey !== null && draggedKey !== row.key)
                      setDropTargetKey(row.key);
                  }}
                  onDragOver={(event) => {
                    if (draggedKey !== null) {
                      event.preventDefault();
                      event.dataTransfer.dropEffect = "move";
                    }
                  }}
                  onDragLeave={(event) => {
                    if (
                      !event.currentTarget.contains(event.relatedTarget as Node)
                    )
                      setDropTargetKey((current) =>
                        current === row.key ? null : current,
                      );
                  }}
                  onDrop={(event) => {
                    event.preventDefault();
                    const from = rows.findIndex(
                      (item) => item.key === draggedKey,
                    );
                    move(from, index);
                    setDraggedKey(null);
                    setDropTargetKey(null);
                  }}
                >
                  <button
                    type="button"
                    draggable
                    className="absolute left-3 top-1/2 grid size-9 -translate-y-1/2 cursor-grab place-items-center rounded-lg text-tertiary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 active:cursor-grabbing"
                    aria-label={`Reorder ${row.description || `line ${index + 1}`}. Use arrow keys to move it.`}
                    onDragStart={(event) => {
                      setDraggedKey(row.key);
                      event.dataTransfer.effectAllowed = "move";
                      event.dataTransfer.setData("text/plain", row.key);
                      const card = event.currentTarget.closest("article");
                      if (card instanceof HTMLElement) {
                        const bounds = card.getBoundingClientRect();
                        event.dataTransfer.setDragImage(
                          card,
                          event.clientX - bounds.left,
                          event.clientY - bounds.top,
                        );
                      }
                    }}
                    onDragEnd={() => {
                      setDraggedKey(null);
                      setDropTargetKey(null);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "ArrowUp") {
                        event.preventDefault();
                        move(index, index - 1);
                      }
                      if (event.key === "ArrowDown") {
                        event.preventDefault();
                        move(index, index + 1);
                      }
                    }}
                  >
                    <GripVertical className="size-5" aria-hidden="true" />
                  </button>
                  <div className="grid gap-4 xl:grid-cols-[minmax(280px,2fr)_minmax(120px,.6fr)_minmax(110px,.5fr)_minmax(130px,.65fr)_minmax(110px,.5fr)_minmax(130px,.7fr)_40px] xl:items-end">
                    <div className="flex min-w-0 flex-col gap-2 xl:row-span-2">
                      <label className="text-xs font-semibold uppercase tracking-wide text-tertiary">
                        {strings.billingColDescription}
                      </label>
                      <Input
                        value={row.description}
                        onChange={(event) =>
                          replace(index, {
                            ...row,
                            description: event.target.value,
                          })
                        }
                        placeholder={strings.billingDescriptionPlaceholder}
                        invalid={problem === "description"}
                      />
                      {products.length > 0 && (
                        <ChoicePicker
                          value=""
                          label={strings.billingPickProduct}
                          placeholder={strings.billingPickProduct}
                          options={products
                            .filter((product) => !product.archived)
                            .map((product) => ({
                              value: product.id,
                              label: product.name,
                            }))}
                          onChange={(productId) => {
                            const picked = products.find(
                              (product) => product.id === productId,
                            );
                            if (picked)
                              replace(index, rowFromProduct(row, picked));
                          }}
                        />
                      )}
                      {problem !== null && (
                        <span className={styles.fieldError}>
                          {problemMessage(problem)}
                        </span>
                      )}
                    </div>
                    <LineField label={strings.billingColUnit}>
                      <Input
                        value={row.unit}
                        onChange={(event) =>
                          replace(index, { ...row, unit: event.target.value })
                        }
                        placeholder={strings.billingUnitPlaceholder}
                      />
                    </LineField>
                    <LineField label={strings.billingColQty}>
                      <Input
                        className={styles.numeric}
                        value={row.qty}
                        onChange={(event) =>
                          replace(index, { ...row, qty: event.target.value })
                        }
                        placeholder={strings.billingQtyPlaceholder}
                        inputMode="decimal"
                        invalid={problem === "qty"}
                      />
                    </LineField>
                    <LineField label={strings.billingColUnitPrice}>
                      <Input
                        className={styles.numeric}
                        value={row.price}
                        onChange={(event) =>
                          replace(index, { ...row, price: event.target.value })
                        }
                        placeholder={strings.billingAmountPlaceholder}
                        inputMode="decimal"
                        invalid={problem === "price"}
                      />
                    </LineField>
                    <LineField label={strings.billingColVatRate}>
                      <Input
                        className={styles.numeric}
                        value={row.rate}
                        onChange={(event) =>
                          replace(index, { ...row, rate: event.target.value })
                        }
                        placeholder={strings.billingRatePlaceholder}
                        inputMode="decimal"
                        invalid={problem === "rate"}
                      />
                    </LineField>
                    <div className="min-w-0">
                      <span className="block text-xs font-semibold uppercase tracking-wide text-tertiary">
                        {strings.billingColNet}
                      </span>
                      <strong
                        className={cx(
                          "mt-3 block truncate text-base font-semibold tabular-nums text-primary",
                          !saved && styles.stale,
                        )}
                      >
                        {line === undefined
                          ? "—"
                          : formatAmount(line.netCents, locale, currency)}
                      </strong>
                    </div>
                    <IconButton
                      label={strings.billingRemoveLine}
                      icon={<Trash2 size={16} />}
                      onClick={() =>
                        onChange(
                          rows.filter((_, itemIndex) => itemIndex !== index),
                        )
                      }
                    />
                  </div>
                </article>
              );
            })}
          </div>
        )}
        <div className="flex justify-end pt-1">
          <ButtonLine
            onClick={() => onChange([...rows, blankRow(nextKey())])}
          />
        </div>
      </section>
    );
  }

  return (
    <section className={styles.lines}>
      <div className={styles.linesHead}>
        <h2 className={styles.sectionTitle}>{title ?? strings.billingLines}</h2>
        {!readOnly && (
          <button
            type="button"
            className={styles.linkAction}
            onClick={() => onChange([...rows, blankRow(nextKey())])}
          >
            <Plus size={14} aria-hidden="true" /> {strings.billingAddLine}
          </button>
        )}
      </div>

      {rows.length === 0 ? (
        <p className={styles.noMatches}>{strings.billingNoLines}</p>
      ) : (
        <Table
          label={title ?? strings.billingLines}
          className="bg-[var(--quote-table-row,var(--bg-surface))] [&_thead_th]:!bg-[var(--quote-table-header,var(--bg-surface))] [&_td]:!text-[var(--quote-text,var(--text-primary))]"
        >
          <thead>
            <tr>
              <Th>{strings.billingColDescription}</Th>
              {(columns?.unit ?? true) && (
                <Th className={styles.narrowCol}>{strings.billingColUnit}</Th>
              )}
              {(columns?.quantity ?? true) && (
                <Th numeric className={styles.narrowCol}>
                  {strings.billingColQty}
                </Th>
              )}
              {(columns?.unitPrice ?? true) && (
                <Th numeric className={styles.narrowCol}>
                  {strings.billingColUnitPrice}
                </Th>
              )}
              {(columns?.vat ?? true) && (
                <Th numeric className={styles.narrowCol}>
                  {strings.billingColVatRate}
                </Th>
              )}
              {(columns?.net ?? true) && (
                <Th numeric>{strings.billingColNet}</Th>
              )}
              {!readOnly && <Th hideLabel>{strings.billingColActions}</Th>}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, index) => {
              const problem = isBlankRow(row) ? null : rowProblem(row);
              const line = stored.get(row.key);
              return (
                <tr key={row.key}>
                  <td className={styles.lineDescription}>
                    {readOnly ? (
                      row.description
                    ) : (
                      <>
                        <Input
                          value={row.description}
                          onChange={(e) =>
                            replace(index, {
                              ...row,
                              description: e.target.value,
                            })
                          }
                          placeholder={strings.billingDescriptionPlaceholder}
                          aria-label={strings.billingColDescription}
                          invalid={problem === "description"}
                        />
                        {products.length > 0 && (
                          <ChoicePicker
                            value=""
                            label={strings.billingPickProduct}
                            placeholder={strings.billingPickProduct}
                            options={products.map((product) => ({
                              value: product.id,
                              label: product.name,
                            }))}
                            onChange={(productId) => {
                              const picked = products.find(
                                (product) => product.id === productId,
                              );
                              if (picked !== undefined)
                                replace(index, rowFromProduct(row, picked));
                            }}
                          />
                        )}
                        {problem !== null && (
                          <span className={styles.fieldError}>
                            {problemMessage(problem)}
                          </span>
                        )}
                      </>
                    )}
                  </td>
                  {(columns?.unit ?? true) && (
                    <td>
                      {readOnly ? (
                        row.unit
                      ) : (
                        <Input
                          value={row.unit}
                          onChange={(e) =>
                            replace(index, { ...row, unit: e.target.value })
                          }
                          placeholder={strings.billingUnitPlaceholder}
                          aria-label={strings.billingColUnit}
                        />
                      )}
                    </td>
                  )}
                  {(columns?.quantity ?? true) && (
                    <Td numeric>
                      {readOnly ? (
                        line === undefined ? (
                          row.qty
                        ) : (
                          formatQty(line.qtyMilli, locale)
                        )
                      ) : (
                        <Input
                          className={styles.numeric}
                          value={row.qty}
                          onChange={(e) =>
                            replace(index, { ...row, qty: e.target.value })
                          }
                          placeholder={strings.billingQtyPlaceholder}
                          inputMode="decimal"
                          aria-label={strings.billingColQty}
                          invalid={problem === "qty"}
                        />
                      )}
                    </Td>
                  )}
                  {(columns?.unitPrice ?? true) && (
                    <Td numeric>
                      {readOnly ? (
                        line === undefined ? (
                          row.price
                        ) : (
                          formatAmount(line.unitPriceCents, locale, currency)
                        )
                      ) : (
                        <Input
                          className={styles.numeric}
                          value={row.price}
                          onChange={(e) =>
                            replace(index, { ...row, price: e.target.value })
                          }
                          placeholder={strings.billingAmountPlaceholder}
                          inputMode="decimal"
                          aria-label={strings.billingColUnitPrice}
                          invalid={problem === "price"}
                        />
                      )}
                    </Td>
                  )}
                  {(columns?.vat ?? true) && (
                    <Td numeric>
                      {readOnly ? (
                        line === undefined ? (
                          row.rate
                        ) : (
                          formatRate(line.vatRateBp, locale)
                        )
                      ) : (
                        <Input
                          className={styles.numeric}
                          value={row.rate}
                          onChange={(e) =>
                            replace(index, { ...row, rate: e.target.value })
                          }
                          placeholder={strings.billingRatePlaceholder}
                          inputMode="decimal"
                          aria-label={strings.billingColVatRate}
                          invalid={problem === "rate"}
                        />
                      )}
                    </Td>
                  )}
                  {(columns?.net ?? true) && (
                    <Td numeric className={cx(!saved && styles.stale)}>
                      {line === undefined
                        ? ""
                        : formatAmount(line.netCents, locale, currency)}
                    </Td>
                  )}
                  {!readOnly && (
                    <td className={styles.rowActions}>
                      <IconButton
                        label={strings.billingRemoveLine}
                        icon={<Trash2 size={15} />}
                        size="sm"
                        onClick={() =>
                          onChange(rows.filter((_, i) => i !== index))
                        }
                      />
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </Table>
      )}
    </section>
  );
}

function LineField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="min-w-0">
      <span className="mb-2 block text-xs font-semibold uppercase tracking-wide text-tertiary">
        {label}
      </span>
      {children}
    </label>
  );
}

function ButtonLine({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="inline-flex min-h-10 items-center gap-2 rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-on-accent transition-colors hover:bg-accent-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
      onClick={onClick}
    >
      <Plus className="size-4" aria-hidden="true" />
      {strings.billingAddLine}
    </button>
  );
}
