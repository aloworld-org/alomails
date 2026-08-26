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
  Heading1,
  Heading2,
  Heading3,
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
  RotateCcw,
  Rows3,
  Search,
  Table2,
  Type,
  Trash2,
  Upload,
  X,
} from "lucide-react";

import { Button, ChoicePicker, Modal, Select, cx } from "../ds";
import {
  QuoteTableOptionsProvider,
  type QuoteLineContent,
  type QuoteTableLayout,
  type QuoteTotalsDetail,
  type QuoteTotalsPlacement,
} from "./quoteTableOptions";

type Theme = "modern" | "editorial" | "minimal";
type HeaderAlignment = "left" | "right";
type Block =
  | { id: string; kind: "text"; heading: string; body: string }
  | { id: string; kind: "heading"; level: 1 | 2 | 3; text: string }
  | { id: string; kind: "paragraph"; text: string }
  | { id: string; kind: "quote"; text: string; attribution: string }
  | {
      id: string;
      kind: "list";
      ordered: boolean;
      items: string;
      columns?: 1 | 2 | 3;
    }
  | { id: string; kind: "divider" }
  | {
      id: string;
      kind: "image";
      src: string;
      caption: string;
      body?: string;
      placement?: "full" | "left" | "right";
      columnRatio?: "33-67" | "40-60" | "50-50" | "60-40" | "67-33";
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
  headerBackground: string;
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
  headerAlignment: HeaderAlignment;
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
  headerBackground: "#fffefc",
  text: "#102a43",
  tableHeader: "#f3f0ea",
  tableRows: "#fffefc",
};
const EMPTY: Design = {
  logo: "",
  headerAlignment: "left",
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

function hasPreviewText(value: string): boolean {
  return (
    value
      .replace(/<[^>]*>/g, "")
      .replaceAll("&nbsp;", " ")
      .trim().length > 0
  );
}

function blockHasPreviewContent(block: Block): boolean {
  switch (block.kind) {
    case "pricing":
      return block.rowKeys === undefined || block.rowKeys.length > 0;
    case "table":
      return generalTableHasContent(block);
    case "heading":
    case "paragraph":
      return hasPreviewText(block.text);
    case "quote":
      return hasPreviewText(block.text) || hasPreviewText(block.attribution);
    case "list":
      return block.items.split("\n").some(hasPreviewText);
    case "text":
      return hasPreviewText(block.heading) || hasPreviewText(block.body);
    case "image":
      return block.src.trim().length > 0;
    case "divider":
      return true;
  }
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
    tableSubtotal: (
      rowKeys?: string[],
      presentation?: {
        placement: QuoteTotalsPlacement;
        detail: QuoteTotalsDetail;
        showCurrencyCode: boolean;
        emphasizeTotal: boolean;
        showTaxNote: boolean;
      },
    ) => ReactNode;
    lineKeys: string[];
    onColumnsChange?: (columns: QuoteColumns) => void;
  }
>(function QuoteContentStudio(
  {
    quoteId,
    readOnly,
    preview = false,
    pricingTable,
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
      "--quote-header-background": design.colors.headerBackground,
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
    if (kind === "list")
      insertBlock(index, { id, kind, ordered, items: "", columns: 1 });
    if (kind === "divider") insertBlock(index, { id, kind });
    if (kind === "pricing") {
      setDesign((current) => ({
        ...current,
        blocks: [
          ...current.blocks
            .slice(0, index)
            .map((block) =>
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
              current.blocks.filter((block) => block.kind === "pricing")
                .length + 1
            }`,
          },
          ...current.blocks
            .slice(index)
            .map((block) =>
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
        className={cx(
          "overflow-hidden bg-[var(--quote-background)]",
          preview
            ? "rounded-none"
            : "rounded-2xl border border-default shadow-sm",
        )}
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
            <div
              className={cx(
                "mb-8 flex min-h-28 items-center justify-between gap-8 rounded-2xl bg-[var(--quote-header-background)] px-6 py-5 max-sm:px-4",
                design.headerAlignment === "right" && "flex-row-reverse",
              )}
            >
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
            <div className={cx("flex flex-col", readOnly ? "gap-8" : "gap-5")}>
              {design.blocks
                .filter((block) => !readOnly || blockHasPreviewContent(block))
                .map((block, index) => (
                  <div key={block.id}>
                    <article
                      className={cx(
                        "group/quote-block bg-[var(--quote-background)] text-[var(--quote-text)]",
                        readOnly
                          ? "overflow-visible"
                          : "overflow-hidden rounded-xl border border-[var(--quote-table-header)] shadow-sm",
                      )}
                    >
                      {!readOnly && (
                        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--quote-table-header)] bg-raised/40 px-4 py-2.5">
                          {block.kind === "pricing" ? (
                            <label className="flex min-w-0 flex-col gap-1.5 sm:flex-row sm:items-center sm:gap-3">
                              <span className="shrink-0 text-xs font-semibold uppercase tracking-wide text-tertiary">
                                Table name
                              </span>
                              <span className="relative block min-w-0">
                                <Pencil
                                  className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-tertiary"
                                  aria-hidden="true"
                                />
                                <input
                                  className="min-h-10 w-full min-w-52 rounded-lg border border-default bg-surface py-2 pl-9 pr-3 text-sm font-semibold text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none"
                                  value={block.title ?? "Pricing table"}
                                  aria-label="Table name"
                                  placeholder="Pricing table"
                                  onChange={(event) =>
                                    update(block.id, {
                                      title: event.target.value,
                                    })
                                  }
                                />
                              </span>
                            </label>
                          ) : (
                            <span className="text-xs font-semibold uppercase tracking-wide text-secondary">
                              {blockName(block)}
                            </span>
                          )}
                          <div className="flex flex-wrap items-center gap-3 opacity-0 transition-opacity group-hover/quote-block:opacity-100 group-focus-within/quote-block:opacity-100 max-md:opacity-100">
                            {(block.kind === "pricing" ||
                              block.kind === "image") && (
                              <div className="flex flex-wrap items-center gap-2 border-r border-default pr-3">
                                {block.kind === "pricing" && (
                                  <>
                                    {design.blocks.filter(
                                      (item) => item.kind === "pricing",
                                    ).length > 1 && (
                                      <BlockCommand
                                        accent
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
                                      accent
                                      label="Table settings"
                                      onClick={() => setTableSettings(true)}
                                    >
                                      <Palette className="size-4" />
                                    </BlockCommand>
                                  </>
                                )}
                                {block.kind === "image" && (
                                  <BlockCommand
                                    accent
                                    label="Edit block"
                                    onClick={() => setEditingImageId(block.id)}
                                  >
                                    <Pencil className="size-4" />
                                  </BlockCommand>
                                )}
                              </div>
                            )}
                            <div className="flex flex-wrap items-center gap-2">
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
                            </div>
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
                      <div className={readOnly ? "py-1" : "p-5"}>
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
                            {block.showSubtotal !== false &&
                              tableSubtotal(block.rowKeys, {
                                placement: design.totalsPlacement,
                                detail: design.totalsDetail,
                                showCurrencyCode: design.showCurrencyCode,
                                emphasizeTotal: design.emphasizeTotal,
                                showTaxNote: design.showTaxNote,
                              })}
                          </QuoteTableOptionsProvider>
                        ) : block.kind === "table" ? (
                          <GeneralTableBlock
                            block={block}
                            readOnly={readOnly}
                            onChange={(patch) => update(block.id, patch)}
                          />
                        ) : block.kind === "heading" ? (
                          readOnly ? (
                            block.level === 1 ? (
                              <h1 className="text-3xl font-semibold leading-tight">
                                <InlineRichTextContent value={block.text} />
                              </h1>
                            ) : block.level === 2 ? (
                              <h2 className="text-2xl font-semibold leading-tight">
                                <InlineRichTextContent value={block.text} />
                              </h2>
                            ) : (
                              <h3 className="text-xl font-semibold leading-tight">
                                <InlineRichTextContent value={block.text} />
                              </h3>
                            )
                          ) : (
                            <div className="grid gap-3 sm:grid-cols-[7rem_minmax(0,1fr)]">
                              <Select
                                fullWidth
                                value={String(block.level)}
                                aria-label="Heading level"
                                onChange={(event) =>
                                  update(block.id, {
                                    level: Number(event.target.value) as
                                      1 | 2 | 3,
                                  })
                                }
                              >
                                <option value="1">Heading 1</option>
                                <option value="2">Heading 2</option>
                                <option value="3">Heading 3</option>
                              </Select>
                              <InlineRichTextEditor
                                value={block.text}
                                placeholder="Section heading"
                                aria-label="Section heading"
                                onChange={(text) => update(block.id, { text })}
                              />
                            </div>
                          )
                        ) : block.kind === "paragraph" ? (
                          readOnly ? (
                            <RichTextContent value={block.text} />
                          ) : (
                            <RichTextEditor
                              value={block.text}
                              label="Paragraph"
                              placeholder="Write a paragraph…"
                              onChange={(text) => update(block.id, { text })}
                            />
                          )
                        ) : block.kind === "quote" ? (
                          readOnly ? (
                            <blockquote className="border-l-4 border-[var(--quote-accent)] pl-5 text-lg italic">
                              <RichTextContent value={block.text} />
                              {block.attribution && (
                                <footer className="mt-2 text-sm not-italic opacity-70">
                                  <InlineRichTextContent
                                    value={block.attribution}
                                  />
                                </footer>
                              )}
                            </blockquote>
                          ) : (
                            <div className="grid gap-3">
                              <RichTextEditor
                                value={block.text}
                                label="Quotation"
                                placeholder="Add a customer quote or important statement…"
                                onChange={(text) => update(block.id, { text })}
                              />
                              <InlineRichTextEditor
                                value={block.attribution}
                                placeholder="Attribution (optional)"
                                aria-label="Quote attribution"
                                onChange={(attribution) =>
                                  update(block.id, { attribution })
                                }
                              />
                            </div>
                          )
                        ) : block.kind === "list" ? (
                          readOnly ? (
                            block.ordered ? (
                              <ol
                                className={cx(
                                  "grid list-decimal gap-x-10 gap-y-2 pl-6",
                                  (block.columns ?? 1) === 2 &&
                                    "md:grid-cols-2",
                                  (block.columns ?? 1) === 3 &&
                                    "md:grid-cols-3",
                                )}
                              >
                                {block.items
                                  .split("\n")
                                  .filter(Boolean)
                                  .map((item, itemIndex) => (
                                    <li key={itemIndex}>
                                      <InlineRichTextContent value={item} />
                                    </li>
                                  ))}
                              </ol>
                            ) : (
                              <ul
                                className={cx(
                                  "grid list-disc gap-x-10 gap-y-2 pl-6",
                                  (block.columns ?? 1) === 2 &&
                                    "md:grid-cols-2",
                                  (block.columns ?? 1) === 3 &&
                                    "md:grid-cols-3",
                                )}
                              >
                                {block.items
                                  .split("\n")
                                  .filter(Boolean)
                                  .map((item, itemIndex) => (
                                    <li key={itemIndex}>
                                      <InlineRichTextContent value={item} />
                                    </li>
                                  ))}
                              </ul>
                            )
                          ) : (
                            <ListBlockEditor
                              ordered={block.ordered}
                              items={block.items}
                              columns={block.columns ?? 1}
                              onChange={(patch) => update(block.id, patch)}
                            />
                          )
                        ) : block.kind === "divider" ? (
                          <hr className="border-0 border-t border-[var(--quote-table-header)]" />
                        ) : block.kind === "text" ? (
                          readOnly ? (
                            <>
                              <h3 className="text-lg font-semibold">
                                <InlineRichTextContent value={block.heading} />
                              </h3>
                              <div className="mt-2 opacity-80">
                                <RichTextContent value={block.body} />
                              </div>
                            </>
                          ) : (
                            <div>
                              <InlineRichTextEditor
                                value={block.heading}
                                placeholder="Section heading"
                                aria-label="Section heading"
                                onChange={(heading) =>
                                  update(block.id, { heading })
                                }
                              />
                              <div className="mt-3">
                                <RichTextEditor
                                  value={block.body}
                                  label="Section text"
                                  placeholder="Write the information your customer needs…"
                                  onChange={(body) =>
                                    update(block.id, { body })
                                  }
                                />
                              </div>
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
                    {!readOnly && (
                      <BottomComposer
                        index={index + 1}
                        onAdd={addSimpleBlock}
                        onImage={chooseImage}
                      />
                    )}
                  </div>
                ))}
            </div>
          )}
          {!readOnly && design.blocks.length === 0 && (
            <BottomComposer
              index={0}
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
      {editingImageId !== null &&
        (() => {
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
  columns,
  onChange,
}: {
  ordered: boolean;
  items: string;
  columns: 1 | 2 | 3;
  onChange: (patch: { items?: string; columns?: 1 | 2 | 3 }) => void;
}) {
  const rows = items === "" ? [""] : items.split("\n");
  const replace = (index: number, value: string) =>
    onChange({
      items: rows
        .map((item, itemIndex) => (itemIndex === index ? value : item))
        .join("\n"),
    });
  const remove = (index: number) => {
    const next = rows.filter((_, itemIndex) => itemIndex !== index);
    onChange({ items: next.length === 0 ? "" : next.join("\n") });
  };
  const move = (index: number, direction: -1 | 1) => {
    const destination = index + direction;
    if (destination < 0 || destination >= rows.length) return;
    const next = [...rows];
    const [item] = next.splice(index, 1);
    if (item === undefined) return;
    next.splice(destination, 0, item);
    onChange({ items: next.join("\n") });
  };

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-primary">List layout</p>
          <p className="mt-0.5 text-xs text-secondary">
            Split longer lists into easy-to-scan columns.
          </p>
        </div>
        <div className="flex items-center gap-2 text-xs font-semibold text-secondary">
          <span>Columns</span>
          <div className="w-36">
            <ChoicePicker
              value={String(columns)}
              label={`${ordered ? "Numbered" : "Bullet"} list columns`}
              placeholder="Choose columns"
              options={[
                { value: "1", label: "1 column" },
                { value: "2", label: "2 columns" },
                { value: "3", label: "3 columns" },
              ]}
              onChange={(value) =>
                onChange({ columns: Number(value) as 1 | 2 | 3 })
              }
            />
          </div>
        </div>
      </div>
      <div
        className={cx(
          "grid gap-2",
          columns === 2 && "md:grid-cols-2",
          columns === 3 && "md:grid-cols-2 xl:grid-cols-3",
        )}
      >
        {rows.map((item, index) => (
          <div
            key={index}
            className="group/list-item grid grid-cols-[2.25rem_minmax(0,1fr)_7.75rem] items-center gap-3 rounded-xl border border-default bg-surface p-3 shadow-sm transition-colors hover:border-accent/30 focus-within:border-accent/30 max-md:grid-cols-[2.25rem_minmax(0,1fr)]"
          >
            <span className="grid size-9 place-items-center rounded-lg bg-raised text-sm font-semibold text-secondary">
              {ordered ? index + 1 : "•"}
            </span>
            <InlineRichTextEditor
              value={item}
              aria-label={`${ordered ? "Numbered" : "Bullet"} item ${index + 1}`}
              placeholder="Write an item"
              onChange={(value) => replace(index, value)}
            />
            <div className="flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover/list-item:opacity-100 group-focus-within/list-item:opacity-100 max-md:col-span-2 max-md:justify-self-end max-md:opacity-100">
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
              <BlockCommand
                label="Remove item"
                danger
                onClick={() => remove(index)}
              >
                <Trash2 className="size-4" />
              </BlockCommand>
            </div>
          </div>
        ))}
      </div>
      <button
        type="button"
        className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
        onClick={() => onChange({ items: items === "" ? "\n" : `${items}\n` })}
      >
        <Plus className="size-4" aria-hidden="true" /> Add item below
      </button>
    </div>
  );
}

function sanitizeInlineRichText(value: string): string {
  const template = document.createElement("template");
  template.innerHTML = value;
  const inlineTags = new Set(["B", "EM", "I", "STRONG"]);
  for (const element of [...template.content.querySelectorAll("*")]) {
    if (!inlineTags.has(element.tagName)) {
      element.replaceWith(...element.childNodes);
      continue;
    }
    for (const attribute of [...element.attributes])
      element.removeAttribute(attribute.name);
  }
  return template.innerHTML;
}

function InlineRichTextContent({ value }: { value: string }) {
  return (
    <span
      className="[&_strong]:font-semibold"
      dangerouslySetInnerHTML={{ __html: sanitizeInlineRichText(value) }}
    />
  );
}

function InlineRichTextEditor({
  value,
  placeholder,
  onChange,
  ...rest
}: {
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
  "aria-label": string;
}) {
  const editor = useRef<HTMLDivElement>(null);
  const lastEmitted = useRef("");
  const [showTools, setShowTools] = useState(false);

  useEffect(() => {
    if (editor.current !== null && value !== lastEmitted.current) {
      editor.current.innerHTML = sanitizeInlineRichText(value);
      lastEmitted.current = value;
    }
  }, [value]);

  const emit = () => {
    if (editor.current === null) return;
    const next = sanitizeInlineRichText(editor.current.innerHTML);
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
  const command = (name: "bold" | "italic") => {
    editor.current?.focus();
    document.execCommand(name);
    emit();
    inspectSelection();
  };

  return (
    <div className="relative min-w-0">
      {showTools && (
        <div
          className="absolute bottom-[calc(100%+0.5rem)] left-3 z-20 flex items-center gap-1 rounded-xl border border-default bg-surface p-1.5 shadow-lg"
          role="toolbar"
          aria-label="List item formatting"
          onMouseDown={(event) => event.preventDefault()}
        >
          <RichTextCommand label="Bold" onClick={() => command("bold")}>
            <Bold className="size-4" />
          </RichTextCommand>
          <RichTextCommand label="Italic" onClick={() => command("italic")}>
            <Italic className="size-4" />
          </RichTextCommand>
        </div>
      )}
      <div
        ref={editor}
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        data-placeholder={placeholder}
        className="min-h-11 w-full rounded-lg bg-transparent px-2 py-2.5 text-sm leading-6 text-primary transition-colors selection:bg-accent-soft selection:text-primary empty:before:pointer-events-none empty:before:text-tertiary empty:before:content-[attr(data-placeholder)] hover:bg-raised/50 focus:bg-accent-soft/30 focus:outline-none [&_strong]:font-semibold"
        onInput={emit}
        onMouseUp={inspectSelection}
        onKeyUp={inspectSelection}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.preventDefault();
        }}
        onBlur={() => {
          if (editor.current !== null) {
            const clean = sanitizeInlineRichText(editor.current.innerHTML);
            editor.current.innerHTML = clean;
            lastEmitted.current = clean;
            onChange(clean);
          }
          setShowTools(false);
        }}
        {...rest}
      />
    </div>
  );
}

type ImageBlock = Extract<Block, { kind: "image" }>;

const IMAGE_FRAME = {
  natural: "",
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

const IMAGE_COLUMN_GRID = {
  "33-67": {
    left: "md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]",
    right: "md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]",
  },
  "40-60": {
    left: "md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]",
    right: "md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]",
  },
  "50-50": {
    left: "md:grid-cols-2",
    right: "md:grid-cols-2",
  },
  "60-40": {
    left: "md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]",
    right: "md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]",
  },
  "67-33": {
    left: "md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]",
    right: "md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]",
  },
} as const;

function QuotationBlockImage({
  block,
  onDoubleClick,
}: {
  block: ImageBlock;
  onDoubleClick?: () => void;
}) {
  const aspect = block.aspect ?? "landscape";
  const fit = block.fit ?? "cover";
  const zoom =
    fit === "cover" ? Math.max(100, block.zoom ?? 100) : (block.zoom ?? 100);
  return (
    <div
      className={cx(
        "relative overflow-hidden rounded-xl bg-surface",
        IMAGE_FRAME[aspect],
      )}
    >
      <img
        src={block.src}
        alt={block.caption || "Quotation image"}
        className={cx(
          "transition-transform duration-200",
          aspect === "natural"
            ? "mx-auto max-h-[520px] w-full"
            : "absolute inset-0 size-full",
          fit === "contain" ? "object-contain" : "object-cover",
          IMAGE_BLOCK_ZOOM[zoom as keyof typeof IMAGE_BLOCK_ZOOM],
        )}
        onDoubleClick={onDoubleClick}
      />
    </div>
  );
}

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
    <figure>
      <QuotationBlockImage
        block={block}
        {...(readOnly ? {} : { onDoubleClick: onEdit })}
      />
      {block.caption && (
        <figcaption className="mt-2 px-1 text-xs leading-relaxed opacity-65">
          <RichTextContent value={block.caption} />
        </figcaption>
      )}
    </figure>
  );
  const copy = block.body && (
    <div className="flex flex-col justify-center px-1 py-2">
      <RichTextContent value={block.body} />
    </div>
  );

  if (placement === "full")
    return (
      <div>
        {image}
        {copy && <div className="mt-4">{copy}</div>}
      </div>
    );
  return (
    <div
      className={cx(
        "grid items-center gap-6",
        IMAGE_COLUMN_GRID[block.columnRatio ?? "50-50"][placement],
      )}
    >
      {placement === "left" ? image : copy}
      {placement === "left" ? copy : image}
    </div>
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
      title="Edit content block"
      icon={<ImagePlus className="size-5" />}
      onClose={onClose}
      wide="extra"
      footer={
        <>
          <p className="mr-auto text-xs text-secondary">
            Changes are shown immediately in the quotation.
          </p>
          <Button onClick={onClose}>Done</Button>
        </>
      }
    >
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h3 className="text-base font-semibold text-primary">
          Compose image and text
        </h3>
        <p className="w-full text-sm text-secondary">
          Arrange the block once and see exactly how it will appear in the
          quotation.
        </p>
      </div>
      <section className="border-y border-subtle py-5">
        <h4 className="text-sm font-semibold text-primary">Layout tools</h4>
        <p className="mt-1 text-xs text-secondary">
          Choose how this content block will appear in the quotation.
        </p>
        <div className="mt-5 flex flex-col gap-5">
          <div className="grid items-start gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,5fr)]">
            <div>
              <ImageOptionGroup
                label="Composition"
                visual="composition"
                value={block.placement ?? "full"}
                options={[
                  ["full", "Below image"],
                  ["left", "Image left"],
                  ["right", "Image right"],
                ]}
                onChange={(placement) => onChange({ placement })}
              />
            </div>
            <div>
              <ImageColumnRatioPicker
                value={block.columnRatio ?? "50-50"}
                placement={block.placement ?? "full"}
                onChange={(columnRatio) => onChange({ columnRatio })}
              />
            </div>
          </div>
          <div className="grid items-start gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)_minmax(0,3fr)]">
            <div>
              <ImageOptionGroup
                label="Image frame"
                visual="frame"
                value={block.aspect ?? "landscape"}
                options={[
                  ["natural", "Natural"],
                  ["landscape", "Wide"],
                  ["square", "Square"],
                ]}
                onChange={(aspect) => onChange({ aspect })}
              />
            </div>
            <div>
              <ImageOptionGroup
                label="Fit"
                visual="fit"
                value={block.fit ?? "cover"}
                options={[
                  ["cover", "Fill frame"],
                  ["contain", "Whole image"],
                ]}
                onChange={(fit) =>
                  onChange({
                    fit,
                    zoom:
                      fit === "cover" && (block.zoom ?? 100) < 100
                        ? 100
                        : (block.zoom ?? 100),
                  })
                }
              />
            </div>
            <div>
              <ImageZoomControl
                value={
                  block.fit === "cover"
                    ? (Math.max(100, block.zoom ?? 100) as Exclude<
                        ImageBlock["zoom"],
                        undefined
                      >)
                    : (block.zoom ?? 100)
                }
                minimum={block.fit === "cover" ? 100 : 50}
                onChange={(zoom) => onChange({ zoom })}
              />
            </div>
          </div>
        </div>
      </section>
      <div
        className={cx(
          "grid items-start gap-6",
          (block.placement ?? "full") === "full"
            ? "md:grid-cols-2"
            : IMAGE_COLUMN_GRID[block.columnRatio ?? "50-50"][
                block.placement === "right" ? "right" : "left"
              ],
        )}
      >
        <section className="min-w-0">
          <div className="mb-2 flex min-h-10 items-center justify-between gap-3">
            <h4 className="text-sm font-semibold text-primary">Image</h4>
            <button
              type="button"
              className="inline-flex min-h-9 items-center gap-2 rounded-lg border border-default bg-surface px-3 text-xs font-semibold text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
              onClick={onReplace}
            >
              <Upload className="size-4" aria-hidden="true" /> Replace
            </button>
          </div>
          <div className="rounded-2xl border border-default bg-surface p-3 shadow-sm">
            <QuotationBlockImage block={block} />
          </div>
        </section>
        <section className="min-w-0">
          <RichTextEditor
            value={block.body ?? ""}
            placeholder="Explain the product, project, or result shown in the image."
            onChange={(body) => onChange({ body })}
          />
          <div className="mt-4">
            <RichTextEditor
              value={block.caption}
              label="Caption"
              placeholder="Optional short caption"
              onChange={(caption) => onChange({ caption })}
            />
          </div>
        </section>
      </div>
    </Modal>
  );
}

const RICH_TEXT_TAGS = new Set([
  "B",
  "BR",
  "EM",
  "H1",
  "H2",
  "H3",
  "I",
  "LI",
  "OL",
  "P",
  "STRONG",
  "UL",
]);

function sanitizeRichText(value: string): string {
  const hadMarkup = value.includes("<");
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
  const sanitized = template.innerHTML;
  return hadMarkup ? sanitized : sanitized.replaceAll("\n", "<br>");
}

function RichTextContent({ value }: { value: string }) {
  return (
    <div
      className="text-sm leading-relaxed opacity-90 [&_h1]:mb-2 [&_h1]:text-2xl [&_h1]:font-semibold [&_h2]:mb-2 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:mb-2 [&_h3]:text-lg [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:space-y-1 [&_ol]:pl-6 [&_p+p]:mt-3 [&_strong]:font-semibold [&_ul]:list-disc [&_ul]:space-y-1 [&_ul]:pl-6"
      dangerouslySetInnerHTML={{ __html: sanitizeRichText(value) }}
    />
  );
}

function RichTextEditor({
  value,
  label = "Supporting text",
  placeholder,
  onChange,
}: {
  value: string;
  label?: string;
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
    <div>
      <div className="mb-2 flex items-center justify-between gap-3">
        <p className="text-sm font-semibold text-primary">{label}</p>
        <button
          type="button"
          className={cx(
            "inline-flex min-h-9 items-center gap-2 rounded-lg border px-3 text-xs font-semibold transition-colors",
            showTools
              ? "border-accent bg-accent-soft text-accent"
              : "border-default bg-surface text-secondary hover:border-accent hover:bg-accent-soft hover:text-accent",
          )}
          aria-expanded={showTools}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => setShowTools((current) => !current)}
        >
          <Type className="size-4" aria-hidden="true" /> Text tools
        </button>
      </div>
      <div className="relative">
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
              label="Heading 1"
              onClick={() => command("formatBlock", "h1")}
            >
              <Heading1 className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label="Heading 2"
              onClick={() => command("formatBlock", "h2")}
            >
              <Heading2 className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label="Heading 3"
              onClick={() => command("formatBlock", "h3")}
            >
              <Heading3 className="size-4" />
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
          aria-label={label}
          data-placeholder={placeholder}
          className="min-h-32 w-full overflow-y-auto rounded-lg bg-transparent px-2 py-3 text-sm font-normal leading-relaxed text-primary transition-colors selection:bg-accent-soft selection:text-primary empty:before:pointer-events-none empty:before:text-tertiary empty:before:content-[attr(data-placeholder)] hover:bg-raised/50 focus:bg-accent-soft/30 focus:outline-none [&_h1]:my-2 [&_h1]:text-2xl [&_h1]:font-semibold [&_h2]:my-2 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:my-2 [&_h3]:text-lg [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:pl-6 [&_strong]:font-semibold [&_ul]:list-disc [&_ul]:pl-6"
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
      className="group relative grid size-9 place-items-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
      aria-label={label}
      onClick={onClick}
    >
      {children}
      <span
        role="tooltip"
        className="pointer-events-none absolute bottom-[calc(100%+.5rem)] left-1/2 z-20 -translate-x-1/2 whitespace-nowrap rounded-lg bg-primary px-2.5 py-1.5 text-xs font-medium text-surface opacity-0 shadow-md transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100"
      >
        {label}
      </span>
    </button>
  );
}

const IMAGE_COLUMN_RATIOS = [
  ["33-67", 33, 67],
  ["40-60", 40, 60],
  ["50-50", 50, 50],
  ["60-40", 60, 40],
  ["67-33", 67, 33],
] as const;
const IMAGE_RATIO_WIDTH = {
  33: "w-1/3",
  40: "w-2/5",
  50: "w-1/2",
  60: "w-3/5",
  67: "w-2/3",
} as const;

function ImageColumnRatioPicker({
  value,
  placement,
  onChange,
}: {
  value: NonNullable<ImageBlock["columnRatio"]>;
  placement: NonNullable<ImageBlock["placement"]>;
  onChange: (value: NonNullable<ImageBlock["columnRatio"]>) => void;
}) {
  const disabled = placement === "full";
  return (
    <fieldset className="min-w-0" disabled={disabled}>
      <legend className="sr-only">Column width</legend>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-semibold uppercase tracking-wide text-tertiary">
          Column width
        </p>
        {disabled && (
          <span className="text-[11px] text-tertiary">Side-by-side only</span>
        )}
      </div>
      <div className="grid grid-cols-5 gap-1.5">
        {IMAGE_COLUMN_RATIOS.map(([id, image, text]) => {
          const selected = value === id;
          const imageFirst = placement !== "right";
          return (
            <button
              key={id}
              type="button"
              aria-label={`Image ${image}%, text ${text}%`}
              aria-pressed={selected}
              className={cx(
                "group h-20 rounded-xl border bg-surface p-2 transition-colors hover:border-accent hover:bg-accent-soft disabled:cursor-not-allowed disabled:opacity-40",
                selected
                  ? "border-accent ring-1 ring-inset ring-accent/15"
                  : "border-default",
              )}
              onClick={() => onChange(id)}
            >
              <span className="mx-auto flex h-10 max-w-24 gap-1 overflow-hidden rounded-md bg-raised p-1.5">
                <span
                  className={cx(
                    "rounded-sm bg-accent/25",
                    imageFirst ? "order-1" : "order-2",
                    IMAGE_RATIO_WIDTH[image],
                  )}
                />
                <span
                  className={cx(
                    "rounded-sm bg-surface shadow-sm",
                    imageFirst ? "order-2" : "order-1",
                    IMAGE_RATIO_WIDTH[text],
                  )}
                />
              </span>
              <span
                className={cx(
                  "mt-1 block text-center text-[10px] font-semibold tabular-nums",
                  selected ? "text-accent" : "text-tertiary",
                )}
              >
                {image}:{text}
              </span>
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

function ImageOptionGroup<T extends string | number>({
  label,
  visual,
  value,
  options,
  onChange,
}: {
  label: string;
  visual?: "composition" | "frame" | "fit";
  value: T;
  options: Array<readonly [T, string]>;
  onChange: (value: T) => void;
}) {
  return (
    <fieldset className="min-w-0">
      <legend className="sr-only">{label}</legend>
      <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">
        {label}
      </p>
      <div
        className={cx(
          "grid",
          visual
            ? "gap-2"
            : "gap-1 rounded-xl border border-default bg-raised/60 p-1 shadow-sm",
          options.length === 3 ? "grid-cols-3" : "grid-cols-2",
        )}
      >
        {options.map(([id, name]) => (
          <button
            key={id}
            type="button"
            aria-label={name}
            aria-pressed={value === id}
            className={cx(
              "group relative whitespace-nowrap border text-center text-sm font-medium transition-all hover:border-accent hover:bg-accent-soft hover:text-accent",
              visual
                ? "h-20 rounded-xl bg-transparent p-2"
                : "min-h-11 rounded-lg px-3",
              value === id
                ? visual
                  ? "border-accent bg-transparent text-accent ring-1 ring-inset ring-accent/15"
                  : "border-accent/30 bg-accent-soft font-semibold text-accent shadow-sm ring-1 ring-inset ring-accent/15"
                : visual
                  ? "border-transparent text-secondary"
                  : "border-transparent bg-transparent text-secondary",
            )}
            onClick={() => onChange(id)}
          >
            {visual && <ImageOptionPreview kind={visual} option={String(id)} />}
            {!visual && <span>{name}</span>}
            {visual && (
              <span className="pointer-events-none absolute bottom-[calc(100%+.5rem)] left-1/2 z-20 -translate-x-1/2 whitespace-nowrap rounded-lg bg-primary px-2.5 py-1.5 text-xs font-medium text-surface opacity-0 shadow-md transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
                {name}
              </span>
            )}
          </button>
        ))}
      </div>
    </fieldset>
  );
}

function ImageOptionPreview({
  kind,
  option,
}: {
  kind: "composition" | "frame" | "fit";
  option: string;
}) {
  if (kind === "composition") {
    if (option === "full")
      return (
        <span className="mx-auto flex h-10 max-w-24 flex-col gap-1 rounded-md bg-raised p-1.5">
          <span className="h-4 rounded-sm bg-accent/25" />
          <span className="h-1 rounded-full bg-tertiary/30" />
          <span className="h-1 w-3/4 rounded-full bg-tertiary/20" />
        </span>
      );
    const imageFirst = option === "left";
    return (
      <span className="mx-auto flex h-10 max-w-24 gap-1 rounded-md bg-raised p-1.5">
        <span
          className={cx(
            "w-2/5 rounded-sm bg-accent/25",
            imageFirst ? "order-1" : "order-2",
          )}
        />
        <span
          className={cx(
            "flex w-3/5 flex-col justify-center gap-1",
            imageFirst ? "order-2" : "order-1",
          )}
        >
          <span className="h-1 rounded-full bg-tertiary/30" />
          <span className="h-1 w-4/5 rounded-full bg-tertiary/20" />
          <span className="h-1 w-2/3 rounded-full bg-tertiary/20" />
        </span>
      </span>
    );
  }
  if (kind === "frame") {
    return (
      <span className="mx-auto flex h-10 max-w-24 items-center justify-center rounded-md bg-raised p-1.5">
        <span
          className={cx(
            "border border-accent/30 bg-accent/25",
            option === "natural" && "h-7 w-5 rounded-sm",
            option === "landscape" && "h-5 w-full rounded-sm",
            option === "square" && "size-7 rounded-sm",
          )}
        />
      </span>
    );
  }
  return (
    <span className="mx-auto flex h-10 max-w-24 items-center justify-center overflow-hidden rounded-md border border-subtle bg-surface p-1">
      <span
        className={cx(
          "bg-accent/25",
          option === "cover" ? "size-full rounded-sm" : "h-6 w-3/5 rounded-sm",
        )}
      />
    </span>
  );
}

const IMAGE_ZOOM_STEPS = [50, 75, 100, 125, 150, 175, 200] as const;

function ImageZoomControl({
  value,
  minimum = 50,
  onChange,
}: {
  value: ImageBlock["zoom"] extends infer Z ? Exclude<Z, undefined> : never;
  minimum?: 50 | 100;
  onChange: (value: Exclude<ImageBlock["zoom"], undefined>) => void;
}) {
  const index = IMAGE_ZOOM_STEPS.indexOf(value);
  const minimumIndex = IMAGE_ZOOM_STEPS.indexOf(minimum);
  const previous =
    IMAGE_ZOOM_STEPS[Math.max(minimumIndex, index - 1)] ?? minimum;
  const next =
    IMAGE_ZOOM_STEPS[Math.min(IMAGE_ZOOM_STEPS.length - 1, index + 1)] ?? 200;
  return (
    <section className="min-w-0">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h4 className="text-xs font-semibold uppercase tracking-wide text-tertiary">
            Zoom
          </h4>
        </div>
        <button
          type="button"
          className="rounded-md px-2 py-1 text-xs font-semibold text-secondary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-default disabled:opacity-35"
          disabled={value === 100}
          onClick={() => onChange(100)}
        >
          Reset
        </button>
      </div>
      <div className="mt-2 grid grid-cols-[2.5rem_minmax(0,1fr)_2.5rem] items-center gap-1 rounded-xl border border-default bg-raised/60 p-1 shadow-sm">
        <button
          type="button"
          className="grid size-10 place-items-center rounded-lg text-primary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-not-allowed disabled:opacity-35"
          aria-label="Zoom out"
          disabled={index <= minimumIndex}
          onClick={() => onChange(previous)}
        >
          <Minus className="size-4" aria-hidden="true" />
        </button>
        <strong className="text-center text-sm font-semibold tabular-nums text-primary">
          {value}%
        </strong>
        <button
          type="button"
          className="grid size-10 place-items-center rounded-lg text-primary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-not-allowed disabled:opacity-35"
          aria-label="Zoom in"
          disabled={index === IMAGE_ZOOM_STEPS.length - 1}
          onClick={() => onChange(next)}
        >
          <Plus className="size-4" aria-hidden="true" />
        </button>
      </div>
      <div className="mt-2 flex justify-between px-1 text-[11px] text-tertiary">
        <span>{minimum}%</span>
        <span>100%</span>
        <span>200%</span>
      </div>
    </section>
  );
}

type GeneralTable = Extract<Block, { kind: "table" }>;

function generalTableHasContent(block: GeneralTable): boolean {
  return block.rows.some((row) =>
    block.columns.some((column) => (row.cells[column.id] ?? "").trim() !== ""),
  );
}

function GeneralTableBlock({
  block,
  readOnly,
  onChange,
}: {
  block: GeneralTable;
  readOnly: boolean;
  onChange: (patch: Partial<GeneralTable>) => void;
}) {
  const setColumnCount = (count: number) => {
    const columns = block.columns.slice(0, count);
    while (columns.length < count) {
      const number = columns.length + 1;
      columns.push({ id: crypto.randomUUID(), label: `Column ${number}` });
    }
    onChange({
      columns,
      rows: block.rows.map((row) => ({
        ...row,
        cells: Object.fromEntries(
          columns.map((column) => [column.id, row.cells[column.id] ?? ""]),
        ),
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

  if (readOnly && !generalTableHasContent(block)) return null;

  if (readOnly) {
    return (
      <div className="overflow-x-auto rounded-xl border border-default">
        <table className="w-full border-collapse text-left text-sm">
          <thead className="bg-[var(--quote-table-header)]">
            <tr>
              {block.columns.map((column) => (
                <th key={column.id} className="px-4 py-3 font-semibold">
                  <InlineRichTextContent value={column.label} />
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row) => (
              <tr key={row.id} className="border-t border-default">
                {block.columns.map((column) => (
                  <td key={column.id} className="px-4 py-3 align-top">
                    <RichTextContent value={row.cells[column.id] ?? ""} />
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
          <h3 className="text-sm font-semibold text-primary">
            Information table
          </h3>
          <p className="mt-1 text-xs text-secondary">
            Rename columns, then add as many rows or columns as the document
            needs.
          </p>
        </div>
        <label className="grid min-w-40 gap-1 text-xs font-semibold text-secondary">
          Columns
          <Select
            fullWidth
            value={String(block.columns.length)}
            aria-label="Number of table columns"
            onChange={(event) => setColumnCount(Number(event.target.value))}
          >
            {[1, 2, 3, 4, 5, 6].map((count) => (
              <option key={count} value={count}>
                {count} {count === 1 ? "column" : "columns"}
              </option>
            ))}
          </Select>
        </label>
      </div>
      <div className="overflow-x-auto rounded-xl border border-default">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-raised/50">
            <tr>
              {block.columns.map((column, columnIndex) => (
                <th
                  key={column.id}
                  className="group/table-column min-w-44 border-r border-default p-2 last:border-r-0"
                >
                  <div className="flex items-center gap-2">
                    <InlineRichTextEditor
                      value={column.label}
                      aria-label={`Column ${columnIndex + 1} name`}
                      placeholder={`Column ${columnIndex + 1}`}
                      onChange={(label) =>
                        onChange({
                          columns: block.columns.map((item) =>
                            item.id === column.id ? { ...item, label } : item,
                          ),
                        })
                      }
                    />
                    <button
                      type="button"
                      className="grid size-9 shrink-0 place-items-center rounded-lg text-secondary opacity-0 transition-[color,background-color,opacity] hover:bg-danger-tint hover:text-danger focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 group-hover/table-column:opacity-100 group-focus-within/table-column:opacity-100 disabled:cursor-not-allowed disabled:opacity-35 max-md:opacity-100"
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
              <tr
                key={row.id}
                className="group/table-row border-t border-default"
              >
                {block.columns.map((column) => (
                  <td
                    key={column.id}
                    className="border-r border-default p-2 last:border-r-0"
                  >
                    <InlineRichTextEditor
                      value={row.cells[column.id] ?? ""}
                      aria-label={`${column.label || "Column"}, row ${rowIndex + 1}`}
                      placeholder="Enter value"
                      onChange={(value) =>
                        onChange({
                          rows: block.rows.map((item) =>
                            item.id === row.id
                              ? {
                                  ...item,
                                  cells: {
                                    ...item.cells,
                                    [column.id]: value,
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
                    className="grid size-9 place-items-center rounded-lg text-secondary opacity-0 transition-[color,background-color,opacity] hover:bg-danger-tint hover:text-danger focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 group-hover/table-row:opacity-100 group-focus-within/table-row:opacity-100 max-md:opacity-100"
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
  "heading" | "paragraph" | "quote" | "list" | "divider" | "pricing" | "table";

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
  const [query, setQuery] = useState("");
  const add = (kind: InsertKind, label: string, ordered = false) => {
    onAdd(index, kind, ordered);
    setOpen(false);
    setQuery("");
  };
  const options: Array<{
    label: string;
    help: string;
    category: "Text" | "Media" | "Tables" | "Layout";
    Icon: typeof AlignLeft;
    action: () => void;
  }> = [
    {
      label: "Heading",
      help: "Choose H1, H2, or H3",
      category: "Text",
      Icon: Type,
      action: () => add("heading", "Heading"),
    },
    {
      label: "Paragraph",
      help: "Add explanatory text",
      category: "Text",
      Icon: AlignLeft,
      action: () => add("paragraph", "Paragraph"),
    },
    {
      label: "Quote",
      help: "Highlight a statement",
      category: "Text",
      Icon: Quote,
      action: () => add("quote", "Quote"),
    },
    {
      label: "Bullet list",
      help: "List key points",
      category: "Text",
      Icon: List,
      action: () => add("list", "Bullet list"),
    },
    {
      label: "Numbered list",
      help: "Show ordered steps",
      category: "Text",
      Icon: ListOrdered,
      action: () => add("list", "Numbered list", true),
    },
    {
      label: "Image",
      help: "Upload and arrange a visual",
      category: "Media",
      Icon: ImagePlus,
      action: () => {
        onImage(index);
        setOpen(false);
        setQuery("");
      },
    },
    {
      label: "Pricing table",
      help: "Group products and services",
      category: "Tables",
      Icon: Table2,
      action: () => add("pricing", "Pricing table"),
    },
    {
      label: "Table",
      help: "Create flexible rows and columns",
      category: "Tables",
      Icon: Rows3,
      action: () => add("table", "Table"),
    },
    {
      label: "Divider",
      help: "Separate document sections",
      category: "Layout",
      Icon: Minus,
      action: () => add("divider", "Divider"),
    },
  ];
  const categories = ["Text", "Media", "Tables", "Layout"] as const;
  const normalizedQuery = query.trim().toLowerCase();
  const visibleOptions = options.filter((option) =>
    `${option.label} ${option.help} ${option.category}`
      .toLowerCase()
      .includes(normalizedQuery),
  );
  return (
    <div
      className="relative flex flex-col items-center py-2"
      aria-label="Add quotation content"
    >
      <div className="flex w-full items-center gap-3">
        <span
          className="h-px flex-1 bg-[var(--quote-table-header)]"
          aria-hidden="true"
        />
        <button
          type="button"
          className="group inline-flex min-h-9 items-center gap-2 rounded-full px-3 text-xs font-semibold text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
          aria-expanded={open}
          aria-label="Add content below"
          onClick={() => setOpen((value) => !value)}
        >
          <span className="grid size-6 place-items-center rounded-full bg-accent-soft text-accent transition-colors group-hover:bg-accent group-hover:text-on-accent">
            <Plus className="size-3.5" aria-hidden="true" />
          </span>
          Add content
        </button>
        <span
          className="h-px flex-1 bg-[var(--quote-table-header)]"
          aria-hidden="true"
        />
      </div>
      {open && (
        <div className="mt-2 w-full max-w-2xl rounded-2xl border border-default bg-surface shadow-xl">
          <div className="p-5 pb-4">
            <div className="flex items-start justify-between gap-3">
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
            <label className="mt-4 flex min-h-11 items-center gap-3 rounded-xl border border-default bg-surface px-3 text-secondary transition-colors focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/10">
              <Search className="size-4 shrink-0" aria-hidden="true" />
              <input
                autoFocus
                className="min-w-0 flex-1 appearance-none !border-0 bg-transparent !p-0 text-sm text-primary !shadow-none !outline-none !ring-0 placeholder:text-tertiary focus:!border-0 focus:!outline-none focus:!ring-0"
                value={query}
                placeholder="Search blocks..."
                aria-label="Search quotation blocks"
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") setOpen(false);
                }}
              />
            </label>
          </div>
          <div className="max-h-[min(65vh,40rem)] overflow-y-auto border-t border-default px-5">
            {(normalizedQuery === ""
              ? categories
              : (["Search results"] as const)
            ).map((section, sectionIndex) => {
              const sectionOptions =
                section === "Search results"
                  ? visibleOptions
                  : visibleOptions.filter(
                      (option) => option.category === section,
                    );
              if (sectionOptions.length === 0) return null;
              return (
                <section
                  key={section}
                  className={cx(
                    "py-4",
                    sectionIndex > 0 && "border-t border-default",
                  )}
                  aria-labelledby={`quote-blocks-${section.toLowerCase().replace(" ", "-")}`}
                >
                  <h4
                    id={`quote-blocks-${section.toLowerCase().replace(" ", "-")}`}
                    className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary"
                  >
                    {section}
                  </h4>
                  <div className="grid gap-1 sm:grid-cols-2">
                    {sectionOptions.map(({ label, help, Icon, action }) => (
                      <AddButton
                        key={label}
                        label={label}
                        help={help}
                        Icon={Icon}
                        onClick={action}
                      />
                    ))}
                  </div>
                </section>
              );
            })}
          </div>
          {visibleOptions.length === 0 && (
            <div className="border-t border-default px-5 py-8 text-center">
              <p className="text-sm font-semibold text-primary">
                No matching blocks
              </p>
              <p className="mt-1 text-xs text-secondary">Try another name.</p>
            </div>
          )}
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
      className="flex min-h-16 items-center gap-3 rounded-xl px-3 py-2.5 text-left text-primary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 disabled:cursor-not-allowed disabled:opacity-45"
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
  accent = false,
}: {
  label: string;
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
  accent?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex size-10 shrink-0 items-center justify-center rounded-lg border border-transparent text-xs font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-45",
        danger
          ? "text-danger hover:border-danger/20 hover:bg-danger-tint"
          : accent
            ? "bg-accent-soft text-accent hover:border-accent/25 hover:bg-accent hover:text-on-accent"
            : "text-secondary hover:border-accent/25 hover:bg-accent-soft hover:text-accent",
      )}
      onClick={onClick}
    >
      {children}
      <span className="sr-only">{label}</span>
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
      wide="extra"
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
      <div className="space-y-7">
        <section className="flex flex-wrap items-center gap-5 rounded-2xl border border-default bg-raised/35 p-4">
          <div className="min-w-52 flex-1">
            <h3 className="text-base font-semibold text-primary">Brand mark</h3>
            <p className="mt-1 text-sm leading-relaxed text-secondary">
              Shown at the top of the customer quotation.
            </p>
          </div>
          <button
            type="button"
            className="flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-default bg-surface p-3 text-sm font-semibold text-secondary shadow-sm transition-all hover:border-accent hover:bg-accent-soft hover:text-accent hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
            onClick={() => logoInput.current?.click()}
          >
            {design.logo ? (
              <img
                src={design.logo}
                alt="Quote logo"
                className="max-h-20 max-w-full object-contain"
              />
            ) : (
              <span className="flex flex-col items-center gap-3 text-center">
                <span className="grid size-10 place-items-center rounded-xl bg-accent-soft text-accent">
                  <Upload className="size-5" />
                </span>
                <span>
                  <strong className="sr-only">Upload your logo</strong>
                </span>
              </span>
            )}
          </button>
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              className="inline-flex min-h-9 items-center gap-2 rounded-lg px-3 text-sm font-semibold text-accent transition-colors hover:bg-accent-soft hover:text-accent-hover disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-50"
              onClick={() => logoInput.current?.click()}
            >
              <Upload className="size-4" />
              {design.logo ? "Replace" : "Choose file"}
            </button>
            <button
              type="button"
              disabled={!design.logo}
              className="min-h-9 rounded-lg px-3 text-sm font-semibold text-secondary transition-colors hover:bg-danger-tint hover:text-danger disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-40"
              onClick={() => onChange((current) => ({ ...current, logo: "" }))}
            >
              Remove
            </button>
          </div>
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
        <div className="min-w-0 space-y-7">
          <section>
            <div>
              <h3 className="text-base font-semibold text-primary">
                Header arrangement
              </h3>
              <p className="mt-1 text-sm text-secondary">
                Choose which side carries your company identity.
              </p>
            </div>
            <div className="mt-4 grid gap-4 sm:grid-cols-2">
              {(["left", "right"] as const).map((alignment) => (
                <button
                  key={alignment}
                  type="button"
                  aria-pressed={design.headerAlignment === alignment}
                  className={cx(
                    "group relative min-h-40 overflow-hidden rounded-2xl border p-3 text-left transition-all hover:-translate-y-0.5 hover:border-accent hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                    design.headerAlignment === alignment
                      ? "border-accent bg-accent-soft/30 shadow-sm"
                      : "border-default bg-surface shadow-sm",
                  )}
                  onClick={() =>
                    onChange((current) => ({
                      ...current,
                      headerAlignment: alignment,
                    }))
                  }
                >
                  <span
                    className={cx(
                      "flex h-20 items-center justify-between gap-5 rounded-xl bg-raised px-5",
                      alignment === "right" && "flex-row-reverse",
                    )}
                    aria-hidden="true"
                  >
                    <span className="flex items-center gap-2.5">
                      <span className="size-9 rounded-lg border border-accent/20 bg-accent-soft" />
                      <span className="space-y-1.5">
                        <span className="block h-2 w-16 rounded-full bg-primary/20" />
                        <span className="block h-1.5 w-11 rounded-full bg-primary/10" />
                      </span>
                    </span>
                    <span className="space-y-1.5">
                      <span className="block h-1.5 w-10 rounded-full bg-primary/15" />
                      <span className="block h-1.5 w-14 rounded-full bg-accent/70" />
                    </span>
                  </span>
                  <span className="flex items-start justify-between gap-4 px-1 pb-1 pt-3">
                    <span>
                      <strong className="block text-sm font-semibold text-primary">
                        Logo {alignment}
                      </strong>
                      <small className="mt-1 block text-xs font-normal leading-relaxed text-secondary">
                        Company identity on the {alignment}; quote details opposite.
                      </small>
                    </span>
                    <span
                      className={cx(
                        "mt-0.5 grid size-6 shrink-0 place-items-center rounded-full border transition-colors",
                        design.headerAlignment === alignment
                          ? "border-accent bg-accent text-white"
                          : "border-default bg-surface group-hover:border-accent",
                      )}
                    >
                      {design.headerAlignment === alignment && (
                        <Check className="size-3.5" />
                      )}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          </section>
          <section className="border-t border-subtle pt-6">
            <div className="flex items-center justify-between gap-3">
              <div>
                <h3 className="text-base font-semibold text-primary">
                  Document palette
                </h3>
                <p className="mt-1 text-sm text-secondary">
                  Control the customer-facing page and pricing table colours.
                </p>
              </div>
              <button
                type="button"
                className="inline-flex min-h-10 items-center gap-2 rounded-xl border border-default bg-surface px-3.5 text-sm font-semibold text-secondary shadow-sm transition-all hover:border-accent hover:bg-accent-soft hover:text-accent"
                onClick={() =>
                  onChange((current) => ({
                    ...current,
                    colors: DEFAULT_COLORS,
                  }))
                }
              >
                <RotateCcw className="size-4" aria-hidden="true" />
                Reset
              </button>
            </div>
            <div className="mt-5 grid gap-6 xl:grid-cols-2">
              <div>
                <div className="mb-4">
                  <h4 className="text-sm font-semibold text-primary">
                    Document
                  </h4>
                  <p className="mt-0.5 text-xs text-secondary">
                    Brand, page, header, and copy.
                  </p>
                </div>
                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-1">
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
                    label="Header"
                    value={design.colors.headerBackground}
                    onChange={(value) => setColor("headerBackground", value)}
                  />
                  <ColorField
                    label="Text"
                    value={design.colors.text}
                    onChange={(value) => setColor("text", value)}
                  />
                </div>
              </div>
              <div className="xl:border-l xl:border-subtle xl:pl-6">
                <div className="mb-4">
                  <h4 className="text-sm font-semibold text-primary">
                    Pricing tables
                  </h4>
                  <p className="mt-0.5 text-xs text-secondary">
                    Keep headings and rows easy to scan.
                  </p>
                </div>
                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-1">
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
              </div>
            </div>
          </section>
          <section className="border-t border-subtle pt-7">
            <div>
              <h3 className="text-base font-semibold text-primary">
                Typography
              </h3>
              <p className="mt-1 text-sm text-secondary">
                Choose the reading style that best matches your brand.
              </p>
            </div>
            <div className="mt-5 grid gap-4 sm:grid-cols-3">
              {themeChoices.map((theme) => (
                <button
                  key={theme.id}
                  type="button"
                  aria-pressed={design.theme === theme.id}
                  className={cx(
                    "group relative min-h-52 overflow-hidden rounded-2xl border p-3 text-left shadow-sm transition-all hover:-translate-y-0.5 hover:border-accent hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                    design.theme === theme.id
                      ? "border-accent bg-accent-soft/30"
                      : "border-default bg-surface",
                  )}
                  onClick={() =>
                    onChange((current) => ({ ...current, theme: theme.id }))
                  }
                >
                  <span
                    className={cx(
                      "block h-28 rounded-xl border border-subtle bg-raised px-4 py-4",
                    )}
                    aria-hidden="true"
                  >
                    <span
                      className={cx(
                        "block text-xl leading-none text-primary",
                        theme.id === "modern" && "font-semibold tracking-tight",
                      theme.id === "editorial" && "font-editorial",
                        theme.id === "minimal" &&
                          "font-light uppercase tracking-[0.14em]",
                      )}
                    >
                      Proposal
                    </span>
                    <span
                      className={cx(
                        "mt-4 block h-1.5 rounded-full bg-primary/20",
                        theme.id === "modern" && "w-4/5",
                        theme.id === "editorial" && "w-full",
                        theme.id === "minimal" && "w-3/5",
                      )}
                    />
                    <span className="mt-2 block h-1.5 w-2/3 rounded-full bg-primary/10" />
                  </span>
                  <span className="flex items-start justify-between gap-3 px-1 pb-1 pt-4">
                    <span>
                      <strong className="block text-sm font-semibold text-primary">
                        {theme.name}
                      </strong>
                      <small className="mt-1 block text-xs leading-relaxed text-secondary">
                        {theme.help}
                      </small>
                    </span>
                    <span
                      className={cx(
                        "grid size-6 shrink-0 place-items-center rounded-full border transition-colors",
                        design.theme === theme.id
                          ? "border-accent bg-accent text-on-accent"
                          : "border-default bg-surface group-hover:border-accent",
                      )}
                    >
                      {design.theme === theme.id && (
                        <Check className="size-3.5" aria-hidden="true" />
                      )}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          </section>
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
          Pricing table totals
        </h3>
        <p className="mt-1 text-sm text-secondary">
          Choose how the amount summary appears beneath each pricing table.
          Every table keeps its own subtotal.
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
  const fieldId = `quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`;
  return (
    <div className="flex min-h-16 items-center gap-3 rounded-xl border border-default bg-surface p-2.5 shadow-sm transition-all hover:border-accent hover:shadow-md focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/10">
      <label
        className="relative grid size-11 shrink-0 cursor-pointer place-items-center overflow-hidden rounded-lg border border-default bg-surface shadow-sm"
        htmlFor={`${fieldId}-picker`}
        title={`Choose ${label.toLowerCase()} colour`}
      >
        <input
          id={`${fieldId}-picker`}
          type="color"
          value={valid ? value : DEFAULT_COLORS.accent}
          aria-label={`Choose ${label.toLowerCase()} colour`}
          className="size-8 cursor-pointer rounded-md border-0 bg-transparent p-0 [&::-moz-color-swatch]:rounded-md [&::-moz-color-swatch]:border [&::-moz-color-swatch]:border-black/10 [&::-webkit-color-swatch-wrapper]:p-0 [&::-webkit-color-swatch]:rounded-md [&::-webkit-color-swatch]:border [&::-webkit-color-swatch]:border-black/10"
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
      <div className="min-w-0 flex-1">
        <label
          className="block text-xs font-semibold text-secondary"
          htmlFor={fieldId}
        >
          {label}
        </label>
        <input
          id={fieldId}
          value={value.toUpperCase()}
          aria-label={`${label} hex colour`}
          className="mt-0.5 h-6 min-w-0 w-full border-0 bg-transparent p-0 font-mono text-sm font-semibold uppercase text-primary shadow-none outline-none ring-0 focus:border-0 focus:outline-none focus:ring-0"
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
