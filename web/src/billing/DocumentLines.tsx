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
import { Plus, Trash2 } from "lucide-react";

import { IconButton, cx } from "../ds";
import { strings, useLocale } from "../i18n";
import { formatAmount, formatQty, formatRate } from "./money";
import { blankRow, isBlankRow, rowFromProduct, rowProblem } from "./lineRows";
import type { LineRow, RowProblem } from "./lineRows";
import type { BillingProduct, DocumentLine } from "./types";
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
  onChange,
  nextKey,
}: Props) {
  const locale = useLocale();

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

  return (
    <section className={styles.lines}>
      <div className={styles.linesHead}>
        <h2 className={styles.sectionTitle}>{strings.billingLines}</h2>
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
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.billingColDescription}</th>
                <th scope="col">{strings.billingColUnit}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColQty}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColUnitPrice}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColVatRate}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColNet}
                </th>
                {!readOnly && (
                  <th scope="col">
                    <span className={styles.srOnly}>{strings.billingColActions}</span>
                  </th>
                )}
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
                          <input
                            className={styles.input}
                            value={row.description}
                            onChange={(e) => replace(index, { ...row, description: e.target.value })}
                            placeholder={strings.billingDescriptionPlaceholder}
                            aria-label={strings.billingColDescription}
                            aria-invalid={problem === "description"}
                          />
                          {products.length > 0 && (
                            <select
                              className={styles.select}
                              value=""
                              aria-label={strings.billingPickProduct}
                              onChange={(e) => {
                                const picked = products.find((p) => p.id === e.target.value);
                                if (picked !== undefined) replace(index, rowFromProduct(row, picked));
                              }}
                            >
                              <option value="">{strings.billingPickProduct}</option>
                              {products.map((p) => (
                                <option key={p.id} value={p.id}>
                                  {p.name}
                                </option>
                              ))}
                            </select>
                          )}
                          {problem !== null && (
                            <span className={styles.fieldError}>{problemMessage(problem)}</span>
                          )}
                        </>
                      )}
                    </td>
                    <td>
                      {readOnly ? (
                        row.unit
                      ) : (
                        <input
                          className={cx(styles.input, styles.inputNarrow)}
                          value={row.unit}
                          onChange={(e) => replace(index, { ...row, unit: e.target.value })}
                          placeholder={strings.billingUnitPlaceholder}
                          aria-label={strings.billingColUnit}
                        />
                      )}
                    </td>
                    <td className={styles.numeric}>
                      {readOnly ? (
                        line === undefined ? (
                          row.qty
                        ) : (
                          formatQty(line.qtyMilli, locale)
                        )
                      ) : (
                        <input
                          className={cx(styles.input, styles.inputNarrow, styles.numeric)}
                          value={row.qty}
                          onChange={(e) => replace(index, { ...row, qty: e.target.value })}
                          placeholder={strings.billingQtyPlaceholder}
                          inputMode="decimal"
                          aria-label={strings.billingColQty}
                          aria-invalid={problem === "qty"}
                        />
                      )}
                    </td>
                    <td className={styles.numeric}>
                      {readOnly ? (
                        line === undefined ? (
                          row.price
                        ) : (
                          formatAmount(line.unitPriceCents, locale, currency)
                        )
                      ) : (
                        <input
                          className={cx(styles.input, styles.inputNarrow, styles.numeric)}
                          value={row.price}
                          onChange={(e) => replace(index, { ...row, price: e.target.value })}
                          placeholder={strings.billingAmountPlaceholder}
                          inputMode="decimal"
                          aria-label={strings.billingColUnitPrice}
                          aria-invalid={problem === "price"}
                        />
                      )}
                    </td>
                    <td className={styles.numeric}>
                      {readOnly ? (
                        line === undefined ? (
                          row.rate
                        ) : (
                          formatRate(line.vatRateBp, locale)
                        )
                      ) : (
                        <input
                          className={cx(styles.input, styles.inputNarrow, styles.numeric)}
                          value={row.rate}
                          onChange={(e) => replace(index, { ...row, rate: e.target.value })}
                          placeholder={strings.billingRatePlaceholder}
                          inputMode="decimal"
                          aria-label={strings.billingColVatRate}
                          aria-invalid={problem === "rate"}
                        />
                      )}
                    </td>
                    <td className={cx(styles.numeric, !saved && styles.stale)}>
                      {line === undefined ? "" : formatAmount(line.netCents, locale, currency)}
                    </td>
                    {!readOnly && (
                      <td className={styles.rowActions}>
                        <IconButton
                          label={strings.billingRemoveLine}
                          icon={<Trash2 size={15} />}
                          size="sm"
                          onClick={() => onChange(rows.filter((_, i) => i !== index))}
                        />
                      </td>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
