// The line grid of an order — what is being bought, or what is being sold, one
// row at a time (B5.09b).
//
// One grid for both documents, because the lines of the two are the same object
// pointed in opposite directions. What differs is stated as props and nothing
// else: which price a picked product is copied at, and which columns of
// *progress* a placed order shows — how much has arrived, or how much has gone
// out and been billed.
//
// **Every number in a shaded column is the server's.** The net beside a line is
// the net the API computed for it, and it is shown only while the draft is
// saved; the progress columns are the store's own derived quantities, handed in
// already formatted by the screen that read them. A figure this browser worked
// out from two others would be a second opinion about a warehouse, and the one
// place a second opinion always loses is a stocktake.
//
// **A row remembers which catalog item it is.** Picking a product writes the
// link onto the row, and the link is what a receipt or a delivery later turns
// into goods actually moving. A line typed by hand has no link and moves
// nothing — which is right: freight does not arrive on a pallet.
import { Plus, Trash2 } from "lucide-react";

import { formatAmount, formatQty, formatRate, isBlankRow, type BillingProduct } from "../billing";
import { IconButton, cx } from "../ds";
import { strings, useLocale } from "../i18n";
import {
  blankOrderRow,
  orderRowFromProduct,
  orderRowProblem,
  type OrderRow,
  type PriceSide,
} from "./orderRows";
import type { OrderLine } from "./types";
import styles from "./InventoryModule.module.css";

/** One right-hand column of a placed order: a heading, and one already-rendered
 *  cell per stored line, in the order the document carries them. */
export interface ProgressColumn {
  key: string;
  label: string;
  values: string[];
}

interface Props {
  rows: OrderRow[];
  /** The catalog to pick from; archived items are not offered. */
  products: BillingProduct[];
  /** Which price picking one copies — the direction the goods go. */
  priceSide: PriceSide;
  /** The lines as the server last stored them, in document order. */
  savedLines: OrderLine[];
  /** Whether those stored lines still describe what the rows say. */
  saved: boolean;
  currency: string;
  readOnly: boolean;
  /** What has already happened to each line. Empty on a draft, where nothing
   *  has. */
  progress?: ProgressColumn[];
  onChange: (rows: OrderRow[]) => void;
  /** A fresh row's key, from the screen that owns the document rather than from
   *  a counter that resets on every render. */
  nextKey: () => string;
}

/** What to say about a row's first problem. A blank description is the common
 *  one — it is how a freshly added row starts — so it is worded as an
 *  instruction rather than as a complaint. */
function problemMessage(problem: NonNullable<ReturnType<typeof orderRowProblem>>): string {
  switch (problem) {
    case "description":
      return strings.inventoryLineNeedsDescription;
    case "qty":
      return strings.inventoryNotAQuantity;
    case "rate":
      return strings.inventoryNotARate;
    default:
      return strings.inventoryNotAnAmount;
  }
}

export function OrderLines({
  rows,
  products,
  priceSide,
  savedLines,
  saved,
  currency,
  readOnly,
  progress = [],
  onChange,
  nextKey,
}: Props) {
  const locale = useLocale();

  // Which stored line each row stands for. The server replaces the whole set in
  // the order sent, so the n-th stored line is the n-th row that was *worth*
  // sending — a wholly blank row is dropped on save and must not consume a
  // line here either, or a net and a received quantity would slide one row down
  // the moment somebody clicked "add line" and paused.
  const storedIndex = new Map<string, number>();
  let taken = 0;
  for (const row of rows) {
    if (isBlankRow(row)) continue;
    storedIndex.set(row.key, taken);
    taken += 1;
  }

  function replace(index: number, row: OrderRow) {
    onChange(rows.map((r, i) => (i === index ? row : r)));
  }

  return (
    <section className={styles.lines}>
      <div className={styles.linesHead}>
        <h2 className={styles.sectionTitle}>{strings.inventoryLines}</h2>
        {!readOnly && (
          <button
            type="button"
            className={styles.linkAction}
            onClick={() => onChange([...rows, blankOrderRow(nextKey())])}
          >
            <Plus size={14} aria-hidden="true" /> {strings.inventoryAddLine}
          </button>
        )}
      </div>

      {rows.length === 0 ? (
        <p className={styles.noMatches}>{strings.inventoryNoLines}</p>
      ) : (
        <div className={styles.tableWrap} data-allow-overflow="">
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.inventoryColDescription}</th>
                <th scope="col">{strings.inventoryColUnit}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColQuantity}
                </th>
                {progress.map((column) => (
                  <th key={column.key} scope="col" className={styles.numeric}>
                    {column.label}
                  </th>
                ))}
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColUnitPrice}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColVatRate}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColNet}
                </th>
                {!readOnly && (
                  <th scope="col">
                    <span className={styles.srOnly}>{strings.inventoryColActions}</span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, index) => {
                const problem = orderRowProblem(row);
                const stored = storedIndex.get(row.key);
                const line = stored === undefined ? undefined : savedLines[stored];
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
                            onChange={(e) =>
                              replace(index, { ...row, description: e.target.value })
                            }
                            placeholder={strings.inventoryDescriptionPlaceholder}
                            aria-label={strings.inventoryColDescription}
                            aria-invalid={problem === "description"}
                          />
                          {products.length > 0 && (
                            <select
                              className={styles.select}
                              value={row.productId}
                              aria-label={strings.inventoryPickProduct}
                              onChange={(e) => {
                                const picked = products.find((p) => p.id === e.target.value);
                                replace(
                                  index,
                                  picked === undefined
                                    ? // Clearing the picker unlinks the line
                                      // without rewriting what somebody typed:
                                      // the words stay, the goods stop moving.
                                      { ...row, productId: "" }
                                    : orderRowFromProduct(row, picked, priceSide),
                                );
                              }}
                            >
                              <option value="">{strings.inventoryPickProduct}</option>
                              {products.map((product) => (
                                <option key={product.id} value={product.id}>
                                  {product.name}
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
                          placeholder={strings.inventoryUnitPlaceholder}
                          aria-label={strings.inventoryColUnit}
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
                          placeholder={strings.inventoryQtyPlaceholder}
                          inputMode="decimal"
                          aria-label={strings.inventoryColQuantity}
                          aria-invalid={problem === "qty"}
                        />
                      )}
                    </td>
                    {progress.map((column) => (
                      <td key={column.key} className={cx(styles.numeric, styles.muted)}>
                        {stored === undefined ? "" : (column.values[stored] ?? "")}
                      </td>
                    ))}
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
                          placeholder={strings.inventoryAmountPlaceholder}
                          inputMode="decimal"
                          aria-label={strings.inventoryColUnitPrice}
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
                          placeholder={strings.inventoryRatePlaceholder}
                          inputMode="decimal"
                          aria-label={strings.inventoryColVatRate}
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
                          label={strings.inventoryRemoveLine}
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
