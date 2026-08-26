import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import {
  AlignLeft,
  ArrowDown,
  ArrowUp,
  Bold,
  Check,
  Copy,
  Heading2,
  ImagePlus,
  Italic,
  List,
  ListOrdered,
  Minus,
  Palette,
  Pilcrow,
  Pencil,
  Plus,
  Quote,
  Rows3,
  Table2,
  Trash2,
  Upload,
  X,
} from "lucide-react";

import { Button, Input, Modal, cx } from "../ds";
import {
  QuoteTableOptionsProvider,
  type QuoteLineContent,
  type QuoteTableLayout,
  type QuoteTotalsDetail,
  type QuoteTotalsPlacement,
} from "./quoteTableOptions";

type Theme = "modern" | "editorial" | "minimal";
type Block =
  | { id: string; kind: "text"; heading: string; body: string }
  | { id: string; kind: "heading"; level: 1 | 2 | 3; text: string }
  | { id: string; kind: "paragraph"; text: string }
  | { id: string; kind: "quote"; text: string; attribution: string }
  | { id: string; kind: "list"; ordered: boolean; items: string }
  | { id: string; kind: "divider" }
  | {
      id: string;
      kind: "image";
      src: string;
      caption: string;
      body?: string;
      placement?: "full" | "left" | "right";
      aspect?: "natural" | "landscape" | "square";
      fit?: "cover" | "contain";
      zoom?: 50 | 75 | 100 | 125 | 150 | 175 | 200;
    }
  | {
      id: string;
      kind: "pricing";
      rowKeys?: string[];
      showSubtotal?: boolean;
      title?: string;
    }
  | {
      id: string;
      kind: "table";
      columns: Array<{ id: string; label: string }>;
      rows: Array<{ id: string; cells: Record<string, string> }>;
    };
interface Colors {
  accent: string;
  background: string;
  text: string;
  tableHeader: string;
  tableRows: string;
}
export interface QuoteColumns {
  unit: boolean;
  quantity: boolean;
  unitPrice: boolean;
  vat: boolean;
  net: boolean;
}
export const DEFAULT_QUOTE_COLUMNS: QuoteColumns = {
  unit: true,
  quantity: true,
  unitPrice: true,
  vat: true,
  net: true,
};
interface Design {
  logo: string;
  theme: Theme;
  colors: Colors;
  columns: QuoteColumns;
  tableLayout: QuoteTableLayout;
  showProductImages: boolean;
  showProductDescriptions: boolean;
  lineContent: Record<string, QuoteLineContent>;
  totalsPlacement: QuoteTotalsPlacement;
  totalsDetail: QuoteTotalsDetail;
  showCurrencyCode: boolean;
  emphasizeTotal: boolean;
  showTaxNote: boolean;
  blocks: Block[];
}
const DEFAULT_COLORS: Colors = {
  accent: "#e76f51",
  background: "#fffefc",
  text: "#102a43",
  tableHeader: "#f3f0ea",
  tableRows: "#fffefc",
};
const EMPTY: Design = {
  logo: "",
  theme: "modern",
  colors: DEFAULT_COLORS,
  columns: DEFAULT_QUOTE_COLUMNS,
  tableLayout: "compact",
  showProductImages: false,
  showProductDescriptions: false,
  lineContent: {},
  totalsPlacement: "summary",
  totalsDetail: "summary",
  showCurrencyCode: false,
  emphasizeTotal: true,
  showTaxNote: false,
  blocks: [{ id: "pricing-table", kind: "pricing" }],
};
const DESIGN_STORE = "quote-designs";
const DESIGN_DATABASE = "alo-quote-assets";
const themeChoices: Array<{ id: Theme; name: string; help: string }> = [
  { id: "modern", name: "Modern", help: "Clean and confident" },
  { id: "editorial", name: "Editorial", help: "Story-led headings" },
  { id: "minimal", name: "Minimal", help: "Quiet and precise" },
];

function legacyDesign(key: string): Design | null {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return null;
    const saved = JSON.parse(raw) as Partial<Design>;
    return normalizeDesign({
      ...EMPTY,
      ...saved,
      colors: { ...DEFAULT_COLORS, ...saved.colors },
    });
  } catch {
    return null;
  }
}

function normalizeDesign(design: Design): Design {
  return design.blocks.some((block) => block.kind === "pricing")
    ? design
    : {
        ...design,
        blocks: [...design.blocks, { id: "pricing-table", kind: "pricing" }],
      };
}

function designDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DESIGN_DATABASE, 1);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(DESIGN_STORE))
        request.result.createObjectStore(DESIGN_STORE);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(
        request.error ??
          new Error("The quotation design database could not be opened."),
      );
  });
}

async function loadDesign(key: string): Promise<Design> {
  try {
    const database = await designDatabase();
    const saved = await new Promise<Partial<Design> | undefined>(
      (resolve, reject) => {
        const request = database
          .transaction(DESIGN_STORE, "readonly")
          .objectStore(DESIGN_STORE)
          .get(key);
        request.onsuccess = () =>
          resolve(request.result as Partial<Design> | undefined);
        request.onerror = () => reject(request.error);
      },
    );
    database.close();
    if (saved !== undefined)
      return normalizeDesign({
        ...EMPTY,
        ...saved,
        colors: { ...DEFAULT_COLORS, ...saved.colors },
      });
  } catch {
    /* Fall through to the small legacy record when IndexedDB is unavailable. */
  }
  return legacyDesign(key) ?? EMPTY;
}

async function saveDesign(key: string, design: Design): Promise<void> {
  const database = await designDatabase();
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction(DESIGN_STORE, "readwrite");
    transaction.objectStore(DESIGN_STORE).put(design, key);
    transaction.oncomplete = () => resolve();
    transaction.onerror = () =>
      reject(
        transaction.error ??
          new Error("The quotation design could not be saved."),
      );
    transaction.onabort = () =>
      reject(
        transaction.error ??
          new Error("The quotation design save was cancelled."),
      );
  });
  database.close();
  localStorage.removeItem(key);
}
function imageData(file: File, done: (value: string) => void) {
  const reader = new FileReader();
  reader.onload = () =>
    typeof reader.result === "string" && done(reader.result);
  reader.readAsDataURL(file);
}

export interface QuoteContentStudioHandle {
  customize: () => void;
  copyTo: (quoteId: string) => Promise<void>;
}

export const QuoteContentStudio = forwardRef<
  QuoteContentStudioHandle,
  {
    quoteId: string;
    readOnly: boolean;
    preview?: boolean;
    pricingTable: (options: {
      rowKeys?: string[];
      title?: string;
      onRowKeysChange: (keys: string[]) => void;
    }) => ReactNode;
    totals: ReactNode;
    tableSubtotal: (rowKeys?: string[]) => ReactNode;
    lineKeys: string[];
    onColumnsChange?: (columns: QuoteColumns) => void;
  }
>(function QuoteContentStudio(
  {
    quoteId,
    readOnly,
    preview = false,
    pricingTable,
    totals,
    tableSubtotal,
    lineKeys,
    onColumnsChange,
  },
  ref,
) {
  const storageKey = `alo:quote-design:${quoteId}`;
  const [design, setDesign] = useState<Design>(EMPTY);
  const [ready, setReady] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [customize, setCustomize] = useState(false);
  const [tableSettings, setTableSettings] = useState(false);
  const [editingImageId, setEditingImageId] = useState<string | null>(null);
  const root = useRef<HTMLElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const replaceImageInput = useRef<HTMLInputElement>(null);
  const pendingImageIndex = useRef<number | null>(null);
  useImperativeHandle(
    ref,
    () => ({
      customize: () => setCustomize(true),
      copyTo: (nextQuoteId) =>
        saveDesign(`alo:quote-design:${nextQuoteId}`, design),
    }),
    [design],
  );

  useEffect(() => {
    let current = true;
    setReady(false);
    void loadDesign(storageKey).then((saved) => {
      if (!current) return;
      setDesign(saved);
      setReady(true);
    });
    return () => {
      current = false;
    };
  }, [storageKey]);
  useEffect(() => {
    if (!ready) return;
    let current = true;
    const timeout = window.setTimeout(() => {
      void saveDesign(storageKey, design)
        .then(() => {
          if (current) setSaveError("");
        })
        .catch(() => {
          if (current)
            setSaveError(
              "This design could not be saved. Try a smaller image or upload it again.",
            );
        });
    }, 200);
    return () => {
      current = false;
      window.clearTimeout(timeout);
    };
  }, [design, ready, storageKey]);
  useEffect(() => {
    const document = root.current?.closest("article");
    if (!(document instanceof HTMLElement)) return;
    const values = {
      "--quote-accent": design.colors.accent,
      "--quote-background": design.colors.background,
      "--quote-text": design.colors.text,
      "--quote-table-header": design.colors.tableHeader,
      "--quote-table-row": design.colors.tableRows,
    };
    Object.entries(values).forEach(([name, value]) =>
      document.style.setProperty(name, value),
    );
  }, [design.colors]);
  useEffect(
    () => onColumnsChange?.(design.columns),
    [design.columns, onColumnsChange],
  );

  const insertBlock = (index: number, block: Block) =>
    setDesign((current) => ({
      ...current,
      blocks: [
        ...current.blocks.slice(0, index),
        block,
        ...current.blocks.slice(index),
      ],
    }));
  const addSimpleBlock = (
    index: number,
    kind:
      | "heading"
      | "paragraph"
      | "quote"
      | "list"
      | "divider"
      | "pricing"
      | "table",
    ordered = false,
  ) => {
    const id = crypto.randomUUID();
    if (kind === "heading")
      insertBlock(index, { id, kind, level: 2, text: "" });
    if (kind === "paragraph") insertBlock(index, { id, kind, text: "" });
    if (kind === "quote")
      insertBlock(index, { id, kind, text: "", attribution: "" });
    if (kind === "list") insertBlock(index, { id, kind, ordered, items: "" });
    if (kind === "divider") insertBlock(index, { id, kind });
    if (kind === "pricing") {
      setDesign((current) => ({
        ...current,
        blocks: [
          ...current.blocks.slice(0, index).map((block) =>
            block.kind === "pricing" && block.rowKeys === undefined
              ? { ...block, rowKeys: lineKeys }
              : block,
          ),
          {
            id,
            kind,
            rowKeys: [],
            showSubtotal: true,
            title: `Pricing table ${
              current.blocks.filter((block) => block.kind === "pricing").length + 1
            }`,
          },
          ...current.blocks.slice(index).map((block) =>
            block.kind === "pricing" && block.rowKeys === undefined
              ? { ...block, rowKeys: lineKeys }
              : block,
          ),
        ],
      }));
    }
    if (kind === "table")
      insertBlock(index, {
        id,
        kind,
        columns: [
          { id: crypto.randomUUID(), label: "Column 1" },
          { id: crypto.randomUUID(), label: "Column 2" },
          { id: crypto.randomUUID(), label: "Column 3" },
        ],
        rows: [
          {
            id: crypto.randomUUID(),
            cells: {},
          },
        ],
      });
  };
  const chooseImage = (index: number) => {
    pendingImageIndex.current = index;
    imageInput.current?.click();
  };
  const update = (id: string, patch: Partial<Block>) =>
    setDesign((current) => ({
      ...current,
      blocks: current.blocks.map((block) =>
        block.id === id ? ({ ...block, ...patch } as Block) : block,
      ),
    }));
  const updateLineContent = (key: string, patch: Partial<QuoteLineContent>) =>
    setDesign((current) => ({
      ...current,
      lineContent: {
        ...current.lineContent,
        [key]: {
          description: current.lineContent[key]?.description ?? "",
          image: current.lineContent[key]?.image ?? "",
          ...patch,
        },
      },
    }));
  const removeBlock = (id: string) =>
    setDesign((current) => {
      const removed = current.blocks.find((block) => block.id === id);
      if (removed?.kind !== "pricing")
        return {
          ...current,
          blocks: current.blocks.filter((block) => block.id !== id),
        };

      const remainingPricing = current.blocks.find(
        (block) => block.kind === "pricing" && block.id !== id,
      );
      if (remainingPricing?.kind !== "pricing") return current;
      const reassignedKeys = removed.rowKeys ?? [];
      return {
        ...current,
        blocks: current.blocks
          .filter((block) => block.id !== id)
          .map((block) =>
            block.kind === "pricing" && block.id === remainingPricing.id
              ? {
                  ...block,
                  rowKeys: Array.from(
                    new Set([...(block.rowKeys ?? []), ...reassignedKeys]),
                  ),
                }
              : block,
          ),
      };
    });
  const duplicateBlock = (index: number) =>
    setDesign((current) => {
      const source = current.blocks[index];
      if (source === undefined) return current;
      const copy = { ...source, id: crypto.randomUUID() };
      return {
        ...current,
        blocks: [
          ...current.blocks.slice(0, index + 1),
          copy,
          ...current.blocks.slice(index + 1),
        ],
      };
    });
  const moveBlock = (index: number, direction: -1 | 1) =>
    setDesign((current) => {
      const destination = index + direction;
      if (destination < 0 || destination >= current.blocks.length)
        return current;
      const blocks = [...current.blocks];
      const [block] = blocks.splice(index, 1);
      if (block === undefined) return current;
      blocks.splice(destination, 0, block);
      return { ...current, blocks };
    });

  return (
    <>
      <section
        ref={root}
        className="overflow-hidden rounded-2xl border border-default bg-surface shadow-sm"
      >
        {!preview && (
          <header className="flex flex-wrap items-center justify-between gap-4 border-b border-subtle px-6 py-4 max-md:px-4">
            <div>
              <h2 className="text-base font-semibold text-primary">
                Build your quotation
              </h2>
              <p className="mt-0.5 text-sm text-secondary">
                Add content directly. Changes save automatically.
              </p>
            </div>
          </header>
        )}
        <div
          className={cx(
            "p-6 max-md:p-4",
            design.theme === "editorial" &&
              "[&_h3]:font-editorial [&_h3]:text-2xl",
            design.theme === "minimal" && "[&_article]:shadow-none",
          )}
        >
          {design.logo && (
            <div className="mb-6 flex items-center justify-between border-b border-[var(--quote-table-header)] pb-5">
              <img
                src={design.logo}
                alt="Company logo"
                className="max-h-16 max-w-56 object-contain"
              />
              <span className="h-1 w-20 rounded-full bg-[var(--quote-accent)]" />
            </div>
          )}
          {design.blocks.length === 0 ? (
            <EmptyBuilder readOnly={readOnly} />
          ) : (
            <div className="flex flex-col gap-3">
              {design.blocks.map((block, index) => (
                <div key={block.id}>
                  <article className="overflow-hidden rounded-xl border border-[var(--quote-table-header)] bg-[var(--quote-background)] text-[var(--quote-text)] shadow-sm">
                    {!readOnly && (
                      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--quote-table-header)] bg-raised/40 px-4 py-2.5">
                        {block.kind === "pricing" ? (
                          <input
                            className="min-h-9 max-w-64 rounded-lg border border-default bg-surface px-3 text-sm font-semibold text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
                            value={block.title ?? "Pricing table"}
                            aria-label="Pricing table name"
                            onChange={(event) =>
                              update(block.id, { title: event.target.value })
                            }
                          />
                        ) : (
                          <span className="text-xs font-semibold uppercase tracking-wide text-secondary">
                            {blockName(block)}
                          </span>
                        )}
                        <div className="flex flex-wrap items-center gap-1">
                          {block.kind === "pricing" && (
                            <>
                              {design.blocks.filter(
                                (item) => item.kind === "pricing",
                              ).length > 1 && (
                                <BlockCommand
                                  label={
                                    block.showSubtotal === false
                                      ? "Show subtotal"
                                      : "Hide subtotal"
                                  }
                                  onClick={() =>
                                    update(block.id, {
                                      showSubtotal:
                                        block.showSubtotal === false,
                                    })
                                  }
                                >
                                  <Rows3 className="size-4" />
                                </BlockCommand>
                              )}
                              <BlockCommand
                                label="Table settings"
                                onClick={() => setTableSettings(true)}
                              >
                                <Palette className="size-4" />
                              </BlockCommand>
                            </>
                          )}
                          <BlockCommand
                            label="Move up"
                            disabled={index === 0}
                            onClick={() => moveBlock(index, -1)}
                          >
                            <ArrowUp className="size-4" />
                          </BlockCommand>
                          <BlockCommand
                            label="Move down"
                            disabled={index === design.blocks.length - 1}
                            onClick={() => moveBlock(index, 1)}
                          >
                            <ArrowDown className="size-4" />
                          </BlockCommand>
                          {block.kind !== "pricing" && (
                            <BlockCommand
                              label="Duplicate"
                              onClick={() => duplicateBlock(index)}
                            >
                              <Copy className="size-4" />
                            </BlockCommand>
                          )}
                          <BlockCommand
                            label="Delete"
                            danger
                            disabled={
                              block.kind === "pricing" &&
                              design.blocks.filter(
                                (item) => item.kind === "pricing",
                              ).length === 1
                            }
                            onClick={() => removeBlock(block.id)}
                          >
                            <Trash2 className="size-4" />
                          </BlockCommand>
                        </div>
                      </div>
                    )}
                    <div className="p-5">
                      {block.kind === "pricing" ? (
                        <QuoteTableOptionsProvider
                          value={{
                            enabled: true,
                            layout: design.tableLayout,
                            showImages: design.showProductImages,
                            showDescriptions: design.showProductDescriptions,
                            totalsPlacement: design.totalsPlacement,
                            totalsDetail: design.totalsDetail,
                            showCurrencyCode: design.showCurrencyCode,
                            emphasizeTotal: design.emphasizeTotal,
                            showTaxNote: design.showTaxNote,
                            lineContent: design.lineContent,
                            updateLineContent,
                          }}
                        >
                          {pricingTable({
                            ...(block.rowKeys === undefined
                              ? {}
                              : { rowKeys: block.rowKeys }),
                            title: block.title ?? "Pricing table",
                            onRowKeysChange: (rowKeys) =>
                              update(block.id, { rowKeys }),
                          })}
                          {design.blocks.filter(
                            (item) => item.kind === "pricing",
                          ).length > 1 &&
                            block.showSubtotal !== false &&
                            tableSubtotal(block.rowKeys)}
                        </QuoteTableOptionsProvider>
                      ) : block.kind === "table" ? (
                        <GeneralTableBlock
                          block={block}
                          readOnly={readOnly}
                          onChange={(patch) => update(block.id, patch)}
                        />
                      ) : block.kind === "heading" ? (
                        readOnly ? (
                          <h3 className="text-xl font-semibold">
                            {block.text}
                          </h3>
                        ) : (
                          <Input
                            value={block.text}
                            placeholder="Section heading"
                            aria-label="Section heading"
                            onChange={(event) =>
                              update(block.id, { text: event.target.value })
                            }
                          />
                        )
                      ) : block.kind === "paragraph" ? (
                        readOnly ? (
                          <p className="whitespace-pre-wrap leading-relaxed">
                            {block.text}
                          </p>
                        ) : (
                          <textarea
                            className="min-h-28 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm leading-relaxed text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
                            value={block.text}
                            placeholder="Write a paragraph…"
                            aria-label="Paragraph"
                            onChange={(event) =>
                              update(block.id, { text: event.target.value })
                            }
                          />
                        )
                      ) : block.kind === "quote" ? (
                        readOnly ? (
                          <blockquote className="border-l-4 border-[var(--quote-accent)] pl-5 text-lg italic">
                            <p>{block.text}</p>
                            {block.attribution && (
                              <footer className="mt-2 text-sm not-italic opacity-70">
                                {block.attribution}
                              </footer>
                            )}
                          </blockquote>
                        ) : (
                          <div className="grid gap-3">
                            <textarea
                              className="min-h-24 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
                              value={block.text}
                              placeholder="Add a customer quote or important statement…"
                              aria-label="Quote text"
                              onChange={(event) =>
                                update(block.id, { text: event.target.value })
                              }
                            />
                            <Input
                              value={block.attribution}
                              placeholder="Attribution (optional)"
                              aria-label="Quote attribution"
                              onChange={(event) =>
                                update(block.id, {
                                  attribution: event.target.value,
                                })
                              }
                            />
                          </div>
                        )
                      ) : block.kind === "list" ? (
                        readOnly ? (
                          block.ordered ? (
                            <ol className="list-decimal space-y-1 pl-6">
                              {block.items
                                .split("\n")
                                .filter(Boolean)
                                .map((item, itemIndex) => (
                                  <li key={itemIndex}>{item}</li>
                                ))}
                            </ol>
                          ) : (
                            <ul className="list-disc space-y-1 pl-6">
                              {block.items
                                .split("\n")
                                .filter(Boolean)
                                .map((item, itemIndex) => (
                                  <li key={itemIndex}>{item}</li>
                                ))}
                            </ul>
                          )
                        ) : (
                          <ListBlockEditor
                            ordered={block.ordered}
                            items={block.items}
                            onChange={(items) => update(block.id, { items })}
                          />
                        )
                      ) : block.kind === "divider" ? (
                        <hr className="border-0 border-t border-[var(--quote-table-header)]" />
                      ) : block.kind === "text" ? (
                        readOnly ? (
                          <>
                            <h3 className="text-lg font-semibold">
                              {block.heading}
                            </h3>
                            <p className="mt-2 whitespace-pre-wrap text-sm leading-relaxed opacity-80">
                              {block.body}
                            </p>
                          </>
                        ) : (
                          <div>
                            <Input
                              value={block.heading}
                              placeholder="Section heading"
                              aria-label="Section heading"
                              onChange={(event) =>
                                update(block.id, {
                                  heading: event.target.value,
                                })
                              }
                            />
                            <textarea
                              className="mt-3 min-h-28 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm leading-relaxed text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
                              value={block.body}
                              placeholder="Write the information your customer needs…"
                              aria-label="Section paragraph"
                              onChange={(event) =>
                                update(block.id, { body: event.target.value })
                              }
                            />
                          </div>
                        )
                      ) : (
                        <ImageContentBlock
                          block={block}
                          readOnly={readOnly}
                          onEdit={() => setEditingImageId(block.id)}
                        />
                      )}
                    </div>
                  </article>
                </div>
              ))}
            </div>
          )}
          <QuoteTableOptionsProvider
            value={{
              enabled: true,
              layout: design.tableLayout,
              showImages: design.showProductImages,
              showDescriptions: design.showProductDescriptions,
              totalsPlacement: design.totalsPlacement,
              totalsDetail: design.totalsDetail,
              showCurrencyCode: design.showCurrencyCode,
              emphasizeTotal: design.emphasizeTotal,
              showTaxNote: design.showTaxNote,
              lineContent: design.lineContent,
              updateLineContent,
            }}
          >
            <div className="mt-6">{totals}</div>
          </QuoteTableOptionsProvider>
          {!readOnly && (
            <BottomComposer
              index={design.blocks.length}
              onAdd={addSimpleBlock}
              onImage={chooseImage}
            />
          )}
        </div>
        <input
          ref={imageInput}
          type="file"
          accept="image/png,image/jpeg,image/webp"
          className="sr-only"
          onChange={(event) => {
            const file = event.target.files?.[0];
            const index = pendingImageIndex.current ?? design.blocks.length;
            if (file)
              imageData(file, (src) =>
                insertBlock(index, {
                  id: crypto.randomUUID(),
                  kind: "image",
                  src,
                  caption: "",
                  body: "",
                  placement: "full",
                  aspect: "landscape",
                  fit: "cover",
                  zoom: 100,
                }),
              );
            pendingImageIndex.current = null;
            event.currentTarget.value = "";
          }}
        />
        <input
          ref={replaceImageInput}
          type="file"
          accept="image/png,image/jpeg,image/webp"
          className="sr-only"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file && editingImageId !== null)
              imageData(file, (src) => update(editingImageId, { src }));
            event.currentTarget.value = "";
          }}
        />
      </section>
      {customize && (
        <CustomizeQuote
          design={design}
          saveError={saveError}
          onChange={setDesign}
          onClose={() => setCustomize(false)}
        />
      )}
      {tableSettings && (
        <CustomizeTable
          design={design}
          saveError={saveError}
          onChange={setDesign}
          onClose={() => setTableSettings(false)}
        />
      )}
      {editingImageId !== null && (() => {
        const imageBlock = design.blocks.find(
          (block) => block.id === editingImageId && block.kind === "image",
        );
        return imageBlock?.kind === "image" ? (
          <ImageBlockEditor
            block={imageBlock}
            onChange={(patch) => update(imageBlock.id, patch)}
            onReplace={() => replaceImageInput.current?.click()}
            onClose={() => setEditingImageId(null)}
          />
        ) : null;
      })()}
    </>
  );
});

function ListBlockEditor({
  ordered,
  items,
  onChange,
}: {
  ordered: boolean;
  items: string;
  onChange: (items: string) => void;
}) {
  const rows = items === "" ? [""] : items.split("\n");
  const replace = (index: number, value: string) =>
    onChange(rows.map((item, itemIndex) => (itemIndex === index ? value : item)).join("\n"));
  const remove = (index: number) => {
    const next = rows.filter((_, itemIndex) => itemIndex !== index);
    onChange(next.length === 0 ? "" : next.join("\n"));
  };
  const move = (index: number, direction: -1 | 1) => {
    const destination = index + direction;
    if (destination < 0 || destination >= rows.length) return;
    const next = [...rows];
    const [item] = next.splice(index, 1);
    if (item === undefined) return;
    next.splice(destination, 0, item);
    onChange(next.join("\n"));
  };

  return (
    <div>
      <div className="flex flex-col gap-2">
        {rows.map((item, index) => (
          <div
            key={index}
            className="grid grid-cols-[2.25rem_minmax(0,1fr)_auto] items-center gap-3 rounded-xl border border-default bg-surface p-3 shadow-sm"
          >
            <span className="grid size-9 place-items-center rounded-lg bg-raised text-sm font-semibold text-secondary">
              {ordered ? index + 1 : "•"}
            </span>
            <Input
              value={item}
              aria-label={`${ordered ? "Numbered" : "Bullet"} item ${index + 1}`}
              placeholder="Write an item"
              onChange={(event) => replace(index, event.target.value)}
            />
            <div className="flex items-center gap-1">
              <BlockCommand
                label="Move item up"
                disabled={index === 0}
                onClick={() => move(index, -1)}
              >
                <ArrowUp className="size-4" />
              </BlockCommand>
              <BlockCommand
                label="Move item down"
                disabled={index === rows.length - 1}
                onClick={() => move(index, 1)}
              >
                <ArrowDown className="size-4" />
              </BlockCommand>
              <BlockCommand label="Remove item" danger onClick={() => remove(index)}>
                <Trash2 className="size-4" />
              </BlockCommand>
            </div>
          </div>
        ))}
      </div>
      <button
        type="button"
        className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
        onClick={() => onChange(items === "" ? "\n" : `${items}\n`)}
      >
        <Plus className="size-4" aria-hidden="true" /> Add item below
      </button>
    </div>
  );
}

type ImageBlock = Extract<Block, { kind: "image" }>;

const IMAGE_ASPECT = {
  natural: "max-h-[520px]",
  landscape: "aspect-[16/7]",
  square: "aspect-square",
} as const;

const IMAGE_BLOCK_ZOOM = {
  50: "scale-50",
  75: "scale-75",
  100: "scale-100",
  125: "scale-125",
  150: "scale-150",
  175: "scale-[1.75]",
  200: "scale-200",
} as const;

function ImageContentBlock({
  block,
  readOnly,
  onEdit,
}: {
  block: ImageBlock;
  readOnly: boolean;
  onEdit: () => void;
}) {
  const placement = block.placement ?? "full";
  const image = (
    <div className="group/image relative overflow-hidden rounded-xl bg-raised">
      <img
        src={block.src}
        alt={block.caption || "Quotation image"}
        className={cx(
          "w-full transition-transform duration-200",
          IMAGE_ASPECT[block.aspect ?? "landscape"],
          block.fit === "contain" ? "object-contain" : "object-cover",
          IMAGE_BLOCK_ZOOM[block.zoom ?? 100],
        )}
        onDoubleClick={readOnly ? undefined : onEdit}
      />
      {!readOnly && (
        <button
          type="button"
          className="absolute right-3 top-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface/95 px-3 text-sm font-semibold text-primary shadow-md transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
          onClick={onEdit}
        >
          <Pencil className="size-4" aria-hidden="true" /> Edit image
        </button>
      )}
    </div>
  );
  const copy = (block.body || block.caption) && (
    <div className="flex flex-col justify-center px-1 py-2">
      {block.body && (
        <RichTextContent value={block.body} />
      )}
      {block.caption && (
        <p className={cx("text-xs opacity-65", block.body && "mt-3")}>
          {block.caption}
        </p>
      )}
    </div>
  );

  if (placement === "full")
    return (
      <figure>
        {image}
        {copy && <figcaption className="mt-3">{copy}</figcaption>}
      </figure>
    );
  return (
    <figure className="grid items-center gap-6 md:grid-cols-2">
      {placement === "left" ? image : copy}
      {placement === "left" ? copy : image}
    </figure>
  );
}

function ImageBlockEditor({
  block,
  onChange,
  onReplace,
  onClose,
}: {
  block: ImageBlock;
  onChange: (patch: Partial<ImageBlock>) => void;
  onReplace: () => void;
  onClose: () => void;
}) {
  return (
    <Modal
      title="Edit image block"
      icon={<ImagePlus className="size-5" />}
      onClose={onClose}
      wide
      footer={
        <>
          <p className="mr-auto text-xs text-secondary">
            Changes are shown immediately in the quotation.
          </p>
          <Button onClick={onClose}>Done</Button>
        </>
      }
    >
      <div className="grid gap-6 md:grid-cols-[minmax(0,1.15fr)_minmax(18rem,.85fr)]">
        <div className="overflow-hidden rounded-2xl border border-default bg-raised p-4">
          <img
            src={block.src}
            alt={block.caption || "Quotation image preview"}
            className={cx(
              "w-full rounded-xl bg-surface transition-transform duration-200",
              IMAGE_ASPECT[block.aspect ?? "landscape"],
              block.fit === "contain" ? "object-contain" : "object-cover",
              IMAGE_BLOCK_ZOOM[block.zoom ?? 100],
            )}
          />
          <button
            type="button"
            className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
            onClick={onReplace}
          >
            <Upload className="size-4" aria-hidden="true" /> Replace image
          </button>
        </div>
        <div className="flex flex-col gap-6">
          <ImageOptionGroup
            label="Place text"
            value={block.placement ?? "full"}
            options={[
              ["full", "Below image"],
              ["left", "Image left"],
              ["right", "Image right"],
            ]}
            onChange={(placement) => onChange({ placement })}
          />
          <ImageOptionGroup
            label="Image frame"
            value={block.aspect ?? "landscape"}
            options={[
              ["natural", "Natural"],
              ["landscape", "Wide"],
              ["square", "Square"],
            ]}
            onChange={(aspect) => onChange({ aspect })}
          />
          <ImageOptionGroup
            label="Fit"
            value={block.fit ?? "cover"}
            options={[
              ["cover", "Fill frame"],
              ["contain", "Show whole image"],
            ]}
            onChange={(fit) => onChange({ fit })}
          />
          <ImageOptionGroup
            label="Zoom"
            value={block.zoom ?? 100}
            options={[
              [50, "50%"],
              [75, "75%"],
              [100, "100%"],
              [125, "125%"],
              [150, "150%"],
              [175, "175%"],
              [200, "200%"],
            ]}
            onChange={(zoom) => onChange({ zoom })}
          />
        </div>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <div>
          <p className="text-sm font-semibold text-primary">Supporting text</p>
          <RichTextEditor
            value={block.body ?? ""}
            placeholder="Explain the product, project, or result shown in the image."
            onChange={(body) => onChange({ body })}
          />
        </div>
        <label className="text-sm font-semibold text-primary">
          Caption
          <textarea
            className="mt-2 min-h-28 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm font-normal leading-relaxed text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
            value={block.caption}
            placeholder="Optional short caption"
            onChange={(event) => onChange({ caption: event.target.value })}
          />
        </label>
      </div>
    </Modal>
  );
}

const RICH_TEXT_TAGS = new Set([
  "B",
  "BR",
  "EM",
  "H2",
  "H3",
  "I",
  "LI",
  "OL",
  "P",
  "STRONG",
  "UL",
]);

function escapeRichText(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;")
    .replaceAll("\n", "<br>");
}

function sanitizeRichText(value: string): string {
  if (!value.includes("<")) return escapeRichText(value);
  const template = document.createElement("template");
  template.innerHTML = value;
  const elements = [...template.content.querySelectorAll("*")];
  for (const element of elements) {
    if (!RICH_TEXT_TAGS.has(element.tagName)) {
      element.replaceWith(...element.childNodes);
      continue;
    }
    for (const attribute of [...element.attributes])
      element.removeAttribute(attribute.name);
  }
  return template.innerHTML;
}

function RichTextContent({ value }: { value: string }) {
  return (
    <div
      className="text-sm leading-relaxed opacity-90 [&_h2]:mb-2 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:mb-2 [&_h3]:text-lg [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:space-y-1 [&_ol]:pl-6 [&_p+p]:mt-3 [&_strong]:font-semibold [&_ul]:list-disc [&_ul]:space-y-1 [&_ul]:pl-6"
      dangerouslySetInnerHTML={{ __html: sanitizeRichText(value) }}
    />
  );
}

function RichTextEditor({
  value,
  placeholder,
  onChange,
}: {
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  const editor = useRef<HTMLDivElement>(null);
  const lastEmitted = useRef("");
  const [showTools, setShowTools] = useState(false);

  useEffect(() => {
    if (editor.current !== null && value !== lastEmitted.current) {
      editor.current.innerHTML = sanitizeRichText(value);
      lastEmitted.current = value;
    }
  }, [value]);

  const emit = () => {
    if (editor.current === null) return;
    const next = editor.current.innerHTML;
    lastEmitted.current = next;
    onChange(next);
  };
  const inspectSelection = () => {
    const selection = window.getSelection();
    const node = selection?.anchorNode;
    setShowTools(
      selection !== null &&
        !selection.isCollapsed &&
        node != null &&
        editor.current?.contains(node) === true,
    );
  };
  const command = (name: string, argument?: string) => {
    editor.current?.focus();
    document.execCommand(name, false, argument);
    emit();
    inspectSelection();
  };

  return (
    <div className="relative mt-2">
      {showTools && (
        <div
          className="absolute -top-12 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-xl border border-default bg-surface p-1.5 shadow-lg"
          role="toolbar"
          aria-label="Text formatting"
          onMouseDown={(event) => event.preventDefault()}
        >
          <RichTextCommand label="Bold" onClick={() => command("bold")}>
            <Bold className="size-4" />
          </RichTextCommand>
          <RichTextCommand label="Italic" onClick={() => command("italic")}>
            <Italic className="size-4" />
          </RichTextCommand>
          <RichTextCommand
            label="Heading 2"
            onClick={() => command("formatBlock", "h2")}
          >
            <Heading2 className="size-4" />
          </RichTextCommand>
          <RichTextCommand
            label="Paragraph"
            onClick={() => command("formatBlock", "p")}
          >
            <Pilcrow className="size-4" />
          </RichTextCommand>
          <RichTextCommand
            label="Bullet list"
            onClick={() => command("insertUnorderedList")}
          >
            <List className="size-4" />
          </RichTextCommand>
          <RichTextCommand
            label="Numbered list"
            onClick={() => command("insertOrderedList")}
          >
            <ListOrdered className="size-4" />
          </RichTextCommand>
        </div>
      )}
      <div
        ref={editor}
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        aria-multiline="true"
        aria-label="Supporting text"
        data-placeholder={placeholder}
        className="min-h-32 w-full overflow-y-auto rounded-md border border-default bg-surface px-4 py-3 text-sm font-normal leading-relaxed text-primary selection:bg-accent-soft selection:text-primary empty:before:pointer-events-none empty:before:text-tertiary empty:before:content-[attr(data-placeholder)] focus:border-accent focus:outline-none [&_h2]:my-2 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:my-2 [&_h3]:text-lg [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:pl-6 [&_strong]:font-semibold [&_ul]:list-disc [&_ul]:pl-6"
        onInput={emit}
        onMouseUp={inspectSelection}
        onKeyUp={inspectSelection}
        onBlur={() => {
          if (editor.current !== null) {
            const clean = sanitizeRichText(editor.current.innerHTML);
            editor.current.innerHTML = clean;
            lastEmitted.current = clean;
            onChange(clean);
          }
          setShowTools(false);
        }}
      />
    </div>
  );
}

function RichTextCommand({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className="grid size-9 place-items-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function ImageOptionGroup<T extends string | number>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: Array<readonly [T, string]>;
  onChange: (value: T) => void;
}) {
  return (
    <fieldset>
      <legend className="text-xs font-semibold uppercase tracking-wide text-tertiary">
        {label}
      </legend>
      <div className="mt-3 grid grid-cols-2 gap-2">
        {options.map(([id, name]) => (
          <button
            key={id}
            type="button"
            className={cx(
              "min-h-11 rounded-xl border px-3 text-left text-sm font-semibold transition-colors",
              value === id
                ? "border-accent bg-accent-soft text-accent"
                : "border-default bg-surface text-primary hover:border-accent hover:bg-accent-soft",
            )}
            onClick={() => onChange(id)}
          >
            {name}
          </button>
        ))}
      </div>
    </fieldset>
  );
}

type GeneralTable = Extract<Block, { kind: "table" }>;

function GeneralTableBlock({
  block,
  readOnly,
  onChange,
}: {
  block: GeneralTable;
  readOnly: boolean;
  onChange: (patch: Partial<GeneralTable>) => void;
}) {
  const addColumn = () => {
    const id = crypto.randomUUID();
    onChange({
      columns: [
        ...block.columns,
        { id, label: `Column ${block.columns.length + 1}` },
      ],
      rows: block.rows.map((row) => ({
        ...row,
        cells: { ...row.cells, [id]: "" },
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
  const addRow = () =>
    onChange({
      rows: [
        ...block.rows,
        {
          id: crypto.randomUUID(),
          cells: Object.fromEntries(
            block.columns.map((column) => [column.id, ""]),
          ),
        },
      ],
    });

  if (readOnly) {
    return (
      <div className="overflow-x-auto rounded-xl border border-default">
        <table className="w-full border-collapse text-left text-sm">
          <thead className="bg-[var(--quote-table-header)]">
            <tr>
              {block.columns.map((column) => (
                <th key={column.id} className="px-4 py-3 font-semibold">
                  {column.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row) => (
              <tr key={row.id} className="border-t border-default">
                {block.columns.map((column) => (
                  <td key={column.id} className="px-4 py-3 align-top">
                    {row.cells[column.id]}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-primary">Information table</h3>
          <p className="mt-1 text-xs text-secondary">
            Rename columns, then add as many rows or columns as the document needs.
          </p>
        </div>
        <button
          type="button"
          className="inline-flex min-h-9 items-center gap-2 rounded-lg border border-default bg-surface px-3 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
          onClick={addColumn}
        >
          <Plus className="size-4" aria-hidden="true" /> Add column
        </button>
      </div>
      <div className="overflow-x-auto rounded-xl border border-default">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-raised/50">
            <tr>
              {block.columns.map((column, columnIndex) => (
                <th key={column.id} className="min-w-44 border-r border-default p-2 last:border-r-0">
                  <div className="flex items-center gap-2">
                    <Input
                      value={column.label}
                      aria-label={`Column ${columnIndex + 1} name`}
                      onChange={(event) =>
                        onChange({
                          columns: block.columns.map((item) =>
                            item.id === column.id
                              ? { ...item, label: event.target.value }
                              : item,
                          ),
                        })
                      }
                    />
                    <button
                      type="button"
                      className="grid size-9 shrink-0 place-items-center rounded-lg text-secondary transition-colors hover:bg-danger-tint hover:text-danger disabled:cursor-not-allowed disabled:opacity-35"
                      aria-label={`Remove ${column.label || `column ${columnIndex + 1}`}`}
                      disabled={block.columns.length === 1}
                      onClick={() => removeColumn(column.id)}
                    >
                      <Trash2 className="size-4" aria-hidden="true" />
                    </button>
                  </div>
                </th>
              ))}
              <th className="w-12" aria-label="Row actions" />
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row, rowIndex) => (
              <tr key={row.id} className="border-t border-default">
                {block.columns.map((column) => (
                  <td key={column.id} className="border-r border-default p-2 last:border-r-0">
                    <Input
                      value={row.cells[column.id] ?? ""}
                      aria-label={`${column.label || "Column"}, row ${rowIndex + 1}`}
                      placeholder="Enter value"
                      onChange={(event) =>
                        onChange({
                          rows: block.rows.map((item) =>
                            item.id === row.id
                              ? {
                                  ...item,
                                  cells: {
                                    ...item.cells,
                                    [column.id]: event.target.value,
                                  },
                                }
                              : item,
                          ),
                        })
                      }
                    />
                  </td>
                ))}
                <td className="p-2 text-center">
                  <button
                    type="button"
                    className="grid size-9 place-items-center rounded-lg text-secondary transition-colors hover:bg-danger-tint hover:text-danger"
                    aria-label={`Remove row ${rowIndex + 1}`}
                    onClick={() =>
                      onChange({
                        rows: block.rows.filter((item) => item.id !== row.id),
                      })
                    }
                  >
                    <Trash2 className="size-4" aria-hidden="true" />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {block.rows.length === 0 && (
          <div className="px-5 py-8 text-center text-sm text-secondary">
            Add the first row to begin this table.
          </div>
        )}
      </div>
      <button
        type="button"
        className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
        onClick={addRow}
      >
        <Plus className="size-4" aria-hidden="true" /> Add row below
      </button>
    </div>
  );
}

function blockName(block: Block): string {
  switch (block.kind) {
    case "heading":
      return "Heading";
    case "paragraph":
      return "Paragraph";
    case "quote":
      return "Quote";
    case "list":
      return block.ordered ? "Numbered list" : "Bullet list";
    case "divider":
      return "Divider";
    case "image":
      return "Image";
    case "pricing":
      return "Pricing table";
    case "table":
      return "Table";
    default:
      return "Text";
  }
}

type InsertKind =
  | "heading"
  | "paragraph"
  | "quote"
  | "list"
  | "divider"
  | "pricing"
  | "table";

function BottomComposer({
  index,
  onAdd,
  onImage,
}: {
  index: number;
  onAdd: (index: number, kind: InsertKind, ordered?: boolean) => void;
  onImage: (index: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const add = (kind: InsertKind, ordered = false) => {
    onAdd(index, kind, ordered);
    setOpen(false);
  };
  return (
    <div
      className="relative mt-5 flex flex-col items-center"
      aria-label="Add quotation content"
    >
      <button
        type="button"
        className="inline-flex min-h-11 items-center gap-2 rounded-xl bg-accent px-5 text-sm font-semibold text-on-accent shadow-sm transition-colors hover:bg-accent-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <Plus className="size-4" aria-hidden="true" /> Add block below
      </button>
      {open && (
        <div className="mt-3 w-full max-w-3xl rounded-2xl border border-default bg-surface p-4 shadow-xl">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <h3 className="font-semibold text-primary">Add to quotation</h3>
              <p className="mt-0.5 text-sm text-secondary">
                Choose what should appear next in the document.
              </p>
            </div>
            <button
              type="button"
              className="rounded-lg p-2 text-secondary hover:bg-accent-soft hover:text-accent"
              aria-label="Close block picker"
              onClick={() => setOpen(false)}
            >
              <X className="size-4" />
            </button>
          </div>
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            <AddButton
              label="Heading"
              help="Title a new section"
              Icon={Heading2}
              onClick={() => add("heading")}
            />
            <AddButton
              label="Paragraph"
              help="Add explanatory text"
              Icon={AlignLeft}
              onClick={() => add("paragraph")}
            />
            <AddButton
              label="Quote"
              help="Highlight a statement"
              Icon={Quote}
              onClick={() => add("quote")}
            />
            <AddButton
              label="Bullet list"
              help="List key points"
              Icon={List}
              onClick={() => add("list")}
            />
            <AddButton
              label="Numbered list"
              help="Show ordered steps"
              Icon={ListOrdered}
              onClick={() => add("list", true)}
            />
            <AddButton
              label="Image"
              help="Upload a visual"
              Icon={ImagePlus}
              onClick={() => {
                onImage(index);
                setOpen(false);
              }}
            />
            <AddButton
              label="Divider"
              help="Separate sections"
              Icon={Minus}
              onClick={() => add("divider")}
            />
            <AddButton
              label="Pricing table"
              help="Group products and services"
              Icon={Table2}
              onClick={() => add("pricing")}
            />
            <AddButton
              label="Table"
              help="Create rows and columns for any information"
              Icon={Rows3}
              onClick={() => add("table")}
            />
          </div>
        </div>
      )}
    </div>
  );
}

function AddButton({
  label,
  help,
  Icon,
  disabled = false,
  onClick,
}: {
  label: string;
  help: string;
  Icon: typeof AlignLeft;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      className="flex min-h-20 items-center gap-3 rounded-xl border border-default bg-surface px-4 py-3 text-left text-primary transition-colors hover:border-accent hover:bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 disabled:cursor-not-allowed disabled:opacity-45"
      onClick={onClick}
    >
      <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">
        <Icon className="size-5" aria-hidden="true" />
      </span>
      <span>
        <span className="block text-sm font-semibold">{label}</span>
        <span className="mt-0.5 block text-xs text-secondary">{help}</span>
      </span>
    </button>
  );
}

function EmptyBuilder({ readOnly }: { readOnly: boolean }) {
  return (
    <div className="flex min-h-28 items-center justify-center rounded-xl border border-dashed border-default bg-[var(--quote-background)] px-6 py-8 text-center">
      <div>
        <h3 className="text-base font-semibold text-primary">
          {readOnly ? "No proposal content" : "Start your quotation below"}
        </h3>
        {!readOnly && (
          <p className="mt-1 text-sm text-secondary">
            Add text, a heading, or an image as the first block.
          </p>
        )}
      </div>
    </div>
  );
}

function BlockCommand({
  label,
  children,
  onClick,
  disabled = false,
  danger = false,
}: {
  label: string;
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      className={cx(
        "inline-flex min-h-9 items-center gap-1.5 rounded-lg px-2.5 text-xs font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-35",
        danger
          ? "text-danger hover:bg-danger-tint"
          : "text-secondary hover:bg-surface hover:text-primary",
      )}
      onClick={onClick}
    >
      {children}
      {label}
    </button>
  );
}

function CustomizeQuote({
  design,
  saveError,
  onChange,
  onClose,
}: {
  design: Design;
  saveError: string;
  onChange: React.Dispatch<React.SetStateAction<Design>>;
  onClose: () => void;
}) {
  const logoInput = useRef<HTMLInputElement>(null);
  const setColor = (name: keyof Colors, value: string) =>
    onChange((current) => ({
      ...current,
      colors: { ...current.colors, [name]: value },
    }));
  return (
    <Modal
      title="Customize quotation"
      icon={<Palette className="size-5" />}
      onClose={onClose}
      wide
      actions={
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-raised hover:text-primary"
          aria-label="Close"
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      }
      footer={
        <>
          <p
            className={cx(
              "mr-auto text-xs",
              saveError ? "text-danger" : "text-secondary",
            )}
          >
            {saveError || "Changes are saved automatically."}
          </p>
          <Button onClick={onClose}>Done</Button>
        </>
      }
    >
      <div className="grid gap-6 md:grid-cols-[220px_minmax(0,1fr)]">
        <section>
          <h3 className="text-sm font-semibold text-primary">Logo</h3>
          <p className="mt-1 text-xs text-secondary">PNG, JPG, WebP, or SVG.</p>
          <button
            type="button"
            className="mt-3 flex min-h-28 w-full items-center justify-center overflow-hidden rounded-xl border border-dashed border-default bg-raised/30 p-3 text-sm font-medium text-secondary hover:border-accent hover:bg-accent-soft hover:text-accent"
            onClick={() => logoInput.current?.click()}
          >
            {design.logo ? (
              <img
                src={design.logo}
                alt="Quote logo"
                className="max-h-20 max-w-full object-contain"
              />
            ) : (
              <span className="flex items-center gap-2">
                <Upload className="size-4" /> Upload logo
              </span>
            )}
          </button>
          {design.logo && (
            <button
              type="button"
              className="mt-2 text-xs font-semibold text-secondary hover:text-danger"
              onClick={() => onChange((current) => ({ ...current, logo: "" }))}
            >
              Remove logo
            </button>
          )}
          <input
            ref={logoInput}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/svg+xml"
            className="sr-only"
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file)
                imageData(file, (logo) =>
                  onChange((current) => ({ ...current, logo })),
                );
            }}
          />
        </section>
        <div className="min-w-0">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-primary">
                Document colours
              </h3>
              <p className="mt-1 text-xs text-secondary">
                Applied only to this customer-facing quotation.
              </p>
            </div>
            <button
              type="button"
              className="text-xs font-semibold text-accent hover:text-accent-hover"
              onClick={() =>
                onChange((current) => ({ ...current, colors: DEFAULT_COLORS }))
              }
            >
              Reset
            </button>
          </div>
          <div className="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-3">
            <ColorField
              label="Accent"
              value={design.colors.accent}
              onChange={(value) => setColor("accent", value)}
            />
            <ColorField
              label="Page"
              value={design.colors.background}
              onChange={(value) => setColor("background", value)}
            />
            <ColorField
              label="Text"
              value={design.colors.text}
              onChange={(value) => setColor("text", value)}
            />
            <ColorField
              label="Table heading"
              value={design.colors.tableHeader}
              onChange={(value) => setColor("tableHeader", value)}
            />
            <ColorField
              label="Table rows"
              value={design.colors.tableRows}
              onChange={(value) => setColor("tableRows", value)}
            />
          </div>
          <h3 className="mt-6 text-sm font-semibold text-primary">
            Typography
          </h3>
          <div className="mt-3 grid gap-3 sm:grid-cols-3">
            {themeChoices.map((theme) => (
              <button
                key={theme.id}
                type="button"
                className={cx(
                  "flex min-h-20 items-center gap-3 rounded-xl border bg-surface px-3 py-3 text-left hover:border-accent hover:bg-accent-soft",
                  design.theme === theme.id
                    ? "border-accent shadow-[inset_0_0_0_1px_var(--accent)]"
                    : "border-default",
                )}
                onClick={() =>
                  onChange((current) => ({ ...current, theme: theme.id }))
                }
              >
                <span className="min-w-0 flex-1">
                  <strong className="block text-sm font-semibold text-primary">
                    {theme.name}
                  </strong>
                  <small className="mt-1 block text-xs text-secondary">
                    {theme.help}
                  </small>
                </span>
                {design.theme === theme.id && (
                  <Check className="size-4 shrink-0 text-accent" />
                )}
              </button>
            ))}
          </div>
        </div>
      </div>
    </Modal>
  );
}

function CustomizeTable({
  design,
  saveError,
  onChange,
  onClose,
}: {
  design: Design;
  saveError: string;
  onChange: React.Dispatch<React.SetStateAction<Design>>;
  onClose: () => void;
}) {
  return (
    <Modal
      title="Table settings"
      icon={<Table2 className="size-5" />}
      onClose={onClose}
      wide
      actions={
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-accent-soft hover:text-accent"
          aria-label="Close table settings"
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      }
      footer={
        <>
          <p
            className={cx(
              "mr-auto text-xs",
              saveError ? "text-danger" : "text-secondary",
            )}
          >
            {saveError || "Table changes are saved automatically."}
          </p>
          <Button onClick={onClose}>Done</Button>
        </>
      }
    >
      <section>
        <h3 className="text-sm font-semibold text-primary">Choose a layout</h3>
        <p className="mt-1 text-sm text-secondary">
          Select a starting point, then adjust the visible content and columns
          below.
        </p>
        <div className="mt-5 grid gap-5 sm:grid-cols-3">
          {(
            [
              ["compact", "Compact", "Names and prices only"],
              ["detailed", "Detailed", "Descriptions with optional images"],
              ["catalogue", "Catalogue", "Larger product images and details"],
            ] as const
          ).map(([layout, label, help]) => (
            <button
              key={layout}
              type="button"
              className={cx(
                "group relative min-h-64 overflow-hidden rounded-2xl border bg-surface !p-5 text-left shadow-sm ring-1 transition-all hover:-translate-y-0.5 hover:border-accent hover:shadow-md",
                design.tableLayout === layout
                  ? "border-accent bg-accent-soft ring-accent/25"
                  : "border-default ring-default hover:bg-accent-soft/30",
              )}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  tableLayout: layout,
                  showProductDescriptions: layout !== "compact",
                  showProductImages: layout === "catalogue",
                }))
              }
            >
              <LayoutPreview
                layout={layout}
                selected={design.tableLayout === layout}
              />
              <span className="mt-4 flex items-start justify-between gap-4 px-1 pb-2">
                <span>
                  <strong className="block text-sm font-semibold text-primary">
                    {label}
                  </strong>
                  <span className="mt-1 block text-xs leading-relaxed text-secondary">
                    {help}
                  </span>
                </span>
                <span
                  className={cx(
                    "mt-0.5 grid size-5 shrink-0 place-items-center rounded-full border",
                    design.tableLayout === layout
                      ? "border-accent bg-accent text-on-accent"
                      : "border-default bg-surface group-hover:border-accent",
                  )}
                >
                  {design.tableLayout === layout && (
                    <Check className="size-3.5" />
                  )}
                </span>
              </span>
            </button>
          ))}
        </div>
      </section>

      <section className="mt-8 border-t border-subtle pt-7">
        <h3 className="text-sm font-semibold text-primary">Product content</h3>
        <p className="mt-1 text-sm text-secondary">
          Optional information shown with each product or service.
        </p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <TableToggle
            label="Product images"
            help="Upload an image for each table row"
            checked={design.showProductImages}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showProductImages: !current.showProductImages,
              }))
            }
          />
          <TableToggle
            label="Product descriptions"
            help="Add specifications or scope beneath each item"
            checked={design.showProductDescriptions}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showProductDescriptions: !current.showProductDescriptions,
              }))
            }
          />
        </div>
      </section>

      <section className="mt-8 border-t border-subtle pt-7">
        <h3 className="text-sm font-semibold text-primary">Visible columns</h3>
        <p className="mt-1 text-sm text-secondary">
          Product name and quotation total always remain visible.
        </p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {(
            [
              ["unit", "Unit"],
              ["quantity", "Quantity"],
              ["unitPrice", "Unit price"],
              ["vat", "VAT rate"],
              ["net", "Line total"],
            ] as const
          ).map(([key, label]) => (
            <TableToggle
              key={key}
              label={label}
              help={`Show the ${label.toLowerCase()} column`}
              checked={design.columns[key]}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  columns: { ...current.columns, [key]: !current.columns[key] },
                }))
              }
            />
          ))}
        </div>
      </section>

      <section className="mt-8 border-t border-subtle pt-7">
        <h3 className="text-sm font-semibold text-primary">
          Quotation total
        </h3>
        <p className="mt-1 text-sm text-secondary">
          Choose how the final total across every pricing table appears. Each
          pricing table controls its own subtotal from the table toolbar.
        </p>
        <div className="mt-5 grid gap-5 sm:grid-cols-3">
          {(
            [
              ["summary", "Summary card", "Compact and right aligned"],
              ["full", "Full width", "Balances the entire table"],
              ["footer", "Table footer", "Feels attached to the rows"],
            ] as const
          ).map(([placement, label, help]) => (
            <button
              key={placement}
              type="button"
              className={cx(
                "group min-h-40 rounded-2xl border !p-5 text-left shadow-sm transition-all hover:-translate-y-0.5 hover:border-accent hover:shadow-md",
                design.totalsPlacement === placement
                  ? "border-accent bg-accent-soft ring-1 ring-accent/20"
                  : "border-default bg-surface",
              )}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  totalsPlacement: placement,
                }))
              }
            >
              <TotalsPreview placement={placement} />
              <span className="mt-4 flex items-start justify-between gap-4 px-1 pb-1">
                <span>
                  <strong className="block text-sm font-semibold text-primary">
                    {label}
                  </strong>
                  <span className="mt-0.5 block text-xs text-secondary">
                    {help}
                  </span>
                </span>
                <span
                  className={cx(
                    "grid size-5 shrink-0 place-items-center rounded-full border",
                    design.totalsPlacement === placement
                      ? "border-accent bg-accent text-on-accent"
                      : "border-default group-hover:border-accent",
                  )}
                >
                  {design.totalsPlacement === placement && (
                    <Check className="size-3.5" />
                  )}
                </span>
              </span>
            </button>
          ))}
        </div>

        <h4 className="mt-10 border-t border-subtle pt-7 text-xs font-semibold uppercase tracking-wide text-tertiary">
          Amount details
        </h4>
        <div className="mt-5 grid gap-4 sm:grid-cols-3">
          {(
            [
              ["total", "Total only", "The shortest summary"],
              ["summary", "Net, VAT and total", "Recommended for most quotes"],
              ["breakdown", "VAT breakdown", "Show every VAT rate"],
            ] as const
          ).map(([detail, label, help]) => (
            <TableToggle
              key={detail}
              label={label}
              help={help}
              checked={design.totalsDetail === detail}
              onClick={() =>
                onChange((current) => ({ ...current, totalsDetail: detail }))
              }
            />
          ))}
        </div>
        <div className="mt-4 grid gap-4 sm:grid-cols-3">
          <TableToggle
            label="Currency code"
            help="Show EUR, USD, or the quote currency"
            checked={design.showCurrencyCode}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showCurrencyCode: !current.showCurrencyCode,
              }))
            }
          />
          <TableToggle
            label="Emphasize total"
            help="Give the final amount stronger hierarchy"
            checked={design.emphasizeTotal}
            onClick={() =>
              onChange((current) => ({
                ...current,
                emphasizeTotal: !current.emphasizeTotal,
              }))
            }
          />
          <TableToggle
            label="VAT note"
            help="Explain that VAT is shown separately"
            checked={design.showTaxNote}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showTaxNote: !current.showTaxNote,
              }))
            }
          />
        </div>
      </section>
    </Modal>
  );
}

function TotalsPreview({ placement }: { placement: QuoteTotalsPlacement }) {
  return (
    <span
      className="block h-20 rounded-xl border border-subtle bg-raised/40 p-3"
      aria-hidden="true"
    >
      <span className="block h-5 rounded bg-surface" />
      <span
        className={cx(
          "mt-2 flex flex-col gap-1 rounded-md bg-surface p-2",
          placement === "summary" && "ml-auto w-1/2",
          placement === "full" && "w-full",
          placement === "footer" &&
            "mt-3 w-full rounded-t-none border-t border-accent/35",
        )}
      >
        <span className="flex justify-between">
          <span className="h-1 w-8 rounded bg-secondary/20" />
          <span className="h-1 w-6 rounded bg-secondary/20" />
        </span>
        <span className="flex justify-between">
          <span className="h-1 w-6 rounded bg-primary/25" />
          <span className="h-1 w-8 rounded bg-accent/55" />
        </span>
      </span>
    </span>
  );
}

function LayoutPreview({
  layout,
  selected,
}: {
  layout: QuoteTableLayout;
  selected: boolean;
}) {
  return (
    <span
      className={cx(
        "block rounded-xl border p-3",
        selected ? "border-accent/25 bg-surface" : "border-subtle bg-raised/45",
      )}
      aria-hidden="true"
    >
      <span className="mb-2 flex items-center gap-2 border-b border-subtle pb-2">
        {layout === "catalogue" && (
          <span className="size-6 rounded-md bg-accent-soft" />
        )}
        <span className="h-1.5 w-16 rounded-full bg-secondary/25" />
        <span className="ml-auto h-1.5 w-8 rounded-full bg-accent/55" />
      </span>
      {[0, 1].map((row) => (
        <span key={row} className="flex items-center gap-2 py-1.5">
          {layout === "catalogue" && (
            <span className="size-8 shrink-0 rounded-md bg-accent-soft" />
          )}
          <span className="min-w-0 flex-1">
            <span className="block h-1.5 rounded-full bg-primary/20" />
            {layout !== "compact" && (
              <span className="mt-1.5 block h-1 w-3/4 rounded-full bg-secondary/15" />
            )}
          </span>
          <span className="h-1.5 w-8 rounded-full bg-secondary/20" />
        </span>
      ))}
    </span>
  );
}

function TableToggle({
  label,
  help,
  checked,
  onClick,
}: {
  label: string;
  help: string;
  checked: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={checked}
      className={cx(
        "flex min-h-20 items-center gap-5 rounded-xl border !px-6 !py-5 text-left shadow-sm transition-all hover:border-accent hover:bg-accent-soft hover:shadow-md",
        checked ? "border-accent bg-accent-soft" : "border-default bg-surface",
      )}
      onClick={onClick}
    >
      <span
        className={cx(
          "flex size-5 shrink-0 items-center justify-center rounded border",
          checked
            ? "border-accent bg-accent text-on-accent"
            : "border-default bg-surface",
        )}
      >
        {checked && <Check className="size-3.5" />}
      </span>
      <span>
        <strong className="block text-sm font-semibold text-primary">
          {label}
        </strong>
        <span className="mt-0.5 block text-xs text-secondary">{help}</span>
      </span>
    </button>
  );
}

function ColorField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const valid = /^#[0-9a-f]{6}$/i.test(value);
  return (
    <div className="rounded-xl border border-default bg-surface p-3 transition-colors hover:border-accent">
      <label
        className="block text-xs font-semibold text-primary"
        htmlFor={`quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`}
      >
        {label}
      </label>
      <div className="mt-2 flex items-center gap-2">
        <input
          type="color"
          value={valid ? value : DEFAULT_COLORS.accent}
          aria-label={`Choose ${label.toLowerCase()} colour`}
          title={`Choose ${label.toLowerCase()} colour`}
          className="size-11 shrink-0 cursor-pointer rounded-lg border border-default bg-surface p-1 shadow-sm"
          onChange={(event) => onChange(event.target.value)}
        />
        <input
          id={`quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`}
          value={value.toUpperCase()}
          aria-label={`${label} hex colour`}
          className="h-11 min-w-0 w-full rounded-lg border border-default bg-surface px-3 font-mono text-xs uppercase text-primary focus:border-accent focus:outline-none"
          maxLength={7}
          spellCheck={false}
          onChange={(event) => {
            const next = event.target.value.startsWith("#")
              ? event.target.value
              : `#${event.target.value}`;
            onChange(next.slice(0, 7));
          }}
        />
      </div>
    </div>
  );
}
