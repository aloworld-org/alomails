// One billing document on screen: the customer it is for, what is on it, what
// the server says it comes to, and what can be done to it next.
//
// It is the shell both the invoice editor and the quote editor render, because
// the two screens are the same screen with different words and different
// transitions. A quote a salesperson sends and the invoice it becomes must
// show the same lines, the same totals and the same "this is frozen now"
// story — mirroring them by construction is the only way that stays true.
//
// What varies is passed in: the words (`labels`), the two dates this kind of
// document carries, the state chips, the lifecycle actions, and whatever the
// record shows about its relations (`footer`). What does not vary lives here:
// the header fields, the line grid, the totals panel, the save indicator, the
// create bar, and the read-only rendering of a document that carries a number.
import { useState } from "react";
import type { ReactNode } from "react";
import {
  ArrowLeft,
  Check,
  FilePlus2,
  Plus,
  Printer,
  Search,
  X,
} from "lucide-react";

import { Button, ChoicePicker, Input, Spinner, cx, useDialogs } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage } from "./api";
import { DocumentActions } from "./DocumentActions";
import type { DocumentAction } from "./DocumentActions";
import { CreationTemplatePreview } from "./CreationTemplatePreview";
import { DocumentLines } from "./DocumentLines";
import type { QuoteColumns } from "./QuoteContentStudio";
import type { DocumentDraft, StoredDocument } from "./documentDraft";
import { blankRow, isBlankRow, rowFromProduct } from "./lineRows";
import type { LineRow } from "./lineRows";
import { formatAmount, formatRate, parseHundredths } from "./money";
import { DialogFrame, ErrorBanner, Field } from "./parts";
import type { Pickers } from "./pickers";
import { printSheet } from "./printSheet";
import type {
  QuoteTotalsDetail,
  QuoteTotalsPlacement,
} from "./quoteTableOptions";
import { TotalsPanel } from "./TotalsPanel";
import styles from "./billingStyles";

/** Everything this shell says out loud, so no English lives in it. */
export interface DocumentEditorLabels {
  /** The heading before the document exists ("New invoice"). */
  newTitle: string;
  /** The heading of a document that has no number yet ("Draft invoice"). */
  draftTitle: string;
  /** The link back to the list ("All invoices"). */
  back: string;
  /** Shown when the id in the URL names nothing. */
  gone: string;
  /** What picking a customer decides about the document. */
  customerHint: string;
  createHint: string;
  createLabel: string;
  /** Discarding a draft — the label, and what the dialog says first. */
  discardLabel: string;
  discardMessage: string;
  /** Why a numbered document offers no edits, when that needs a page notice. */
  frozenNotice: string | null;
}

export interface CreationTemplate {
  key: string;
  name: string;
  description: string;
  preview: "blank" | "services" | "project" | "retainer";
  buildRows: (nextKey: () => string) => LineRow[];
}

interface Props<T extends StoredDocument, A> {
  draft: DocumentDraft<T, A>;
  pickers: Pickers;
  labels: DocumentEditorLabels;
  /** The state chips of this document, or nothing before it exists. */
  chips: ReactNode;
  /** The two dates this kind of document carries, as `Field`s. */
  dates: ReactNode;
  /** What can be done to the document from here. Empty on a closed one. */
  actions: DocumentAction[];
  /** Optional document-specific presentation above the accounting fields. */
  lead?: ReactNode;
  /** Whether to show the editable document metadata panel. */
  showSummary?: boolean;
  /** Optional customer-facing pricing columns chosen for a quotation. */
  lineColumns?: QuoteColumns;
  /** Rich content between the document details and its commercial lines. A
   *  quotation canvas receives the pricing table so it can place it like any
   *  other document block. */
  documentBody?:
    | ReactNode
    | ((
        pricingTable: (options: {
          rowKeys?: string[];
          title?: string;
          onRowKeysChange: (keys: string[]) => void;
        }) => ReactNode,
        totals: ReactNode,
        lineKeys: string[],
        tableSubtotal: (
          rowKeys?: string[],
          presentation?: {
            placement: QuoteTotalsPlacement;
            detail: QuoteTotalsDetail;
            showCurrencyCode: boolean;
            emphasizeTotal: boolean;
            showTaxNote: boolean;
          },
        ) => ReactNode,
      ) => ReactNode);
  /** Document-level commands rendered beside save and print. */
  editorActions?: ReactNode;
  /** Temporarily render the editable draft as a customer-facing preview. */
  presentationReadOnly?: boolean;
  /** Lets a document-specific canvas display anchored editing overlays beyond
   *  the shared shell. Customer-facing previews keep the shell clipped. */
  allowEditorOverflow?: boolean;
  creationTemplates?: CreationTemplate[];
  /** What the record shows about its relations — the credit notes against an
   *  invoice, the invoice an accepted offer produced. */
  footer?: ReactNode;
  onBack: () => void;
  /** Called with the document the server just created, to navigate to it. */
  onCreated: (document: T, creationTemplate?: string) => void;
  /** Discards the draft. Only ever offered while the document is editable. */
  onDiscard: () => Promise<void>;
  /** Fetches this document's printable page from the server. `undefined`
   *  before the document exists — there is nothing to print. */
  onPrint: (() => Promise<string>) | undefined;
}

export function documentEditorClass(allowOverflow: boolean) {
  return cx(styles.editor, allowOverflow && "!overflow-visible");
}

export function DocumentEditor<T extends StoredDocument, A>({
  draft,
  pickers,
  labels,
  chips,
  dates,
  actions,
  lead,
  showSummary = true,
  lineColumns,
  documentBody,
  editorActions,
  presentationReadOnly = false,
  allowEditorOverflow = false,
  creationTemplates,
  footer,
  onBack,
  onCreated,
  onDiscard,
  onPrint,
}: Props<T, A>) {
  const locale = useLocale();
  const { confirm } = useDialogs();
  const [printing, setPrinting] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState("blank");
  const [showProductPicker, setShowProductPicker] = useState(false);
  const [productSearch, setProductSearch] = useState("");
  const { document, header, rows } = draft;
  const readOnly = draft.readOnly || presentationReadOnly;
  // An archived customer is still offered on the document already raised for
  // them — otherwise the picker would silently show no one and the next edit
  // would change who is being billed.
  const pickable = pickers.customers.filter(
    (c) => !c.archived || c.id === header.customerId,
  );
  const customerName =
    pickers.customers.find((c) => c.id === header.customerId)?.name ?? "";

  if (draft.loading) {
    return (
      <div className={styles.page}>
        <div className={styles.loading}>
          <Spinner size={20} />
        </div>
      </div>
    );
  }

  if (draft.missing) {
    return (
      <div className={styles.page}>
        <ErrorBanner message={labels.gone} />
        <p className={styles.noMatches}>
          <button type="button" className={styles.linkAction} onClick={onBack}>
            {labels.back}
          </button>
        </p>
      </div>
    );
  }

  async function create() {
    const created = await draft.create();
    if (created !== null) onCreated(created, selectedTemplate);
  }

  // Printing shows the STORED document, which is what the server renders, so a
  // draft holding unsaved edits would print without them. Rather than refuse,
  // the button waits for the save the editor is already doing — the same rule
  // the lifecycle actions follow, for the same reason.
  async function print() {
    if (onPrint === undefined) return;
    setPrinting(true);
    try {
      printSheet(await onPrint());
    } catch (err) {
      draft.fail(billingMessage(err, strings.billingPrintFailed));
    } finally {
      setPrinting(false);
    }
  }

  async function discard() {
    if (
      !(await confirm({
        title: labels.discardLabel,
        message: labels.discardMessage,
        confirmLabel: labels.discardLabel,
        danger: true,
      }))
    ) {
      return;
    }
    try {
      await onDiscard();
    } catch {
      draft.fail(strings.billingSaveFailed);
    }
  }

  const currency =
    document?.currency ??
    pickers.customers.find((customer) => customer.id === header.customerId)
      ?.currency ??
    "";
  const saved = draft.saveState === "saved";
  const renderPricingTable = ({
    rowKeys,
    title,
    onRowKeysChange,
  }: {
    rowKeys?: string[];
    title?: string;
    onRowKeysChange: (keys: string[]) => void;
  }) => {
    if (document === null) return null;
    const owned = new Set(rowKeys ?? rows.map((row) => row.key));
    const tableRows = rows.filter((row) => owned.has(row.key));
    let savedLineIndex = 0;
    const savedByRowKey = new Map<string, (typeof document.lines)[number]>();
    for (const row of rows) {
      if (isBlankRow(row)) continue;
      const line = document.lines[savedLineIndex];
      savedLineIndex += 1;
      if (line !== undefined) savedByRowKey.set(row.key, line);
    }
    const tableSavedLines = tableRows.flatMap((row) => {
      const line = savedByRowKey.get(row.key);
      return line === undefined ? [] : [line];
    });
    return (
      <DocumentLines
        rows={tableRows}
        products={pickers.products}
        savedLines={tableSavedLines}
        saved={saved}
        currency={currency}
        readOnly={readOnly}
        columns={lineColumns}
        title={
          typeof documentBody === "function"
            ? (title ?? strings.quoteStudioPricingTable)
            : undefined
        }
        onChange={(next) => {
          const merged = rows.filter((row) => !owned.has(row.key)).concat(next);
          draft.edit({ rows: merged });
          onRowKeysChange(next.map((row) => row.key));
        }}
        nextKey={draft.nextKey}
      />
    );
  };
  const totalsPanel =
    document === null ? null : (
      <TotalsPanel
        totals={document.totals}
        currency={currency}
        stale={!saved}
      />
    );
  const renderTableSubtotal = (
    rowKeys?: string[],
    presentation = {
      placement: "summary" as QuoteTotalsPlacement,
      detail: "summary" as QuoteTotalsDetail,
      showCurrencyCode: false,
      emphasizeTotal: true,
      showTaxNote: false,
    },
  ) => {
    if (document === null) return null;
    const owned = new Set(rowKeys ?? rows.map((row) => row.key));
    let lineIndex = 0;
    const ownedLines = rows.flatMap((row) => {
      if (isBlankRow(row)) return [];
      const line = document.lines[lineIndex];
      lineIndex += 1;
      return owned.has(row.key) && line !== undefined ? [line] : [];
    });
    const ownsEveryLine = ownedLines.length === document.lines.length;
    const netCents = ownsEveryLine
      ? document.totals.netCents
      : ownedLines.reduce((sum, line) => sum + line.netCents, 0);
    const netByRate = ownedLines.reduce<Map<number, number>>((rates, line) => {
      rates.set(
        line.vatRateBp,
        (rates.get(line.vatRateBp) ?? 0) + line.netCents,
      );
      return rates;
    }, new Map());
    const vatCents = ownsEveryLine
      ? document.totals.vatCents
      : Array.from(netByRate).reduce(
          (sum, [rate, net]) => sum + Math.round((net * rate) / 10_000),
          0,
        );
    const grossCents = ownsEveryLine
      ? document.totals.grossCents
      : netCents + vatCents;
    const amount = (cents: number) =>
      saved ? formatAmount(cents, locale, currency) : "—";
    const totalAmount = `${amount(grossCents)}${
      presentation.showCurrencyCode && currency ? ` ${currency}` : ""
    }`;
    const rowsToShow: Array<{ label: string; value: string; total?: boolean }> =
      presentation.detail === "total"
        ? [
            {
              label: strings.billingTotalsGross,
              value: totalAmount,
              total: true,
            },
          ]
        : [
            { label: strings.billingTotalsNet, value: amount(netCents) },
            ...(presentation.detail === "breakdown"
              ? Array.from(netByRate)
                  .sort(([left], [right]) => left - right)
                  .map(([rate, net]) => ({
                    label: strings.billingVatAtRate(formatRate(rate, locale)),
                    value: amount(Math.round((net * rate) / 10_000)),
                  }))
              : [{ label: strings.billingVat, value: amount(vatCents) }]),
            {
              label: strings.billingTotalsGross,
              value: totalAmount,
              total: true,
            },
          ];
    return (
      <div
        className={cx(
          "text-sm",
          presentation.placement === "summary" && "mt-4 flex justify-end",
          presentation.placement === "full" && "mt-4",
          presentation.placement === "footer" && "-mt-3",
        )}
      >
        <div
          className={cx(
            "bg-raised/45 px-5 py-4",
            presentation.placement === "summary" &&
              "w-full max-w-md rounded-xl border border-default shadow-sm",
            presentation.placement === "full" &&
              "w-full rounded-xl border border-default",
            presentation.placement === "footer" &&
              "w-full rounded-b-xl border border-t-0 border-default",
          )}
        >
          <dl
            className={cx(
              "grid grid-cols-[1fr_auto] gap-x-8 gap-y-2",
              presentation.placement === "summary" && "ml-auto max-w-sm",
              presentation.placement === "full" && "mx-auto max-w-2xl",
            )}
          >
            {rowsToShow.map((row) => (
              <div className="contents" key={row.label}>
                <dt
                  className={cx(
                    "text-secondary",
                    row.total &&
                      "border-t border-default pt-3 font-semibold text-primary",
                  )}
                >
                  {row.label}
                </dt>
                <dd
                  className={cx(
                    "text-right tabular-nums text-primary",
                    row.total && "border-t border-default pt-3 font-semibold",
                    row.total && presentation.emphasizeTotal && "text-base",
                  )}
                >
                  {row.value}
                </dd>
              </div>
            ))}
          </dl>
          {presentation.showTaxNote && (
            <p className="mt-3 text-xs text-tertiary">
              {strings.billingVatIncludedNote}
            </p>
          )}
        </div>
      </div>
    );
  };
  const error = draft.error ?? pickers.error;
  const selectedProductIds = new Set(
    rows.flatMap((row) => row.productId ?? []),
  );
  const availableProducts = pickers.products.filter(
    (product) =>
      !product.archived &&
      !selectedProductIds.has(product.id) &&
      product.name
        .toLocaleLowerCase()
        .includes(productSearch.trim().toLocaleLowerCase()),
  );

  if (document === null) {
    // The label above a control on the create form. The controls themselves are
    // now `ds/Input` and `ds/Select`: this screen declared a control recipe of
    // its own beside them, which was the same reimplementation `billingStyles`
    // was carrying, and it is gone with it (D2.06b).
    const fieldLabel =
      "mb-2 block text-xs font-semibold uppercase tracking-wide text-tertiary";

    return (
      <DialogFrame
        Icon={FilePlus2}
        title={labels.newTitle}
        subtitle={labels.createHint}
        error={error}
        busy={draft.creating}
        canSubmit={header.customerId !== ""}
        submitLabel={labels.createLabel}
        onClose={onBack}
        onSubmit={() => void create()}
        wide={creationTemplates !== undefined && creationTemplates.length > 0}
      >
        {creationTemplates !== undefined && creationTemplates.length > 0 && (
          <section
            className={styles.templatePicker}
            aria-label={strings.billingQuoteTemplate}
          >
            <div>
              <p className={styles.templatePickerTitle}>
                {strings.billingQuoteStartFrom}
              </p>
              <p className={styles.templatePickerHint}>
                {strings.billingQuoteTemplateHint}
              </p>
            </div>
            <div className={styles.templateGrid}>
              {creationTemplates.map((template) => {
                const active = template.key === selectedTemplate;
                return (
                  <button
                    key={template.key}
                    type="button"
                    className={cx(
                      styles.templateCard,
                      active && styles.templateCardActive,
                    )}
                    aria-pressed={active}
                    onClick={() => {
                      setSelectedTemplate(template.key);
                      draft.edit({ rows: template.buildRows(draft.nextKey) });
                    }}
                  >
                    <span
                      className={styles.templateCardPreview}
                      aria-hidden="true"
                    >
                      <CreationTemplatePreview kind={template.preview} />
                    </span>
                    <span className={styles.templateCardFooter}>
                      <span className={styles.templateCardName}>
                        {template.name}
                      </span>
                      <span
                        className={cx(
                          styles.templateCardCheck,
                          active && styles.templateCardCheckActive,
                        )}
                        aria-hidden="true"
                      >
                        <Check className="size-3.5" strokeWidth={2.5} />
                      </span>
                    </span>
                    <span className={styles.templateCardDescription}>
                      {template.description}
                    </span>
                  </button>
                );
              })}
            </div>
            <div className={styles.templateItems}>
              <div className={styles.templateItemsHead}>
                <div>
                  <p className={styles.templateItemsTitle}>
                    {strings.billingQuoteIncludedTitle}
                  </p>
                  <p className={styles.templateItemsHint}>
                    {strings.billingQuoteIncludedHelp}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span className={styles.templateItemsCount}>
                    {strings.billingQuoteIncludedItems(rows.length)}
                  </span>
                  <button
                    type="button"
                    className={styles.templateAddItems}
                    aria-expanded={showProductPicker}
                    onClick={() => setShowProductPicker((open) => !open)}
                  >
                    <Plus className="size-4" aria-hidden="true" />
                    <span>{strings.billingQuoteAddFromPriceList}</span>
                  </button>
                </div>
              </div>
              {rows.length > 0 && (
                <div className={styles.templateItemsList}>
                  {rows.map((row) => (
                    <div key={row.key} className={styles.templateItem}>
                      <span
                        className={styles.templateItemCheck}
                        aria-hidden="true"
                      >
                        <Check className="size-3.5" strokeWidth={2.5} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-semibold text-primary">
                          {row.description}
                        </span>
                        <span className={styles.templateItemMeta}>
                          {formatAmount(
                            parseHundredths(row.price) ?? 0,
                            locale,
                            currency,
                          )}
                          <span aria-hidden="true">/</span>
                          {row.unit || strings.billingQuotePerItem}
                        </span>
                      </span>
                      <button
                        type="button"
                        className={styles.templateItemRemove}
                        aria-label={strings.billingQuoteRemoveIncludedItem(
                          row.description,
                        )}
                        onClick={() => {
                          setSelectedTemplate("custom");
                          draft.edit({
                            rows: rows.filter((item) => item.key !== row.key),
                          });
                        }}
                      >
                        <X className="size-4" aria-hidden="true" />
                      </button>
                    </div>
                  ))}
                </div>
              )}
              {showProductPicker && (
                <div className={styles.templateProductPicker}>
                  <label className={styles.templateProductSearch}>
                    <Search className="size-4" aria-hidden="true" />
                    <Input
                      value={productSearch}
                      onChange={(event) => setProductSearch(event.target.value)}
                      placeholder={strings.billingQuoteSearchPriceList}
                      aria-label={strings.billingQuoteSearchPriceList}
                    />
                  </label>
                  {availableProducts.length === 0 ? (
                    <p className={styles.templateProductEmpty}>
                      {productSearch.trim() === ""
                        ? strings.billingQuoteAllItemsIncluded
                        : strings.billingQuoteNoMatchingItems}
                    </p>
                  ) : (
                    <div className={styles.templateProductList}>
                      {availableProducts.map((product) => (
                        <button
                          key={product.id}
                          type="button"
                          className={styles.templateProductOption}
                          onClick={() => {
                            setSelectedTemplate("custom");
                            draft.edit({
                              rows: [
                                ...rows,
                                rowFromProduct(
                                  { ...blankRow(draft.nextKey()), qty: "1" },
                                  product,
                                ),
                              ],
                            });
                          }}
                        >
                          <span className="min-w-0 flex-1 truncate font-medium text-primary">
                            {product.name}
                          </span>
                          <span className="shrink-0 text-xs text-tertiary">
                            {formatAmount(
                              product.unitPriceCents,
                              locale,
                              currency,
                            )}
                            <span className="mx-1" aria-hidden="true">
                              /
                            </span>
                            {product.unit || strings.billingQuotePerItem}
                          </span>
                          <Plus
                            className="size-4 shrink-0 text-accent"
                            aria-hidden="true"
                          />
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </div>
          </section>
        )}
        <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1">
          <label className="min-w-0">
            <span className={fieldLabel}>{strings.billingFieldCustomer}</span>
            <ChoicePicker
              value={header.customerId}
              options={pickable.map((customer) => ({
                value: customer.id,
                label: customer.name,
              }))}
              placeholder={strings.billingChooseCustomer}
              label={strings.billingFieldCustomer}
              onChange={(customerId) =>
                draft.edit({ header: { ...header, customerId } })
              }
            />
            <span className="mt-2 block text-xs leading-relaxed text-tertiary">
              {labels.customerHint}
            </span>
          </label>

          <label className="min-w-0">
            <span className={fieldLabel}>{strings.billingFieldReference}</span>
            <Input
              value={header.reference}
              onChange={(event) =>
                draft.edit({
                  header: { ...header, reference: event.target.value },
                })
              }
              placeholder={strings.billingReferencePlaceholder}
            />
            <span className="mt-2 block text-xs leading-relaxed text-tertiary">
              {strings.billingReferenceHint}
            </span>
          </label>

          <label className="col-span-2 min-w-0 max-md:col-span-1">
            <span className={fieldLabel}>{strings.billingFieldNote}</span>
            <textarea
              className={cx(styles.textarea, "min-h-28 py-3 leading-relaxed")}
              value={header.note}
              rows={3}
              onChange={(event) =>
                draft.edit({ header: { ...header, note: event.target.value } })
              }
              placeholder={strings.billingNotePlaceholder}
            />
            <span className="mt-2 block text-xs leading-relaxed text-tertiary">
              {strings.billingNoteHint}
            </span>
          </label>
        </div>
      </DialogFrame>
    );
  }

  return (
    <div className={styles.page}>
      <article className={documentEditorClass(allowEditorOverflow)}>
        <div className={styles.editorHead}>
          <button type="button" className={styles.linkAction} onClick={onBack}>
            <ArrowLeft size={14} aria-hidden="true" /> {labels.back}
          </button>
          <h2 className={styles.editorTitle}>
            {document === null
              ? labels.newTitle
              : (document.number ?? labels.draftTitle)}
          </h2>
          <span className={styles.chips}>{chips}</span>
          <span className={styles.saveState} role="status">
            {document === null
              ? ""
              : draft.saveState === "saving"
                ? strings.billingSaving
                : draft.saveState === "pending"
                  ? strings.billingUnsaved
                  : draft.saveState === "failed"
                    ? strings.billingSaveNotDone
                    : strings.billingSaved}
          </span>
          {editorActions}
          {draft.saveState === "failed" && (
            <button
              type="button"
              className={styles.linkAction}
              onClick={draft.saveNow}
            >
              {strings.billingSaveNow}
            </button>
          )}
          {document !== null && (
            <button
              type="button"
              className={styles.linkAction}
              onClick={() => void print()}
              disabled={onPrint === undefined || printing || !saved}
              title={
                onPrint === undefined || !saved
                  ? strings.billingPrintUnsaved
                  : undefined
              }
            >
              <Printer size={14} aria-hidden="true" /> {strings.billingPrint}
            </button>
          )}
          {document !== null && !readOnly && (
            <button
              type="button"
              className={styles.linkAction}
              onClick={() => void discard()}
            >
              {labels.discardLabel}
            </button>
          )}
        </div>

        {error !== null && <ErrorBanner message={error} />}
        {draft.readOnly && labels.frozenNotice !== null && (
          <p className={styles.notice}>{labels.frozenNotice}</p>
        )}

        <div className={styles.editorBody}>
          {lead}
          {showSummary && (
            <section className={styles.documentSummary}>
              <div className={styles.headerFields}>
                <Field
                  label={strings.billingFieldCustomer}
                  hint={labels.customerHint}
                >
                  {readOnly ? (
                    <p className={styles.readOnlyValue}>{customerName}</p>
                  ) : (
                    <ChoicePicker
                      value={header.customerId}
                      options={pickable.map((customer) => ({
                        value: customer.id,
                        label: customer.name,
                      }))}
                      placeholder={strings.billingChooseCustomer}
                      label={strings.billingFieldCustomer}
                      onChange={(customerId) =>
                        draft.edit({
                          header: { ...header, customerId },
                        })
                      }
                    />
                  )}
                </Field>

                <Field
                  label={strings.billingFieldReference}
                  hint={strings.billingReferenceHint}
                >
                  {readOnly ? (
                    <p className={styles.readOnlyValue}>{header.reference}</p>
                  ) : (
                    <Input
                      value={header.reference}
                      onChange={(e) =>
                        draft.edit({
                          header: { ...header, reference: e.target.value },
                        })
                      }
                      placeholder={strings.billingReferencePlaceholder}
                    />
                  )}
                </Field>

                {document !== null && dates}
              </div>

              <div className={styles.documentNote}>
                <Field
                  label={strings.billingFieldNote}
                  hint={strings.billingNoteHint}
                >
                  {readOnly ? (
                    <p className={styles.readOnlyValue}>{header.note}</p>
                  ) : (
                    <textarea
                      className={styles.textarea}
                      value={header.note}
                      rows={2}
                      onChange={(e) =>
                        draft.edit({
                          header: { ...header, note: e.target.value },
                        })
                      }
                      placeholder={strings.billingNotePlaceholder}
                    />
                  )}
                </Field>
              </div>
            </section>
          )}

          {typeof documentBody === "function"
            ? documentBody(
                renderPricingTable,
                totalsPanel,
                rows.map((row) => row.key),
                renderTableSubtotal,
              )
            : documentBody}

          {document === null ? (
            <div className={styles.createBar}>
              <p className={styles.hint}>{labels.createHint}</p>
              <Button
                onClick={() => void create()}
                disabled={draft.creating || header.customerId === ""}
              >
                {labels.createLabel}
              </Button>
            </div>
          ) : (
            <>
              {typeof documentBody === "function"
                ? null
                : renderPricingTable({
                    onRowKeysChange: () => undefined,
                  })}
              {typeof documentBody === "function" ? null : totalsPanel}
              <DocumentActions
                actions={actions}
                unsaved={!saved}
                onFailed={draft.fail}
              />
              {footer !== undefined && (
                <footer className={styles.documentFooter}>{footer}</footer>
              )}
            </>
          )}
        </div>
      </article>
    </div>
  );
}
