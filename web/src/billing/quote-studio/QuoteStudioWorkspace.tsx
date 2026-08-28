import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { QRCodeSVG } from "qrcode.react";
import {
  ArrowDown,
  ArrowUp,
  Copy,
  Mail,
  Palette,
  Pencil,
  Phone,
  Rows3,
  Settings2,
  Trash2,
  Globe2,
} from "lucide-react";

import {
  Select,
  cx,
} from "../../ds";
import { strings, useLocale } from "../../i18n";
import {
  QuoteTableOptionsProvider,
  type QuoteLineContent,
  type QuoteTotalsDetail,
  type QuoteTotalsPlacement,
} from "../quoteTableOptions";
import type { BillingCustomer, BillingQuote, BillingSettings } from "../types";
import { DividerLine } from "./DividerLine";
import { BlockCommand } from "./BlockCommand";
import { EmptyBuilder } from "./EmptyBuilder";
import { BottomComposer } from "./BottomComposer";
import { InlineRichTextContent } from "./InlineRichTextContent";
import { InlineRichTextEditor } from "./InlineRichTextEditor";
import { RichTextContent } from "./RichTextContent";
import { RichTextEditor } from "./RichTextEditor";
import {
  TextColumnsPicker,
  textColumns,
  textColumnsClass,
} from "./TextColumnsPicker";
import { DividerBlockEditor } from "./DividerBlockEditor";
import { ListBlockContent } from "./ListBlockContent";
import { ListBlockEditor } from "./ListBlockEditor";
import { resolveListStyle } from "./listStyles";
import { ImageContentBlock } from "./ImageContentBlock";
import { ImageBlockEditor } from "./ImageBlockEditor";
import { GeneralTableBlock } from "./GeneralTableBlock";
import type {
  QuoteStudioBlock as Block,
} from "./QuoteStudioBlock";
import { quoteStudioBlockName as blockName } from "./quoteStudioBlockName";
import { CustomizeTable } from "./CustomizeTable";
import { CustomizeQuote } from "./CustomizeQuote";
import {
  type QuoteColumns,
  type QuoteStudioDesign as Design,
} from "./QuoteStudioDesign";
import { EMPTY_QUOTE_STUDIO_DESIGN } from "./quoteStudioNormalization";
import {
  createContactVCard,
  formatQuoteDocumentDate,
  quoteCustomerDetailsFromCustomer,
  quotationHeaderRatioClass,
  quoteHeaderDetailsFromSettings,
} from "./quoteStudioHeader";
import { quoteBlockHasPreviewContent } from "./quoteStudioPreview";
import {
  loadQuoteStudioDesign,
  saveQuoteStudioDesign,
} from "./quoteStudioPersistence";
import { readQuoteImage } from "./quoteImageData";
export interface QuoteContentStudioHandle {
  customize: () => void;
  edit: () => void;
  copyTo: (quoteId: string) => Promise<void>;
}

export const QuoteStudioWorkspace = forwardRef<
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
    issuer?: BillingSettings | null;
    quote?: BillingQuote | null;
    customer?: BillingCustomer | null;
    customerName?: string;
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
    issuer,
    quote,
    customer,
    customerName = "",
  },
  ref,
) {
  const locale = useLocale();
  const storageKey = `alo:quote-design:${quoteId}`;
  const [design, setDesign] = useState<Design>(EMPTY_QUOTE_STUDIO_DESIGN);
  const [ready, setReady] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [customizeMode, setCustomizeMode] = useState<
    "header" | "document" | null
  >(null);
  const [tableSettings, setTableSettings] = useState(false);
  const [editingImageId, setEditingImageId] = useState<string | null>(null);
  const [editingDividerId, setEditingDividerId] = useState<string | null>(null);
  const root = useRef<HTMLElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const replaceImageInput = useRef<HTMLInputElement>(null);
  const pendingImageIndex = useRef<number | null>(null);
  const issuerHeaderDetails = quoteHeaderDetailsFromSettings(issuer);
  const headerDetails = design.headerDetailsCustomized
    ? design.headerDetails
    : issuerHeaderDetails;
  const selectedCustomerDetails = quoteCustomerDetailsFromCustomer(
    customer,
    customerName,
  );
  const customerDetails = design.customerDetailsCustomized
    ? design.customerDetails
    : selectedCustomerDetails;
  useImperativeHandle(
    ref,
    () => ({
      customize: () => setCustomizeMode("document"),
      edit: () => {
        const target = root.current?.querySelector<HTMLElement>(
          'input:not([disabled]), textarea:not([disabled]), [contenteditable="true"]',
        );
        target?.scrollIntoView?.({ behavior: "smooth", block: "center" });
        target?.focus({ preventScroll: true });
      },
      copyTo: (nextQuoteId) =>
        saveQuoteStudioDesign(`alo:quote-design:${nextQuoteId}`, design),
    }),
    [design],
  );

  useEffect(() => {
    let current = true;
    setReady(false);
    void loadQuoteStudioDesign(storageKey).then((saved) => {
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
      void saveQuoteStudioDesign(storageKey, design)
        .then(() => {
          if (current) setSaveError("");
        })
        .catch(() => {
          if (current) setSaveError(strings.quoteStudioDesignSaveRetry);
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
      "--quote-contact-icons": design.colors.contactIcons,
      "--quote-background": design.colors.background,
      "--quote-header-background": design.colors.headerBackground,
      "--quote-text": design.colors.text,
      "--quote-table-header": design.colors.tableHeader,
      "--quote-table-row": design.colors.tableRows,
      "--quote-bullet-marker": design.colors.bulletMarker,
      "--quote-number-marker": design.colors.numberMarker,
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
    if (kind === "divider")
      insertBlock(index, {
        id,
        kind,
        thickness: "fine",
        style: "solid",
        width: 100,
        color: design.colors.accent,
      });
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
            title: strings.quoteStudioPricingTableNumber(
              current.blocks.filter((block) => block.kind === "pricing")
                .length + 1,
            ),
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
          {
            id: crypto.randomUUID(),
            label: strings.quoteStudioColumnNumber(1),
          },
          {
            id: crypto.randomUUID(),
            label: strings.quoteStudioColumnNumber(2),
          },
          {
            id: crypto.randomUUID(),
            label: strings.quoteStudioColumnNumber(3),
          },
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

  const contactQrNode =
    design.showContactQr && headerDetails.companyName ? (
      <div className="w-fit shrink-0 self-start text-center">
        <div className="inline-flex bg-white p-1">
          <QRCodeSVG
            value={createContactVCard(headerDetails)}
            size={
              design.contactQrSize === "small"
                ? 48
                : design.contactQrSize === "large"
                  ? 80
                  : 64
            }
            fgColor={design.contactQrColor}
            bgColor="#ffffff"
            level="M"
            marginSize={0}
            title={strings.quoteStudioScanToSave}
          />
        </div>
        <p className="mt-1 text-[9px] font-medium leading-tight opacity-60">
          {strings.quoteStudioScanToSave}
        </p>
      </div>
    ) : null;

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
                {strings.quoteStudioBuildTitle}
              </h2>
              <p className="mt-0.5 text-sm text-secondary">
                {strings.quoteStudioBuildHelp}
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
          {(design.logo ||
            quote ||
            customerName ||
            Object.values(headerDetails).some((value) => value.trim())) && (
            <div
              className={cx(
                "group/quote-header relative mb-8 grid overflow-hidden bg-[var(--quote-header-background)]",
                design.headerStyle === "minimal"
                  ? "border-y border-[var(--quote-table-header)]"
                  : "rounded-2xl border border-[var(--quote-table-header)]",
                quotationHeaderRatioClass(
                  design.headerRatio,
                  design.headerAlignment,
                ),
                design.headerStyle === "band" &&
                  "border-t-8 border-t-[var(--quote-accent)]",
              )}
            >
              <div
                className={cx(
                  "flex min-w-0 flex-col px-8 py-8 max-sm:px-5 max-sm:py-6",
                  design.headerStyle === "editorial" && "md:py-12",
                  design.headerStyle === "minimal" && "md:px-6 md:py-6",
                  design.headerAlignment === "right" && "md:order-2",
                )}
              >
                <div
                  className={cx(
                    "flex min-w-0 gap-5",
                    design.headerStyle === "stacked"
                      ? "flex-col items-start gap-3"
                      : "items-center",
                  )}
                >
                  {design.logo && (
                    <img
                      src={design.logo}
                      alt={strings.quoteStudioCompanyLogo}
                      className={cx(
                        "h-16 w-20 shrink-0 object-contain",
                        design.headerStyle === "stacked" && "h-20 w-24",
                        design.headerStyle === "minimal" && "h-10 w-14",
                      )}
                    />
                  )}
                  {headerDetails.companyName && (
                    <p className="text-xl font-semibold leading-tight tracking-tight text-[var(--quote-text)]">
                      {headerDetails.companyName}
                    </p>
                  )}
                </div>
                <div className="mt-7 flex min-w-0 flex-1 flex-col text-[var(--quote-text)]">
                  <div
                    className={cx(
                      "grid items-start gap-x-10 gap-y-6 text-sm leading-6",
                      contactQrNode === null
                        ? "sm:grid-cols-2"
                        : design.contactQrAlignment === "left"
                          ? "sm:grid-cols-[auto_minmax(0,0.9fr)_minmax(0,1.1fr)]"
                          : "sm:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)_auto]",
                    )}
                  >
                    {design.contactQrAlignment === "left" && contactQrNode}
                    {headerDetails.address && (
                      <div>
                        <p className="mb-2 text-xs font-semibold uppercase tracking-[0.12em] opacity-55">
                          {strings.quoteStudioAddress}
                        </p>
                        <p className="whitespace-pre-line opacity-80">
                          {headerDetails.address}
                        </p>
                      </div>
                    )}
                    {(headerDetails.email ||
                      headerDetails.phone ||
                      headerDetails.website) && (
                      <div>
                        <p className="mb-2 text-xs font-semibold uppercase tracking-[0.12em] opacity-55">
                          {strings.quoteStudioContact}
                        </p>
                        <div className="flex min-w-0 flex-col gap-1.5 opacity-80">
                          {headerDetails.email && (
                            <span className="flex items-center gap-2.5">
                              <Mail
                                className="size-4 shrink-0 text-[var(--quote-contact-icons)]"
                                aria-hidden="true"
                              />
                              {headerDetails.email}
                            </span>
                          )}
                          {headerDetails.phone && (
                            <span className="flex items-center gap-2.5">
                              <Phone
                                className="size-4 shrink-0 text-[var(--quote-contact-icons)]"
                                aria-hidden="true"
                              />
                              {headerDetails.phone}
                            </span>
                          )}
                          {headerDetails.website && (
                            <span className="flex items-center gap-2.5 break-all">
                              <Globe2
                                className="size-4 shrink-0 text-[var(--quote-contact-icons)]"
                                aria-hidden="true"
                              />
                              {headerDetails.website.replace(
                                /^https?:\/\//,
                                "",
                              )}
                            </span>
                          )}
                        </div>
                      </div>
                    )}
                    {design.contactQrAlignment === "right" && contactQrNode}
                  </div>
                  {(headerDetails.vatId || headerDetails.registrationNo) && (
                    <dl className="mt-auto grid grid-cols-2 gap-x-10 gap-y-3 border-t border-[var(--quote-table-header)] pt-4 text-xs text-[var(--quote-text)]">
                      {headerDetails.vatId && (
                        <div>
                          <dt className="text-[11px] font-semibold uppercase tracking-wide opacity-55">
                            {strings.quoteStudioVatId}
                          </dt>
                          <dd className="mt-1.5 font-semibold">
                            {headerDetails.vatId}
                          </dd>
                        </div>
                      )}
                      {headerDetails.registrationNo && (
                        <div>
                          <dt className="text-[11px] font-semibold uppercase tracking-wide opacity-55">
                            {strings.quoteStudioCompanyNumber}
                          </dt>
                          <dd className="mt-1.5 font-semibold">
                            {headerDetails.registrationNo}
                          </dd>
                        </div>
                      )}
                    </dl>
                  )}
                </div>
              </div>
              <div
                className={cx(
                  "flex min-w-0 flex-col border-[var(--quote-table-header)] px-8 py-8 max-sm:border-t max-sm:px-5 max-sm:py-6 md:border-l",
                  design.headerStyle === "editorial" && "md:px-10 md:py-12",
                  design.headerStyle === "minimal" && "md:px-6 md:py-6",
                  design.headerAlignment === "right" &&
                    "md:order-1 md:border-l-0 md:border-r",
                )}
              >
                <div className="max-w-lg">
                  <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-[var(--quote-accent)]">
                    {strings.quoteStudioQuotation}
                  </p>
                  <p
                    className={cx(
                      "mt-2 font-semibold leading-none tracking-tight text-[var(--quote-text)]",
                      design.headerStyle === "editorial"
                        ? "text-4xl"
                        : "text-2xl",
                    )}
                  >
                    {quote?.number ?? strings.quoteStudioDraftQuotation}
                  </p>
                  {Object.values(customerDetails).some(Boolean) && (
                    <div className="mt-5 text-[var(--quote-text)]">
                      <p className="text-[11px] font-semibold uppercase tracking-wide opacity-55">
                        {strings.quoteStudioPreparedFor}
                      </p>
                      {customerDetails.companyName && (
                        <p className="mt-1 text-xl font-semibold leading-tight tracking-tight">
                          {customerDetails.companyName}
                        </p>
                      )}
                      {customerDetails.contactName && (
                        <p className="mt-1 text-sm opacity-75">
                          {customerDetails.contactName}
                        </p>
                      )}
                      <div className="mt-4 grid gap-x-12 gap-y-5 text-sm leading-6 sm:grid-cols-2">
                        {customerDetails.address && (
                          <div>
                            <p className="mb-2 text-xs font-semibold uppercase tracking-[0.12em] opacity-55">
                              {strings.quoteStudioAddress}
                            </p>
                            <p className="whitespace-pre-line opacity-80">
                              {customerDetails.address}
                            </p>
                          </div>
                        )}
                        {(customerDetails.email || customerDetails.phone) && (
                          <div>
                            <p className="mb-2 text-xs font-semibold uppercase tracking-[0.12em] opacity-55">
                              {strings.quoteStudioContact}
                            </p>
                            <div className="flex flex-col gap-1.5 opacity-80">
                              {customerDetails.email && (
                                <span className="flex items-center gap-2.5">
                                  <Mail
                                    className="size-4 shrink-0 text-[var(--quote-contact-icons)]"
                                    aria-hidden="true"
                                  />
                                  {customerDetails.email}
                                </span>
                              )}
                              {customerDetails.phone && (
                                <span className="flex items-center gap-2.5">
                                  <Phone
                                    className="size-4 shrink-0 text-[var(--quote-contact-icons)]"
                                    aria-hidden="true"
                                  />
                                  {customerDetails.phone}
                                </span>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                      {customerDetails.vatId && (
                        <p className="mt-4 border-t border-[var(--quote-table-header)] pt-3 text-xs opacity-65">
                          {`VAT ${customerDetails.vatId}`}
                        </p>
                      )}
                    </div>
                  )}
                </div>
                <dl className="mt-auto grid max-w-lg grid-cols-2 gap-x-10 gap-y-3 border-t border-[var(--quote-table-header)] pt-4 text-xs text-[var(--quote-text)]">
                  <div>
                    <dt className="text-[11px] font-semibold uppercase tracking-wide opacity-55">
                      {strings.quoteStudioIssued}
                    </dt>
                    <dd className="mt-1.5 font-semibold">
                      {formatQuoteDocumentDate(quote?.sentDate, locale) ??
                        strings.quoteStudioOnFinalization}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[11px] font-semibold uppercase tracking-wide opacity-55">
                      {strings.quoteStudioValidUntil}
                    </dt>
                    <dd className="mt-1.5 font-semibold">
                      {formatQuoteDocumentDate(quote?.validUntil, locale) ??
                        strings.quoteStudioDaysAfterIssue(
                          new Intl.NumberFormat(locale).format(
                            quote?.validDays ?? 30,
                          ),
                        )}
                    </dd>
                  </div>
                </dl>
              </div>
              {!readOnly && (
                <button
                  type="button"
                  className="absolute right-4 top-4 inline-flex min-h-10 items-center gap-2 rounded-xl bg-accent-soft px-4 py-2 text-sm font-medium text-accent transition-colors duration-150 hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15"
                  onClick={() => setCustomizeMode("header")}
                  aria-label={strings.quoteStudioEditHeader}
                >
                  <Pencil className="size-4" aria-hidden="true" />
                  {strings.quoteStudioEditHeader}
                </button>
              )}
            </div>
          )}
          {design.blocks.length === 0 ? (
            <EmptyBuilder readOnly={readOnly} />
          ) : (
            <div className={cx("flex flex-col", readOnly ? "gap-8" : "gap-5")}>
              {design.blocks
                .filter((block) => !readOnly || quoteBlockHasPreviewContent(block))
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
                                {strings.quoteStudioTableName}
                              </span>
                              <span className="relative block min-w-0">
                                <Pencil
                                  className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-tertiary"
                                  aria-hidden="true"
                                />
                                <input
                                  className="min-h-10 w-full min-w-52 rounded-lg border border-default bg-surface py-2 pl-9 pr-3 text-sm font-semibold text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none"
                                  value={
                                    block.title ??
                                    strings.quoteStudioPricingTable
                                  }
                                  aria-label={strings.quoteStudioTableName}
                                  placeholder={strings.quoteStudioPricingTable}
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
                              block.kind === "image" ||
                              block.kind === "divider") && (
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
                                            ? strings.quoteStudioShowSubtotal
                                            : strings.quoteStudioHideSubtotal
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
                                      label={strings.quoteStudioTableSettings}
                                      onClick={() => setTableSettings(true)}
                                    >
                                      <Palette className="size-4" />
                                    </BlockCommand>
                                  </>
                                )}
                                {block.kind === "image" && (
                                  <BlockCommand
                                    accent
                                    label={strings.quoteStudioEditBlock}
                                    onClick={() => setEditingImageId(block.id)}
                                  >
                                    <Pencil className="size-4" />
                                  </BlockCommand>
                                )}
                                {block.kind === "divider" && (
                                  <BlockCommand
                                    accent
                                    label={strings.quoteStudioDividerSettings}
                                    onClick={() => setEditingDividerId(block.id)}
                                  >
                                    <Settings2 className="size-4" />
                                  </BlockCommand>
                                )}
                              </div>
                            )}
                            <div className="flex flex-wrap items-center gap-2">
                              <BlockCommand
                                label={strings.quoteStudioMoveUp}
                                disabled={index === 0}
                                onClick={() => moveBlock(index, -1)}
                              >
                                <ArrowUp className="size-4" />
                              </BlockCommand>
                              <BlockCommand
                                label={strings.quoteStudioMoveDown}
                                disabled={index === design.blocks.length - 1}
                                onClick={() => moveBlock(index, 1)}
                              >
                                <ArrowDown className="size-4" />
                              </BlockCommand>
                              {block.kind !== "pricing" && (
                                <BlockCommand
                                  label={strings.quoteStudioDuplicate}
                                  onClick={() => duplicateBlock(index)}
                                >
                                  <Copy className="size-4" />
                                </BlockCommand>
                              )}
                            </div>
                            <BlockCommand
                              label={strings.quoteStudioDelete}
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
                              title:
                                block.title ?? strings.quoteStudioPricingTable,
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
                                aria-label={strings.quoteStudioHeadingLevel}
                                onChange={(event) =>
                                  update(block.id, {
                                    level: Number(event.target.value) as
                                      1 | 2 | 3,
                                  })
                                }
                              >
                                <option value="1">
                                  {strings.quoteStudioHeading1}
                                </option>
                                <option value="2">
                                  {strings.quoteStudioHeading2}
                                </option>
                                <option value="3">
                                  {strings.quoteStudioHeading3}
                                </option>
                              </Select>
                              <InlineRichTextEditor
                                value={block.text}
                                placeholder={strings.quoteStudioSectionHeading}
                                aria-label={strings.quoteStudioSectionHeading}
                                onChange={(text) => update(block.id, { text })}
                              />
                            </div>
                          )
                        ) : block.kind === "paragraph" ? (
                          readOnly ? (
                            <div className={textColumnsClass(textColumns(block.columns))}>
                              <RichTextContent value={block.text} />
                            </div>
                          ) : (
                            <div className="grid gap-3">
                              <div className="flex justify-end">
                                <TextColumnsPicker
                                  value={textColumns(block.columns)}
                                  label={strings.quoteStudioParagraphColumns}
                                  onChange={(columns) => update(block.id, { columns })}
                                />
                              </div>
                              <RichTextEditor
                                value={block.text}
                                label={strings.quoteStudioParagraph}
                                placeholder={strings.quoteStudioWriteParagraph}
                                onChange={(text) => update(block.id, { text })}
                              />
                            </div>
                          )
                        ) : block.kind === "quote" ? (
                          readOnly ? (
                            <blockquote className="border-l-4 border-[var(--quote-accent)] pl-5 text-lg italic">
                              <div className={textColumnsClass(textColumns(block.columns))}>
                                <RichTextContent value={block.text} />
                              </div>
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
                              <div className="flex justify-end">
                                <TextColumnsPicker
                                  value={textColumns(block.columns)}
                                  label={strings.quoteStudioQuoteColumns}
                                  onChange={(columns) => update(block.id, { columns })}
                                />
                              </div>
                              <RichTextEditor
                                value={block.text}
                                label={strings.quoteStudioQuotation}
                                placeholder={
                                  strings.quoteStudioImportantStatement
                                }
                                onChange={(text) => update(block.id, { text })}
                              />
                              <InlineRichTextEditor
                                value={block.attribution}
                                placeholder={strings.quoteStudioAttribution}
                                aria-label={strings.quoteStudioQuoteAttribution}
                                onChange={(attribution) =>
                                  update(block.id, { attribution })
                                }
                              />
                            </div>
                          )
                        ) : block.kind === "list" ? (
                          readOnly ? (
                            <ListBlockContent block={block} />
                          ) : (
                            <ListBlockEditor
                              ordered={block.ordered}
                              items={block.items}
                              columns={block.columns ?? 1}
                              style={resolveListStyle(block.style, block.ordered)}
                              onChange={(patch) => update(block.id, patch)}
                            />
                          )
                        ) : block.kind === "divider" ? (
                          readOnly ? (
                            <DividerLine block={block} />
                          ) : (
                            <DividerBlockEditor
                              block={block}
                              fallbackColor={design.colors.accent}
                              onChange={(patch) => update(block.id, patch)}
                              open={editingDividerId === block.id}
                              onOpenChange={(open) =>
                                setEditingDividerId(open ? block.id : null)
                              }
                            />
                          )
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
                                placeholder={strings.quoteStudioSectionHeading}
                                aria-label={strings.quoteStudioSectionHeading}
                                onChange={(heading) =>
                                  update(block.id, { heading })
                                }
                              />
                              <div className="mt-3">
                                <RichTextEditor
                                  value={block.body}
                                  label={strings.quoteStudioSectionText}
                                  placeholder={
                                    strings.quoteStudioSectionTextPlaceholder
                                  }
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
          {issuer?.footerNote.trim() && (
            <footer className="mt-10 px-1 text-xs leading-relaxed text-[var(--quote-text)] opacity-70">
              {issuer.footerNote}
            </footer>
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
              readQuoteImage(file, (src) =>
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
              readQuoteImage(file, (src) => update(editingImageId, { src }));
            event.currentTarget.value = "";
          }}
        />
      </section>
      {customizeMode && (
        <CustomizeQuote
          mode={customizeMode}
          design={design}
          issuerDetails={issuerHeaderDetails}
          customerDetails={selectedCustomerDetails}
          saveError={saveError}
          onChange={setDesign}
          onClose={() => setCustomizeMode(null)}
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
