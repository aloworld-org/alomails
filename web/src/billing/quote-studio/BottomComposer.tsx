import { useState } from "react";
import {
  AlignLeft,
  ImagePlus,
  List,
  ListOrdered,
  Minus,
  Plus,
  Quote,
  Rows3,
  Search,
  Table2,
  Type,
  X,
} from "lucide-react";

import { cx } from "../../ds";
import { strings } from "../../i18n";
import { AddButton } from "./AddButton";

export type InsertKind =
  | "heading"
  | "paragraph"
  | "quote"
  | "list"
  | "divider"
  | "pricing"
  | "table";

interface BottomComposerProps {
  index: number;
  onAdd: (index: number, kind: InsertKind, ordered?: boolean) => void;
  onImage: (index: number) => void;
}

export function BottomComposer({ index, onAdd, onImage }: BottomComposerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const add = (kind: InsertKind, ordered = false) => {
    onAdd(index, kind, ordered);
    setOpen(false);
    setQuery("");
  };
  const options: Array<{
    label: string;
    help: string;
    category: "text" | "media" | "tables" | "layout";
    Icon: typeof AlignLeft;
    action: () => void;
  }> = [
    { label: strings.quoteStudioHeading, help: strings.quoteStudioHeadingHelp, category: "text", Icon: Type, action: () => add("heading") },
    { label: strings.quoteStudioParagraph, help: strings.quoteStudioParagraphHelp, category: "text", Icon: AlignLeft, action: () => add("paragraph") },
    { label: strings.quoteStudioQuote, help: strings.quoteStudioQuoteHelp, category: "text", Icon: Quote, action: () => add("quote") },
    { label: strings.quoteStudioBulletList, help: strings.quoteStudioBulletListHelp, category: "text", Icon: List, action: () => add("list") },
    { label: strings.quoteStudioNumberedList, help: strings.quoteStudioNumberedListHelp, category: "text", Icon: ListOrdered, action: () => add("list", true) },
    {
      label: strings.quoteStudioImage,
      help: strings.quoteStudioImageHelp,
      category: "media",
      Icon: ImagePlus,
      action: () => {
        onImage(index);
        setOpen(false);
        setQuery("");
      },
    },
    { label: strings.quoteStudioPricingTable, help: strings.quoteStudioPricingTableHelp, category: "tables", Icon: Table2, action: () => add("pricing") },
    { label: strings.quoteStudioTable, help: strings.quoteStudioTableHelp, category: "tables", Icon: Rows3, action: () => add("table") },
    { label: strings.quoteStudioDivider, help: strings.quoteStudioDividerHelp, category: "layout", Icon: Minus, action: () => add("divider") },
  ];
  const categories = ["text", "media", "tables", "layout"] as const;
  const categoryLabels = {
    text: strings.quoteStudioCategoryText,
    media: strings.quoteStudioCategoryMedia,
    tables: strings.quoteStudioCategoryTables,
    layout: strings.quoteStudioCategoryLayout,
    results: strings.quoteStudioSearchResults,
  } as const;
  const normalizedQuery = query.trim().toLowerCase();
  const visibleOptions = options.filter((option) =>
    `${option.label} ${option.help} ${option.category}`.toLowerCase().includes(normalizedQuery),
  );

  return (
    <div className="relative flex flex-col items-center py-2" aria-label={strings.quoteStudioAddContentA11y}>
      <div className="flex w-full items-center gap-3">
        <span className="h-px flex-1 bg-[var(--quote-table-header)]" aria-hidden="true" />
        <button
          type="button"
          className="group inline-flex min-h-9 items-center gap-2 rounded-full px-3 text-xs font-semibold text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
          aria-expanded={open}
          aria-label={strings.quoteStudioAddContentBelow}
          onClick={() => setOpen((value) => !value)}
        >
          <span className="grid size-6 place-items-center rounded-full bg-accent-soft text-accent transition-colors group-hover:bg-accent group-hover:text-on-accent">
            <Plus className="size-3.5" aria-hidden="true" />
          </span>
          {strings.quoteStudioAddContent}
        </button>
        <span className="h-px flex-1 bg-[var(--quote-table-header)]" aria-hidden="true" />
      </div>
      {open && (
        <div className="mt-2 w-full max-w-2xl rounded-2xl border border-default bg-surface shadow-xl">
          <div className="p-5 pb-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <h3 className="font-semibold text-primary">{strings.quoteStudioAddToQuotation}</h3>
                <p className="mt-0.5 text-sm text-secondary">{strings.quoteStudioAddToQuotationHelp}</p>
              </div>
              <button
                type="button"
                className="rounded-lg p-2 text-secondary hover:bg-accent-soft hover:text-accent"
                aria-label={strings.quoteStudioCloseBlockPicker}
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
                placeholder={strings.quoteStudioSearchBlocks}
                aria-label={strings.quoteStudioSearchBlocksA11y}
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") setOpen(false);
                }}
              />
            </label>
          </div>
          <div className="max-h-[min(65vh,40rem)] overflow-y-auto border-t border-default px-5">
            {(normalizedQuery === "" ? categories : (["results"] as const)).map((section, sectionIndex) => {
              const sectionOptions = section === "results" ? visibleOptions : visibleOptions.filter((option) => option.category === section);
              if (sectionOptions.length === 0) return null;
              const sectionId = `quote-blocks-${section}`;
              return (
                <section key={section} className={cx("py-4", sectionIndex > 0 && "border-t border-default")} aria-labelledby={sectionId}>
                  <h4 id={sectionId} className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{categoryLabels[section]}</h4>
                  <div className="grid gap-1 sm:grid-cols-2">
                    {sectionOptions.map(({ label, help, Icon, action }) => (
                      <AddButton key={label} label={label} help={help} Icon={Icon} onClick={action} />
                    ))}
                  </div>
                </section>
              );
            })}
          </div>
          {visibleOptions.length === 0 && (
            <div className="border-t border-default px-5 py-8 text-center">
              <p className="text-sm font-semibold text-primary">{strings.quoteStudioNoMatchingBlocks}</p>
              <p className="mt-1 text-xs text-secondary">{strings.quoteStudioTryAnotherName}</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
