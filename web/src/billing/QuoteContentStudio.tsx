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
  Check,
  Copy,
  Heading2,
  ImagePlus,
  List,
  ListOrdered,
  Minus,
  Palette,
  Plus,
  Quote,
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
} from "./quoteTableOptions";

type Theme = "modern" | "editorial" | "minimal";
type Block =
  | { id: string; kind: "text"; heading: string; body: string }
  | { id: string; kind: "heading"; level: 1 | 2 | 3; text: string }
  | { id: string; kind: "paragraph"; text: string }
  | { id: string; kind: "quote"; text: string; attribution: string }
  | { id: string; kind: "list"; ordered: boolean; items: string }
  | { id: string; kind: "divider" }
  | { id: string; kind: "image"; src: string; caption: string }
  | { id: string; kind: "pricing" };
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
    pricingTable: ReactNode;
    onColumnsChange?: (columns: QuoteColumns) => void;
  }
>(function QuoteContentStudio(
  { quoteId, readOnly, preview = false, pricingTable, onColumnsChange },
  ref,
) {
  const storageKey = `alo:quote-design:${quoteId}`;
  const [design, setDesign] = useState<Design>(EMPTY);
  const [ready, setReady] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [customize, setCustomize] = useState(false);
  const [tableSettings, setTableSettings] = useState(false);
  const root = useRef<HTMLElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
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
    kind: "heading" | "paragraph" | "quote" | "list" | "divider" | "pricing",
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
    if (
      kind === "pricing" &&
      !design.blocks.some((block) => block.kind === "pricing")
    )
      insertBlock(index, { id, kind });
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
    setDesign((current) => ({
      ...current,
      blocks: current.blocks.filter((block) => block.id !== id),
    }));
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
                        <span className="text-xs font-semibold uppercase tracking-wide text-secondary">
                          {blockName(block)}
                        </span>
                        <div className="flex flex-wrap items-center gap-1">
                          {block.kind === "pricing" && (
                            <BlockCommand
                              label="Table settings"
                              onClick={() => setTableSettings(true)}
                            >
                              <Palette className="size-4" />
                            </BlockCommand>
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
                            lineContent: design.lineContent,
                            updateLineContent,
                          }}
                        >
                          {pricingTable}
                        </QuoteTableOptionsProvider>
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
                          <textarea
                            className="min-h-28 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm leading-relaxed text-primary placeholder:text-tertiary focus:border-accent focus:outline-none"
                            value={block.items}
                            placeholder="One item per line…"
                            aria-label={
                              block.ordered ? "Numbered list" : "Bullet list"
                            }
                            onChange={(event) =>
                              update(block.id, { items: event.target.value })
                            }
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
                        <>
                          <img
                            src={block.src}
                            alt={block.caption || "Quote image"}
                            className="max-h-[420px] w-full rounded-lg object-cover"
                          />
                          {readOnly ? (
                            <p className="mt-3 text-sm opacity-80">
                              {block.caption}
                            </p>
                          ) : (
                            <Input
                              className="mt-3"
                              value={block.caption}
                              placeholder="Describe this image"
                              aria-label="Image caption"
                              onChange={(event) =>
                                update(block.id, {
                                  caption: event.target.value,
                                })
                              }
                            />
                          )}
                        </>
                      )}
                    </div>
                  </article>
                </div>
              ))}
            </div>
          )}
          {!readOnly && (
            <BottomComposer
              index={design.blocks.length}
              onAdd={addSimpleBlock}
              onImage={chooseImage}
              hasPricing={design.blocks.some(
                (block) => block.kind === "pricing",
              )}
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
                }),
              );
            pendingImageIndex.current = null;
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
    </>
  );
});

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
    default:
      return "Text";
  }
}

type InsertKind =
  "heading" | "paragraph" | "quote" | "list" | "divider" | "pricing";

function BottomComposer({
  index,
  onAdd,
  onImage,
  hasPricing,
}: {
  index: number;
  onAdd: (index: number, kind: InsertKind, ordered?: boolean) => void;
  onImage: (index: number) => void;
  hasPricing: boolean;
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
              help={
                hasPricing
                  ? "Already in this quotation"
                  : "Add products, prices and totals"
              }
              Icon={Table2}
              disabled={hasPricing}
              onClick={() => add("pricing")}
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
        <div className="mt-4 grid gap-3 sm:grid-cols-3">
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
                "group relative overflow-hidden rounded-2xl border bg-surface p-3 text-left shadow-sm ring-1 transition-all hover:-translate-y-0.5 hover:border-accent hover:shadow-md",
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
              <span className="mt-3 flex items-start justify-between gap-3 px-1 pb-1">
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

      <section className="mt-6 border-t border-subtle pt-6">
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

      <section className="mt-6 border-t border-subtle pt-6">
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
    </Modal>
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
        "flex min-h-16 items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors hover:border-accent hover:bg-accent-soft",
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
