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
import { Crop, GripVertical, PackageOpen, Pencil, Plus, Trash2, X } from "lucide-react";

import { Button, ChoicePicker, IconButton, Input, Modal, Table, Td, Th, cx } from "../ds";
import { strings, useLocale } from "../i18n";
import { formatAmount, formatQty, formatRate } from "./money";
import { blankRow, isBlankRow, rowFromProduct, rowProblem } from "./lineRows";
import type { LineRow, RowProblem } from "./lineRows";
import type { BillingProduct, DocumentLine } from "./types";
import type { QuoteColumns } from "./QuoteContentStudio";
import type { QuoteLineContent } from "./quoteTableOptions";
import { useQuoteTableOptions } from "./quoteTableOptions";
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

type ImageDraft = Required<Pick<
  QuoteLineContent,
  "image" | "imageFit" | "imagePosition" | "imageZoom"
>> & { key: string };

const IMAGE_SIZE = {
  small: "size-16",
  medium: "size-24",
  large: "size-32",
} as const;
const IMAGE_POSITION = {
  center: "object-center",
  top: "object-top",
  bottom: "object-bottom",
  left: "object-left",
  right: "object-right",
} as const;
const IMAGE_ZOOM = {
  50: "scale-50",
  60: "scale-[.6]",
  70: "scale-[.7]",
  75: "scale-75",
  80: "scale-[.8]",
  90: "scale-90",
  100: "scale-100",
  110: "scale-110",
  120: "scale-[1.2]",
  125: "scale-125",
  130: "scale-[1.3]",
  140: "scale-[1.4]",
  150: "scale-150",
  160: "scale-[1.6]",
  170: "scale-[1.7]",
  175: "scale-[1.75]",
  180: "scale-[1.8]",
  190: "scale-[1.9]",
  200: "scale-200",
} as const;

function normalizeZoom(value: number): keyof typeof IMAGE_ZOOM {
  if (!Number.isFinite(value)) return 100;
  const supported = Object.keys(IMAGE_ZOOM).map(Number);
  return supported.reduce((closest, candidate) =>
    Math.abs(candidate - value) < Math.abs(closest - value)
      ? candidate
      : closest,
  ) as keyof typeof IMAGE_ZOOM;
}

function imageDraft(key: string, content: QuoteLineContent): ImageDraft {
  return {
    key,
    image: content.image,
    imageFit: content.imageFit ?? "cover",
    imagePosition: content.imagePosition ?? "center",
    imageZoom: content.imageZoom ?? 100,
  };
}

function imageClasses(
  content: Pick<
    QuoteLineContent,
    "imageFit" | "imagePosition" | "imageZoom"
  >,
): string {
  return cx(
    "size-full transition-transform",
    content.imageFit === "contain" ? "object-contain" : "object-cover",
    IMAGE_POSITION[content.imagePosition ?? "center"],
    IMAGE_ZOOM[normalizeZoom(content.imageZoom ?? 100)],
  );
}

function ProductImageEditor({
  draft,
  onChange,
  onApply,
  onClose,
}: {
  draft: ImageDraft;
  onChange: (draft: ImageDraft) => void;
  onApply: () => void;
  onClose: () => void;
}) {
  return (
    <Modal
      title="Edit product image"
      icon={<Crop className="size-5" />}
      onClose={onClose}
      wide
      actions={<IconButton label="Close image editor" icon={<X />} onClick={onClose} />}
      footer={
        <div className="ml-auto flex items-center gap-3">
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button onClick={onApply}>Apply image</Button>
        </div>
      }
    >
      <div className="grid min-h-0 gap-5 lg:grid-cols-[minmax(0,1fr)_15rem]">
        <section className="rounded-xl border border-default bg-app p-5" aria-label="PDF preview">
          <div className="mx-auto max-w-xl rounded-lg border border-default bg-surface p-8 shadow-sm">
            <div className="mb-5 flex items-center justify-between border-b border-subtle pb-4">
              <div>
                <span className="text-xs font-semibold uppercase tracking-wide text-accent">Quotation preview</span>
                <p className="mt-1 text-sm text-secondary">This is the image size and crop used in the PDF.</p>
              </div>
              <span className="text-xs font-medium text-tertiary">A4</span>
            </div>
            <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-4 rounded-lg border border-subtle p-4">
              <div className="size-24 overflow-hidden rounded-lg border border-default bg-raised/30">
                <img src={draft.image} alt="Product PDF preview" className={imageClasses(draft)} />
              </div>
              <div className="min-w-0">
                <div className="h-3 w-3/4 rounded-full bg-default" />
                <div className="mt-3 h-2 w-full rounded-full bg-raised" />
                <div className="mt-2 h-2 w-2/3 rounded-full bg-raised" />
              </div>
              <div className="h-3 w-20 rounded-full bg-accent-soft" />
            </div>
          </div>
        </section>

        <aside className="flex flex-col gap-5">
          <EditorChoice
            label="Crop style"
            value={draft.imageFit}
            choices={[["cover", "Fill frame"], ["contain", "Show full image"]]}
            onChange={(imageFit) => onChange({ ...draft, imageFit })}
          />
          <fieldset>
            <legend className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">
              Zoom
            </legend>
            <div className="grid grid-cols-3 gap-2">
              {[75, 100, 125, 150, 200].map((zoom) => (
                <button
                  key={zoom}
                  type="button"
                  className={cx(
                    "min-h-10 rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
                    draft.imageZoom === zoom
                      ? "border-accent bg-accent-soft text-accent"
                      : "border-default bg-surface text-primary hover:border-accent/50 hover:bg-raised",
                  )}
                  aria-pressed={draft.imageZoom === zoom}
                  onClick={() => onChange({ ...draft, imageZoom: zoom })}
                >
                  {zoom}%
                </button>
              ))}
              <label className="relative">
                <span className="sr-only">Custom zoom percentage</span>
                <input
                  aria-label="Custom zoom percentage"
                  type="number"
                  min="50"
                  max="200"
                  step="10"
                  value={draft.imageZoom}
                  className="min-h-10 w-full rounded-lg border border-default bg-surface px-3 pr-7 text-sm font-medium text-primary focus:border-accent focus:outline-none"
                  onChange={(event) =>
                    onChange({
                      ...draft,
                      imageZoom: event.currentTarget.valueAsNumber,
                    })
                  }
                  onBlur={(event) =>
                    onChange({
                      ...draft,
                      imageZoom: normalizeZoom(event.currentTarget.valueAsNumber),
                    })
                  }
                />
                <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-xs text-tertiary">
                  %
                </span>
              </label>
            </div>
            <p className="mt-2 text-xs leading-relaxed text-secondary">
              Use 50–90% to show more of the image, or more than 100% for a tighter crop.
            </p>
          </fieldset>
          <EditorChoice
            label="Focus area"
            value={draft.imagePosition}
            choices={[["center", "Centre"], ["top", "Top"], ["bottom", "Bottom"], ["left", "Left"], ["right", "Right"]]}
            onChange={(imagePosition) => onChange({ ...draft, imagePosition })}
          />
        </aside>
      </div>
    </Modal>
  );
}

function EditorChoice<T extends string>({
  label,
  value,
  choices,
  onChange,
}: {
  label: string;
  value: T;
  choices: ReadonlyArray<readonly [T, string]>;
  onChange: (value: T) => void;
}) {
  return (
    <fieldset>
      <legend className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</legend>
      <div className="grid grid-cols-2 gap-2">
        {choices.map(([id, name]) => (
          <button
            key={id}
            type="button"
            className={cx(
              "min-h-10 rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
              value === id
                ? "border-accent bg-accent-soft text-accent"
                : "border-default bg-surface text-primary hover:border-accent/50 hover:bg-raised",
            )}
            aria-pressed={value === id}
            onClick={() => onChange(id)}
          >
            {name}
          </button>
        ))}
      </div>
    </fieldset>
  );
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
  const tableOptions = useQuoteTableOptions();
  const [draggedKey, setDraggedKey] = useState<string | null>(null);
  const [dropTargetKey, setDropTargetKey] = useState<string | null>(null);
  const [editingImage, setEditingImage] = useState<ImageDraft | null>(null);

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

  function contentKey(row: LineRow) {
    if (row.productId) return `product:${row.productId}`;
    const description = row.description.trim().toLocaleLowerCase();
    return description ? `description:${description}` : row.key;
  }

  function uploadImage(file: File, key: string) {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string")
        tableOptions.updateLineContent(key, { image: reader.result });
    };
    reader.readAsDataURL(file);
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
              const detailKey = contentKey(row);
              const content = tableOptions.lineContent[detailKey] ?? {
                description: "",
                image: "",
              };
              return (
                <article
                  key={row.key}
                  className={cx(
                    "relative rounded-xl border border-default bg-surface p-4 pl-14 shadow-sm transition-colors",
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
                  {tableOptions.enabled && tableOptions.showImages && (
                    <div className="mb-4 flex items-center gap-3 rounded-xl bg-raised/35 p-3">
                      <div
                        className={cx(
                          "group/image relative grid shrink-0 place-items-center overflow-hidden rounded-lg border border-default bg-surface",
                          IMAGE_SIZE[
                            tableOptions.layout === "catalogue"
                              ? "large"
                              : "medium"
                          ],
                        )}
                        onDoubleClick={() => {
                          if (content.image) setEditingImage(imageDraft(detailKey, content));
                        }}
                      >
                        {content.image ? (
                          <>
                            <img src={content.image} alt="Product" className={imageClasses(content)} />
                            <button
                              type="button"
                              className="absolute right-2 top-2 grid size-9 place-items-center rounded-lg border border-default bg-surface text-primary shadow-sm transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
                              aria-label="Edit product image"
                              title="Edit product image"
                              onClick={() => setEditingImage(imageDraft(detailKey, content))}
                            >
                              <Pencil className="size-4" aria-hidden="true" />
                            </button>
                          </>
                        ) : (
                          <PackageOpen
                            className="size-5 text-tertiary"
                            aria-hidden="true"
                          />
                        )}
                      </div>
                      <div>
                        <strong className="block text-sm font-semibold text-primary">
                          Product image
                        </strong>
                        <p className="mt-1 text-xs text-secondary">
                          Shown beside this item in the customer quotation.
                        </p>
                        <div className="mt-2 flex flex-wrap gap-2">
                          <label className="inline-flex min-h-9 cursor-pointer items-center rounded-lg border border-default bg-surface px-3 text-xs font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent">
                            {content.image ? "Replace image" : "Upload image"}
                            <input
                              type="file"
                              accept="image/png,image/jpeg,image/webp"
                              className="sr-only"
                              onChange={(event) => {
                                const file = event.target.files?.[0];
                                if (file) uploadImage(file, detailKey);
                                event.currentTarget.value = "";
                              }}
                            />
                          </label>
                          {content.image && (
                            <button
                              type="button"
                              className="min-h-9 rounded-lg px-3 text-xs font-semibold text-secondary hover:bg-danger-tint hover:text-danger"
                              onClick={() =>
                                tableOptions.updateLineContent(detailKey, {
                                  image: "",
                                })
                              }
                            >
                              Remove
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  )}
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
                  {tableOptions.enabled && tableOptions.showDescriptions && (
                    <label className="mt-4 block">
                      <span className="text-xs font-semibold uppercase tracking-wide text-tertiary">
                        Product description
                      </span>
                      <textarea
                        className="mt-2 min-h-20 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm leading-relaxed text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
                        value={content.description}
                        placeholder="Add specifications, materials, scope, or other useful details…"
                        onChange={(event) =>
                          tableOptions.updateLineContent(detailKey, {
                            description: event.target.value,
                          })
                        }
                      />
                    </label>
                  )}
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
        {editingImage !== null && (
          <ProductImageEditor
            draft={editingImage}
            onChange={setEditingImage}
            onClose={() => setEditingImage(null)}
            onApply={() => {
              const { key, ...image } = editingImage;
              tableOptions.updateLineContent(key, image);
              setEditingImage(null);
            }}
          />
        )}
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
              {tableOptions.enabled && tableOptions.showImages && (
                <Th>Image</Th>
              )}
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
              const content = tableOptions.lineContent[contentKey(row)] ?? {
                description: "",
                image: "",
              };
              return (
                <tr key={row.key}>
                  {tableOptions.enabled && tableOptions.showImages && (
                    <td className="w-28">
                      {content.image ? (
                        <span
                          className={cx(
                            "block overflow-hidden rounded-lg border border-default bg-raised/30",
                            IMAGE_SIZE[
                              tableOptions.layout === "catalogue"
                                ? "medium"
                                : "small"
                            ],
                          )}
                        >
                          <img
                            src={content.image}
                            alt=""
                            className={imageClasses(content)}
                          />
                        </span>
                      ) : (
                        <span className="grid size-16 place-items-center rounded-lg bg-raised/40">
                          <PackageOpen className="size-5 text-tertiary" />
                        </span>
                      )}
                    </td>
                  )}
                  <td className="min-w-[260px]">
                    <div className="flex flex-col gap-1">
                    {readOnly ? (
                      <div>
                        <strong className="font-medium text-primary">
                          {row.description}
                        </strong>
                        {tableOptions.enabled &&
                          tableOptions.showDescriptions &&
                          content.description && (
                            <p className="mt-1 max-w-2xl whitespace-pre-wrap text-sm leading-relaxed text-secondary">
                              {content.description}
                            </p>
                          )}
                      </div>
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
                    </div>
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
