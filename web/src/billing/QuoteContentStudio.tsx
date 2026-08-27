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
  AlignLeft,
  ArrowDown,
  ArrowUp,
  Bold,
  Building2,
  Check,
  Copy,
  ContactRound,
  FileText,
  Heading1,
  Heading2,
  Heading3,
  ImagePlus,
  Italic,
  List,
  ListOrdered,
  Link,
  Mail,
  Minus,
  Palette,
  Pilcrow,
  Pencil,
  Phone,
  Plus,
  Quote,
  QrCode,
  RotateCcw,
  Rows3,
  Search,
  Settings2,
  Table2,
  Type,
  Trash2,
  Upload,
  Globe2,
  X,
} from "lucide-react";

import {
  Button,
  ChoicePicker,
  ColorPicker,
  IconButton,
  Modal,
  Select,
  cx,
} from "../ds";
import { strings, useLocale } from "../i18n";
import {
  QuoteTableOptionsProvider,
  type QuoteLineContent,
  type QuoteTableLayout,
  type QuoteTotalsDetail,
  type QuoteTotalsPlacement,
} from "./quoteTableOptions";
import type { BillingCustomer, BillingQuote, BillingSettings } from "./types";

type Theme = "modern" | "editorial" | "minimal";
type HeaderAlignment = "left" | "right";
type HeaderStyle = "signature" | "editorial" | "band" | "minimal" | "stacked";
type HeaderRatio = "40-60" | "50-50" | "60-40";
type ContactQrSize = "small" | "medium" | "large";
type DividerThickness = "fine" | "medium" | "bold";
type DividerStyle = "solid" | "dashed" | "dotted";
type DividerWidth = 25 | 50 | 75 | 100;
export type QuoteTemplatePreset = "blank" | "services" | "project" | "retainer";
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
  | {
      id: string;
      kind: "divider";
      thickness?: DividerThickness;
      style?: DividerStyle;
      width?: DividerWidth;
      color?: string;
    }
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
  contactIcons: string;
  background: string;
  headerBackground: string;
  text: string;
  tableHeader: string;
  tableRows: string;
  bulletMarker: string;
  numberMarker: string;
}
interface HeaderDetails {
  companyName: string;
  address: string;
  email: string;
  phone: string;
  website: string;
  vatId: string;
  registrationNo: string;
}
interface CustomerHeaderDetails {
  companyName: string;
  contactName: string;
  address: string;
  email: string;
  phone: string;
  vatId: string;
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
  headerStyle: HeaderStyle;
  headerAlignment: HeaderAlignment;
  headerRatio: HeaderRatio;
  headerDetails: HeaderDetails;
  headerDetailsCustomized: boolean;
  customerDetails: CustomerHeaderDetails;
  customerDetailsCustomized: boolean;
  showContactQr: boolean;
  contactQrAlignment: HeaderAlignment;
  contactQrSize: ContactQrSize;
  contactQrColor: string;
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
  contactIcons: "#e76f51",
  background: "#faf7f2",
  headerBackground: "#ffffff",
  text: "#102a43",
  tableHeader: "#f3f0ea",
  tableRows: "#ffffff",
  bulletMarker: "#e76f51",
  numberMarker: "#e76f51",
};
const DEFAULT_HEADER_DETAILS: HeaderDetails = {
  companyName: "",
  address: "",
  email: "",
  phone: "",
  website: "",
  vatId: "",
  registrationNo: "",
};
const DEFAULT_CUSTOMER_DETAILS: CustomerHeaderDetails = {
  companyName: "",
  contactName: "",
  address: "",
  email: "",
  phone: "",
  vatId: "",
};
const EMPTY: Design = {
  logo: "",
  headerStyle: "signature",
  headerAlignment: "left",
  headerRatio: "50-50",
  headerDetails: DEFAULT_HEADER_DETAILS,
  headerDetailsCustomized: false,
  customerDetails: DEFAULT_CUSTOMER_DETAILS,
  customerDetailsCustomized: false,
  showContactQr: false,
  contactQrAlignment: "right",
  contactQrSize: "medium",
  contactQrColor: "#102a43",
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

function templateBlockId(kind: string) {
  return `${kind}-${crypto.randomUUID()}`;
}

function templateDesign(preset: QuoteTemplatePreset): Design {
  if (preset === "blank") return savedDesign(EMPTY);

  if (preset === "services") {
    return savedDesign({
      ...EMPTY,
      headerStyle: "minimal",
      theme: "minimal",
      tableLayout: "compact",
      totalsPlacement: "footer",
      blocks: [
        {
          id: templateBlockId("heading"),
          kind: "heading",
          level: 1,
          text: strings.quoteStudioTemplateServicesHeading,
        },
        {
          id: templateBlockId("paragraph"),
          kind: "paragraph",
          text: strings.quoteStudioTemplateServicesIntroduction,
        },
        {
          id: templateBlockId("pricing"),
          kind: "pricing",
          title: strings.quoteStudioTemplateServicesTable,
          showSubtotal: true,
        },
      ],
    });
  }

  if (preset === "project") {
    return savedDesign({
      ...EMPTY,
      headerStyle: "band",
      headerRatio: "60-40",
      theme: "editorial",
      tableLayout: "detailed",
      showProductDescriptions: true,
      totalsPlacement: "full",
      blocks: [
        {
          id: templateBlockId("heading"),
          kind: "heading",
          level: 1,
          text: strings.quoteStudioTemplateProjectHeading,
        },
        {
          id: templateBlockId("paragraph"),
          kind: "paragraph",
          text: strings.quoteStudioTemplateProjectIntroduction,
        },
        {
          id: templateBlockId("list"),
          kind: "list",
          ordered: false,
          columns: 3,
          items: [
            strings.quoteStudioTemplateProjectDiscovery,
            strings.quoteStudioTemplateProjectDelivery,
            strings.quoteStudioTemplateProjectHandover,
          ].join("\n"),
        },
        {
          id: templateBlockId("pricing"),
          kind: "pricing",
          title: strings.quoteStudioTemplateProjectTable,
          showSubtotal: true,
        },
      ],
    });
  }

  return savedDesign({
    ...EMPTY,
    headerStyle: "stacked",
    headerRatio: "40-60",
    theme: "modern",
    tableLayout: "detailed",
    totalsPlacement: "summary",
    showCurrencyCode: true,
    blocks: [
      {
        id: templateBlockId("heading"),
        kind: "heading",
        level: 1,
        text: strings.quoteStudioTemplateRetainerHeading,
      },
      {
        id: templateBlockId("paragraph"),
        kind: "paragraph",
        text: strings.quoteStudioTemplateRetainerIntroduction,
      },
      {
        id: templateBlockId("pricing"),
        kind: "pricing",
        title: strings.quoteStudioTemplateRetainerTable,
        showSubtotal: true,
      },
      {
        id: templateBlockId("divider"),
        kind: "divider",
      },
      {
        id: templateBlockId("list"),
        kind: "list",
        ordered: false,
        columns: 2,
        items: [
          strings.quoteStudioTemplateRetainerReporting,
          strings.quoteStudioTemplateRetainerSupport,
        ].join("\n"),
      },
    ],
  });
}
const DESIGN_STORE = "quote-designs";
const DESIGN_DATABASE = "alo-quote-assets";
const headerRatioChoices: Array<{
  id: HeaderRatio;
  columns: string;
  reverseColumns: string;
}> = [
  {
    id: "40-60",
    columns: "grid-cols-[2fr_3fr]",
    reverseColumns: "grid-cols-[3fr_2fr]",
  },
  { id: "50-50", columns: "grid-cols-2", reverseColumns: "grid-cols-2" },
  {
    id: "60-40",
    columns: "grid-cols-[3fr_2fr]",
    reverseColumns: "grid-cols-[2fr_3fr]",
  },
];

const headerRatioClasses: Record<HeaderRatio, string> = {
  "40-60": "md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]",
  "50-50": "md:grid-cols-2",
  "60-40": "md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]",
};

function quotationHeaderRatioClass(
  ratio: HeaderRatio,
  alignment: HeaderAlignment,
) {
  if (alignment === "left" || ratio === "50-50")
    return headerRatioClasses[ratio];
  return ratio === "40-60"
    ? headerRatioClasses["60-40"]
    : headerRatioClasses["40-60"];
}

function HeaderStylePreview({ style }: { style: HeaderStyle }) {
  if (style === "editorial") {
    return (
      <span
        className="flex h-20 items-end justify-between rounded-xl bg-raised p-3"
        aria-hidden="true"
      >
        <span className="space-y-2">
          <span className="block h-2 w-20 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-12 rounded-full bg-primary/10" />
        </span>
        <span className="mb-auto size-7 rounded-lg bg-accent-soft" />
      </span>
    );
  }
  if (style === "band") {
    return (
      <span
        className="flex h-20 overflow-hidden rounded-xl bg-raised"
        aria-hidden="true"
      >
        <span className="flex w-2/5 flex-col justify-center gap-2 bg-accent px-3">
          <span className="block size-6 rounded-md bg-white/80" />
          <span className="block h-1.5 w-12 rounded-full bg-white/70" />
        </span>
        <span className="flex flex-1 flex-col justify-center gap-2 px-3">
          <span className="block h-2 w-16 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-10 rounded-full bg-primary/10" />
        </span>
      </span>
    );
  }
  if (style === "minimal") {
    return (
      <span
        className="flex h-20 items-center justify-between border-y border-default px-2"
        aria-hidden="true"
      >
        <span className="flex items-center gap-2">
          <span className="size-6 rounded-full border border-accent/40" />
          <span className="block h-1.5 w-12 rounded-full bg-primary/20" />
        </span>
        <span className="block h-1.5 w-12 rounded-full bg-accent" />
      </span>
    );
  }
  if (style === "stacked") {
    return (
      <span
        className="grid h-20 grid-cols-[0.8fr_1.2fr] overflow-hidden rounded-xl bg-raised"
        aria-hidden="true"
      >
        <span className="flex flex-col items-center justify-center gap-1.5">
          <span className="size-7 rounded-lg bg-accent-soft" />
          <span className="block h-1.5 w-12 rounded-full bg-primary/20" />
        </span>
        <span className="flex flex-col justify-center gap-2 border-l border-default px-3">
          <span className="block h-2 w-16 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-10 rounded-full bg-accent/60" />
        </span>
      </span>
    );
  }
  return (
    <span
      className="grid h-20 grid-cols-[1.1fr_0.9fr] overflow-hidden rounded-xl bg-raised"
      aria-hidden="true"
    >
      <span className="flex items-center gap-2.5 px-3">
        <span className="size-7 rounded-lg bg-accent-soft" />
        <span className="space-y-1.5">
          <span className="block h-2 w-14 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-10 rounded-full bg-primary/10" />
        </span>
      </span>
      <span className="flex flex-col justify-center gap-2 border-l border-default px-3">
        <span className="block h-1.5 w-12 rounded-full bg-accent/60" />
        <span className="block h-1.5 w-8 rounded-full bg-primary/10" />
      </span>
    </span>
  );
}

function formatDocumentDate(value: string | null | undefined, locale: string) {
  if (!value) return null;
  const [year, month, day] = value.split("-").map(Number);
  if (!year || !month || !day) return value;
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(Date.UTC(year, month - 1, day)));
}

function vCardValue(value: string) {
  return value
    .replace(/\\/g, "\\\\")
    .replace(/\n/g, "\\n")
    .replace(/;/g, "\\;")
    .replace(/,/g, "\\,");
}

function contactVCard(details: HeaderDetails) {
  return [
    "BEGIN:VCARD",
    "VERSION:3.0",
    `FN:${vCardValue(details.companyName)}`,
    `ORG:${vCardValue(details.companyName)}`,
    details.phone && `TEL;TYPE=WORK,VOICE:${vCardValue(details.phone)}`,
    details.email && `EMAIL;TYPE=WORK:${vCardValue(details.email)}`,
    details.website && `URL:${vCardValue(details.website)}`,
    details.address &&
      `ADR;TYPE=WORK:;;${vCardValue(details.address).replace(/\\n/g, ";")};;;`,
    "END:VCARD",
  ]
    .filter(Boolean)
    .join("\n");
}

function legacyDesign(key: string): Design | null {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return null;
    const saved = JSON.parse(raw) as Partial<Design>;
    return savedDesign(saved);
  } catch {
    return null;
  }
}

function savedDesign(saved: Partial<Design>): Design {
  const headerDetails = { ...DEFAULT_HEADER_DETAILS, ...saved.headerDetails };
  const customerDetails = {
    ...DEFAULT_CUSTOMER_DETAILS,
    ...saved.customerDetails,
  };
  return normalizeDesign({
    ...EMPTY,
    ...saved,
    colors: { ...DEFAULT_COLORS, ...saved.colors },
    headerDetails,
    headerDetailsCustomized:
      saved.headerDetailsCustomized ??
      Object.values(headerDetails).some((value) => value.trim().length > 0),
    customerDetails,
    customerDetailsCustomized:
      saved.customerDetailsCustomized ??
      Object.values(customerDetails).some((value) => value.trim().length > 0),
  });
}

function headerDetailsFromSettings(
  settings?: BillingSettings | null,
): HeaderDetails {
  if (!settings) return DEFAULT_HEADER_DETAILS;
  const locality = [settings.postalCode, settings.city]
    .filter(Boolean)
    .join(" ");
  return {
    companyName: settings.legalName,
    address: [
      settings.addressLine1,
      settings.addressLine2,
      locality,
      settings.country,
    ]
      .filter(Boolean)
      .join("\n"),
    email: settings.email,
    phone: settings.phone,
    website: settings.website,
    vatId: settings.vatId ?? "",
    registrationNo: settings.registrationNo,
  };
}

function customerDetailsFromCustomer(
  customer?: BillingCustomer | null,
  fallbackName = "",
): CustomerHeaderDetails {
  if (!customer)
    return { ...DEFAULT_CUSTOMER_DETAILS, companyName: fallbackName };
  const locality = [customer.postalCode, customer.city]
    .filter(Boolean)
    .join(" ");
  return {
    companyName: customer.name,
    contactName: "",
    address: [
      customer.addressLine1,
      customer.addressLine2,
      locality,
      customer.country,
    ]
      .filter(Boolean)
      .join("\n"),
    email: customer.email ?? "",
    phone: "",
    vatId: customer.vatId ?? "",
  };
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
        request.error ?? new Error(strings.quoteStudioDesignDatabaseError),
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
    if (saved !== undefined) return savedDesign(saved);
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
        transaction.error ?? new Error(strings.quoteStudioDesignSaveError),
      );
    transaction.onabort = () =>
      reject(
        transaction.error ?? new Error(strings.quoteStudioDesignSaveCancelled),
      );
  });
  database.close();
  localStorage.removeItem(key);
}

export async function saveQuoteTemplateDesign(
  quoteId: string,
  preset: QuoteTemplatePreset,
): Promise<void> {
  const key = `alo:quote-design:${quoteId}`;
  const design = templateDesign(preset);
  try {
    await saveDesign(key, design);
  } catch {
    localStorage.setItem(key, JSON.stringify(design));
  }
}
function imageData(file: File, done: (value: string) => void) {
  const reader = new FileReader();
  reader.onload = () =>
    typeof reader.result === "string" && done(reader.result);
  reader.readAsDataURL(file);
}

export interface QuoteContentStudioHandle {
  customize: () => void;
  edit: () => void;
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
  const [design, setDesign] = useState<Design>(EMPTY);
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
  const issuerHeaderDetails = headerDetailsFromSettings(issuer);
  const headerDetails = design.headerDetailsCustomized
    ? design.headerDetails
    : issuerHeaderDetails;
  const selectedCustomerDetails = customerDetailsFromCustomer(
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
            value={contactVCard(headerDetails)}
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
                      {formatDocumentDate(quote?.sentDate, locale) ??
                        strings.quoteStudioOnFinalization}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[11px] font-semibold uppercase tracking-wide opacity-55">
                      {strings.quoteStudioValidUntil}
                    </dt>
                    <dd className="mt-1.5 font-semibold">
                      {formatDocumentDate(quote?.validUntil, locale) ??
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
                            <RichTextContent value={block.text} />
                          ) : (
                            <RichTextEditor
                              value={block.text}
                              label={strings.quoteStudioParagraph}
                              placeholder={strings.quoteStudioWriteParagraph}
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
                            block.ordered ? (
                              <ol
                                className={cx(
                                  "grid list-none gap-x-10 gap-y-3",
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
                                    <li
                                      key={itemIndex}
                                      className="flex min-w-0 items-start gap-3"
                                    >
                                      <span
                                        className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full bg-[var(--quote-number-marker)] text-xs font-semibold text-white"
                                        aria-hidden="true"
                                      >
                                        {itemIndex + 1}
                                      </span>
                                      <span className="min-w-0 flex-1 pt-0.5">
                                        <InlineRichTextContent value={item} />
                                      </span>
                                    </li>
                                  ))}
                              </ol>
                            ) : (
                              <ul
                                className={cx(
                                  "grid list-none gap-x-10 gap-y-3",
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
                                    <li
                                      key={itemIndex}
                                      className="flex min-w-0 items-start gap-3"
                                    >
                                      <span
                                        className="mt-2 size-2 shrink-0 rounded-full bg-[var(--quote-bullet-marker)]"
                                        aria-hidden="true"
                                      />
                                      <span className="min-w-0 flex-1">
                                        <InlineRichTextContent value={item} />
                                      </span>
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

const dividerThicknessClasses: Record<DividerThickness, string> = {
  fine: "border-t",
  medium: "border-t-2",
  bold: "border-t-4",
};

const dividerWidthClasses: Record<DividerWidth, string> = {
  25: "w-1/4",
  50: "w-1/2",
  75: "w-3/4",
  100: "w-full",
};

function DividerLine({
  block,
}: {
  block: Extract<Block, { kind: "divider" }>;
}) {
  const thickness = block.thickness ?? "fine";
  const width = block.width ?? 100;
  return (
    <div
      className={cx(
        "mx-auto border-0",
        dividerThicknessClasses[thickness],
        dividerWidthClasses[width],
      )}
      style={{
        borderTopColor: block.color ?? "var(--quote-accent)",
        borderTopStyle: block.style ?? "solid",
      }}
      aria-hidden="true"
    />
  );
}

function DividerBlockEditor({
  block,
  fallbackColor,
  onChange,
  open,
  onOpenChange,
}: {
  block: Extract<Block, { kind: "divider" }>;
  fallbackColor: string;
  onChange: (patch: Partial<Extract<Block, { kind: "divider" }>>) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const thickness = block.thickness ?? "fine";
  const style = block.style ?? "solid";
  const width = block.width ?? 100;
  const color = block.color ?? fallbackColor;
  const thicknessChoices: Array<{ value: DividerThickness; label: string }> = [
    { value: "fine", label: strings.quoteStudioDividerFine },
    { value: "medium", label: strings.quoteStudioDividerMedium },
    { value: "bold", label: strings.quoteStudioDividerBold },
  ];
  const styleChoices: Array<{ value: DividerStyle; label: string }> = [
    { value: "solid", label: strings.quoteStudioDividerSolid },
    { value: "dashed", label: strings.quoteStudioDividerDashed },
    { value: "dotted", label: strings.quoteStudioDividerDotted },
  ];
  const widthChoices: DividerWidth[] = [25, 50, 75, 100];

  return (
    <>
      <div className="flex min-h-16 items-center px-4 py-6">
        <DividerLine block={{ ...block, thickness, style, width, color }} />
      </div>
      {open && (
        <Modal
          title={strings.quoteStudioDividerSettings}
          icon={<Minus className="size-5" />}
          onClose={() => onOpenChange(false)}
          wide
          footer={
            <>
              <p className="mr-auto text-xs text-secondary">
                {strings.quoteStudioChangesImmediate}
              </p>
              <Button onClick={() => onOpenChange(false)}>
                {strings.quoteStudioDone}
              </Button>
            </>
          }
        >
          <div>
            <h3 className="text-base font-semibold text-primary">
              {strings.quoteStudioDividerAppearance}
            </h3>
            <p className="mt-1 text-sm text-secondary">
              {strings.quoteStudioDividerAppearanceHelp}
            </p>
          </div>

          <fieldset className="mt-6">
            <legend className="text-sm font-semibold text-primary">
              {strings.quoteStudioDividerStyle}
            </legend>
            <div className="mt-3 grid grid-cols-3 gap-3">
              {styleChoices.map((choice) => (
                <DividerVisualChoice
                  key={choice.value}
                  label={choice.label}
                  selected={choice.value === style}
                  onClick={() => onChange({ style: choice.value })}
                >
                  <DividerLine
                    block={{ ...block, style: choice.value, width: 100, color }}
                  />
                </DividerVisualChoice>
              ))}
            </div>
          </fieldset>

          <div className="mt-6 grid gap-6 md:grid-cols-2">
            <fieldset>
              <legend className="text-sm font-semibold text-primary">
                {strings.quoteStudioDividerThickness}
              </legend>
              <div className="mt-3 grid grid-cols-3 gap-3">
                {thicknessChoices.map((choice) => (
                  <DividerVisualChoice
                    key={choice.value}
                    label={choice.label}
                    selected={choice.value === thickness}
                    compact
                    onClick={() => onChange({ thickness: choice.value })}
                  >
                    <DividerLine
                      block={{
                        ...block,
                        thickness: choice.value,
                        width: 100,
                        color,
                      }}
                    />
                  </DividerVisualChoice>
                ))}
              </div>
            </fieldset>

            <fieldset>
              <legend className="text-sm font-semibold text-primary">
                {strings.quoteStudioDividerWidth}
              </legend>
              <div className="mt-3 grid grid-cols-4 gap-3">
                {widthChoices.map((choice) => (
                  <DividerVisualChoice
                    key={choice}
                    label={`${choice}%`}
                    selected={choice === width}
                    compact
                    onClick={() => onChange({ width: choice })}
                  >
                    <DividerLine block={{ ...block, width: choice, color }} />
                  </DividerVisualChoice>
                ))}
              </div>
            </fieldset>
          </div>

          <div className="mt-6 border-t border-subtle pt-6">
            <p className="text-sm font-semibold text-primary">
              {strings.quoteStudioDividerColour}
            </p>
            <div className="mt-3 flex w-full items-center gap-4 rounded-xl border border-default bg-surface px-4 py-3">
              <ColorPicker
                label={strings.quoteStudioChooseDividerColour}
                value={color}
                onChange={(next) => onChange({ color: next })}
                triggerClassName="!size-12 !rounded-xl"
              />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium text-primary">
                  {strings.quoteStudioDividerColour}
                </span>
                <span className="mt-1 block font-mono text-xs uppercase text-secondary">
                  {color}
                </span>
              </span>
            </div>
          </div>

          <div className="mt-6 rounded-xl bg-raised px-5 py-7">
            <DividerLine block={{ ...block, thickness, style, width, color }} />
          </div>
        </Modal>
      )}
    </>
  );
}

function DividerVisualChoice({
  label,
  selected,
  compact = false,
  onClick,
  children,
}: {
  label: string;
  selected: boolean;
  compact?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      className={cx(
        "flex flex-col rounded-xl border px-3 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
        compact ? "min-h-20" : "min-h-24",
        selected
          ? "border-accent bg-accent-soft text-accent"
          : "border-default bg-surface text-primary hover:border-accent hover:bg-accent-soft/30",
      )}
      onClick={onClick}
    >
      <span className="flex min-h-8 w-full items-center" aria-hidden="true">
        {children}
      </span>
      <span className="mt-auto text-sm font-medium">{label}</span>
    </button>
  );
}

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
          <p className="text-sm font-semibold text-primary">
            {strings.quoteStudioListLayout}
          </p>
          <p className="mt-0.5 text-xs text-secondary">
            {strings.quoteStudioListLayoutHelp}
          </p>
        </div>
        <div className="flex items-center gap-2 text-xs font-semibold text-secondary">
          <span>{strings.quoteStudioColumns}</span>
          <div className="w-36">
            <ChoicePicker
              value={String(columns)}
              label={
                ordered
                  ? strings.quoteStudioNumberedListColumns
                  : strings.quoteStudioBulletListColumns
              }
              placeholder={strings.quoteStudioChooseColumns}
              options={[
                { value: "1", label: strings.quoteStudioColumnCount(1) },
                { value: "2", label: strings.quoteStudioColumnCount(2) },
                { value: "3", label: strings.quoteStudioColumnCount(3) },
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
              aria-label={
                ordered
                  ? strings.quoteStudioNumberedItemA11y(index + 1)
                  : strings.quoteStudioBulletItemA11y(index + 1)
              }
              placeholder={strings.quoteStudioWriteItem}
              onChange={(value) => replace(index, value)}
            />
            <div className="flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover/list-item:opacity-100 group-focus-within/list-item:opacity-100 max-md:col-span-2 max-md:justify-self-end max-md:opacity-100">
              <BlockCommand
                label={strings.quoteStudioMoveItemUp}
                disabled={index === 0}
                onClick={() => move(index, -1)}
              >
                <ArrowUp className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioMoveItemDown}
                disabled={index === rows.length - 1}
                onClick={() => move(index, 1)}
              >
                <ArrowDown className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioRemoveItem}
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
        <Plus className="size-4" aria-hidden="true" />{" "}
        {strings.quoteStudioAddItemBelow}
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
          aria-label={strings.quoteStudioListItemFormatting}
          onMouseDown={(event) => event.preventDefault()}
        >
          <RichTextCommand
            label={strings.quoteStudioBold}
            onClick={() => command("bold")}
          >
            <Bold className="size-4" />
          </RichTextCommand>
          <RichTextCommand
            label={strings.quoteStudioItalic}
            onClick={() => command("italic")}
          >
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
        alt={block.caption || strings.quoteStudioQuotationImageAlt}
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
      title={strings.quoteStudioEditContentBlock}
      icon={<ImagePlus className="size-5" />}
      onClose={onClose}
      wide="extra"
      footer={
        <>
          <p className="mr-auto text-xs text-secondary">
            {strings.quoteStudioChangesImmediate}
          </p>
          <Button onClick={onClose}>{strings.quoteStudioDone}</Button>
        </>
      }
    >
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h3 className="text-base font-semibold text-primary">
          {strings.quoteStudioComposeImageText}
        </h3>
        <p className="w-full text-sm text-secondary">
          {strings.quoteStudioComposeImageTextHelp}
        </p>
      </div>
      <section className="border-y border-subtle py-5">
        <h4 className="text-sm font-semibold text-primary">
          {strings.quoteStudioLayoutTools}
        </h4>
        <p className="mt-1 text-xs text-secondary">
          {strings.quoteStudioLayoutToolsHelp}
        </p>
        <div className="mt-5 flex flex-col gap-5">
          <div className="grid items-start gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,5fr)]">
            <div>
              <ImageOptionGroup
                label={strings.quoteStudioComposition}
                visual="composition"
                value={block.placement ?? "full"}
                options={[
                  ["full", strings.quoteStudioBelowImage],
                  ["left", strings.quoteStudioImageLeft],
                  ["right", strings.quoteStudioImageRight],
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
                label={strings.quoteStudioImageFrame}
                visual="frame"
                value={block.aspect ?? "landscape"}
                options={[
                  ["natural", strings.quoteStudioNatural],
                  ["landscape", strings.quoteStudioWide],
                  ["square", strings.quoteStudioSquare],
                ]}
                onChange={(aspect) => onChange({ aspect })}
              />
            </div>
            <div>
              <ImageOptionGroup
                label={strings.quoteStudioFit}
                visual="fit"
                value={block.fit ?? "cover"}
                options={[
                  ["cover", strings.quoteStudioFillFrame],
                  ["contain", strings.quoteStudioWholeImage],
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
            <h4 className="text-sm font-semibold text-primary">
              {strings.quoteStudioImage}
            </h4>
            <button
              type="button"
              className="inline-flex min-h-9 items-center gap-2 rounded-lg border border-default bg-surface px-3 text-xs font-semibold text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
              onClick={onReplace}
            >
              <Upload className="size-4" aria-hidden="true" />{" "}
              {strings.quoteStudioReplace}
            </button>
          </div>
          <div className="rounded-2xl border border-default bg-surface p-3 shadow-sm">
            <QuotationBlockImage block={block} />
          </div>
        </section>
        <section className="min-w-0">
          <RichTextEditor
            value={block.body ?? ""}
            placeholder={strings.quoteStudioImageDescriptionPlaceholder}
            onChange={(body) => onChange({ body })}
          />
          <div className="mt-4">
            <RichTextEditor
              value={block.caption}
              label={strings.quoteStudioCaption}
              placeholder={strings.quoteStudioCaptionPlaceholder}
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
  label = strings.quoteStudioSupportingText,
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
          <Type className="size-4" aria-hidden="true" />{" "}
          {strings.quoteStudioTextTools}
        </button>
      </div>
      <div className="relative">
        {showTools && (
          <div
            className="absolute -top-12 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-xl border border-default bg-surface p-1.5 shadow-lg"
            role="toolbar"
            aria-label={strings.quoteStudioTextFormatting}
            onMouseDown={(event) => event.preventDefault()}
          >
            <RichTextCommand
              label={strings.quoteStudioBold}
              onClick={() => command("bold")}
            >
              <Bold className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioItalic}
              onClick={() => command("italic")}
            >
              <Italic className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioHeading1}
              onClick={() => command("formatBlock", "h1")}
            >
              <Heading1 className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioHeading2}
              onClick={() => command("formatBlock", "h2")}
            >
              <Heading2 className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioHeading3}
              onClick={() => command("formatBlock", "h3")}
            >
              <Heading3 className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioParagraph}
              onClick={() => command("formatBlock", "p")}
            >
              <Pilcrow className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioBulletList}
              onClick={() => command("insertUnorderedList")}
            >
              <List className="size-4" />
            </RichTextCommand>
            <RichTextCommand
              label={strings.quoteStudioNumberedList}
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
      <legend className="sr-only">{strings.quoteStudioColumnWidth}</legend>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-semibold uppercase tracking-wide text-tertiary">
          {strings.quoteStudioColumnWidth}
        </p>
        {disabled && (
          <span className="text-[11px] text-tertiary">
            {strings.quoteStudioSideBySideOnly}
          </span>
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
              "group relative whitespace-nowrap border text-center text-sm font-medium transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent",
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
            {strings.quoteStudioZoom}
          </h4>
        </div>
        <button
          type="button"
          className="rounded-md px-2 py-1 text-xs font-semibold text-secondary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-default disabled:opacity-35"
          disabled={value === 100}
          onClick={() => onChange(100)}
        >
          {strings.quoteStudioReset}
        </button>
      </div>
      <div className="mt-2 grid grid-cols-[2.5rem_minmax(0,1fr)_2.5rem] items-center gap-1 rounded-xl border border-default bg-raised/60 p-1 shadow-sm">
        <button
          type="button"
          className="grid size-10 place-items-center rounded-lg text-primary transition-colors hover:bg-accent-soft hover:text-accent disabled:cursor-not-allowed disabled:opacity-35"
          aria-label={strings.quoteStudioZoomOut}
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
          aria-label={strings.quoteStudioZoomIn}
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
      columns.push({
        id: crypto.randomUUID(),
        label: strings.quoteStudioColumnNumber(number),
      });
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
            {strings.quoteStudioInformationTable}
          </h3>
          <p className="mt-1 text-xs text-secondary">
            {strings.quoteStudioInformationTableHelp}
          </p>
        </div>
        <label className="grid min-w-40 gap-1 text-xs font-semibold text-secondary">
          {strings.quoteStudioColumns}
          <Select
            fullWidth
            value={String(block.columns.length)}
            aria-label={strings.quoteStudioTableColumnCount}
            onChange={(event) => setColumnCount(Number(event.target.value))}
          >
            {[1, 2, 3, 4, 5, 6].map((count) => (
              <option key={count} value={count}>
                {strings.quoteStudioColumnCount(count)}
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
                      aria-label={strings.quoteStudioColumnNameA11y(
                        columnIndex + 1,
                      )}
                      placeholder={strings.quoteStudioColumnNumber(
                        columnIndex + 1,
                      )}
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
              <th className="w-12" aria-label={strings.quoteStudioRowActions} />
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row, rowIndex) => (
              <tr
                key={row.id}
                className="group/table-row border-t border-default"
              >
                {block.columns.map((column, columnIndex) => (
                  <td
                    key={column.id}
                    className="border-r border-default p-2 last:border-r-0"
                  >
                    <InlineRichTextEditor
                      value={row.cells[column.id] ?? ""}
                      aria-label={strings.quoteStudioTableCellA11y(
                        column.label ||
                          strings.quoteStudioColumnNumber(columnIndex + 1),
                        rowIndex + 1,
                      )}
                      placeholder={strings.quoteStudioEnterValue}
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
            {strings.quoteStudioAddFirstRow}
          </div>
        )}
      </div>
      <button
        type="button"
        className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
        onClick={addRow}
      >
        <Plus className="size-4" aria-hidden="true" />{" "}
        {strings.quoteStudioAddRowBelow}
      </button>
    </div>
  );
}

function blockName(block: Block): string {
  switch (block.kind) {
    case "heading":
      return strings.quoteStudioHeading;
    case "paragraph":
      return strings.quoteStudioParagraph;
    case "quote":
      return strings.quoteStudioQuote;
    case "list":
      return block.ordered
        ? strings.quoteStudioNumberedList
        : strings.quoteStudioBulletList;
    case "divider":
      return strings.quoteStudioDivider;
    case "image":
      return strings.quoteStudioImage;
    case "pricing":
      return strings.quoteStudioPricingTable;
    case "table":
      return strings.quoteStudioTable;
    default:
      return strings.quoteStudioCategoryText;
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
    {
      label: strings.quoteStudioHeading,
      help: strings.quoteStudioHeadingHelp,
      category: "text",
      Icon: Type,
      action: () => add("heading"),
    },
    {
      label: strings.quoteStudioParagraph,
      help: strings.quoteStudioParagraphHelp,
      category: "text",
      Icon: AlignLeft,
      action: () => add("paragraph"),
    },
    {
      label: strings.quoteStudioQuote,
      help: strings.quoteStudioQuoteHelp,
      category: "text",
      Icon: Quote,
      action: () => add("quote"),
    },
    {
      label: strings.quoteStudioBulletList,
      help: strings.quoteStudioBulletListHelp,
      category: "text",
      Icon: List,
      action: () => add("list"),
    },
    {
      label: strings.quoteStudioNumberedList,
      help: strings.quoteStudioNumberedListHelp,
      category: "text",
      Icon: ListOrdered,
      action: () => add("list", true),
    },
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
    {
      label: strings.quoteStudioPricingTable,
      help: strings.quoteStudioPricingTableHelp,
      category: "tables",
      Icon: Table2,
      action: () => add("pricing"),
    },
    {
      label: strings.quoteStudioTable,
      help: strings.quoteStudioTableHelp,
      category: "tables",
      Icon: Rows3,
      action: () => add("table"),
    },
    {
      label: strings.quoteStudioDivider,
      help: strings.quoteStudioDividerHelp,
      category: "layout",
      Icon: Minus,
      action: () => add("divider"),
    },
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
    `${option.label} ${option.help} ${option.category}`
      .toLowerCase()
      .includes(normalizedQuery),
  );
  return (
    <div
      className="relative flex flex-col items-center py-2"
      aria-label={strings.quoteStudioAddContentA11y}
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
          aria-label={strings.quoteStudioAddContentBelow}
          onClick={() => setOpen((value) => !value)}
        >
          <span className="grid size-6 place-items-center rounded-full bg-accent-soft text-accent transition-colors group-hover:bg-accent group-hover:text-on-accent">
            <Plus className="size-3.5" aria-hidden="true" />
          </span>
          {strings.quoteStudioAddContent}
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
                <h3 className="font-semibold text-primary">
                  {strings.quoteStudioAddToQuotation}
                </h3>
                <p className="mt-0.5 text-sm text-secondary">
                  {strings.quoteStudioAddToQuotationHelp}
                </p>
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
            {(normalizedQuery === "" ? categories : (["results"] as const)).map(
              (section, sectionIndex) => {
                const sectionOptions =
                  section === "results"
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
                      {categoryLabels[section]}
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
              },
            )}
          </div>
          {visibleOptions.length === 0 && (
            <div className="border-t border-default px-5 py-8 text-center">
              <p className="text-sm font-semibold text-primary">
                {strings.quoteStudioNoMatchingBlocks}
              </p>
              <p className="mt-1 text-xs text-secondary">
                {strings.quoteStudioTryAnotherName}
              </p>
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
          {readOnly
            ? strings.quoteStudioNoProposalContent
            : strings.quoteStudioStartQuotationBelow}
        </h3>
        {!readOnly && (
          <p className="mt-1 text-sm text-secondary">
            {strings.quoteStudioFirstBlockHelp}
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
  mode,
  design,
  issuerDetails,
  customerDetails: sourceCustomerDetails,
  saveError,
  onChange,
  onClose,
}: {
  mode: "header" | "document";
  design: Design;
  issuerDetails: HeaderDetails;
  customerDetails: CustomerHeaderDetails;
  saveError: string;
  onChange: React.Dispatch<React.SetStateAction<Design>>;
  onClose: () => void;
}) {
  const logoInput = useRef<HTMLInputElement>(null);
  const themeChoices: Array<{ id: Theme; name: string; help: string }> = [
    {
      id: "modern",
      name: strings.quoteStudioModern,
      help: strings.quoteStudioModernHelp,
    },
    {
      id: "editorial",
      name: strings.quoteStudioEditorial,
      help: strings.quoteStudioEditorialHelp,
    },
    {
      id: "minimal",
      name: strings.quoteStudioMinimal,
      help: strings.quoteStudioMinimalHelp,
    },
  ];
  const headerStyleChoices: Array<{
    id: HeaderStyle;
    name: string;
    help: string;
  }> = [
    {
      id: "signature",
      name: strings.quoteStudioSignature,
      help: strings.quoteStudioSignatureHelp,
    },
    {
      id: "editorial",
      name: strings.quoteStudioEditorial,
      help: strings.quoteStudioHeaderEditorialHelp,
    },
    {
      id: "band",
      name: strings.quoteStudioBrandBand,
      help: strings.quoteStudioBrandBandHelp,
    },
    {
      id: "minimal",
      name: strings.quoteStudioMinimal,
      help: strings.quoteStudioHeaderMinimalHelp,
    },
    {
      id: "stacked",
      name: strings.quoteStudioLogoStack,
      help: strings.quoteStudioLogoStackHelp,
    },
  ];
  const setColor = (name: keyof Colors, value: string) =>
    onChange((current) => ({
      ...current,
      colors: { ...current.colors, [name]: value },
    }));
  const displayedHeaderDetails = design.headerDetailsCustomized
    ? design.headerDetails
    : issuerDetails;
  const setHeaderDetail = (name: keyof HeaderDetails, value: string) =>
    onChange((current) => ({
      ...current,
      headerDetails: { ...displayedHeaderDetails, [name]: value },
      headerDetailsCustomized: true,
    }));
  const displayedCustomerDetails = design.customerDetailsCustomized
    ? design.customerDetails
    : sourceCustomerDetails;
  const setCustomerDetail = (
    name: keyof CustomerHeaderDetails,
    value: string,
  ) =>
    onChange((current) => ({
      ...current,
      customerDetails: { ...displayedCustomerDetails, [name]: value },
      customerDetailsCustomized: true,
    }));
  return (
    <Modal
      title={
        mode === "header"
          ? strings.quoteStudioEditQuotationHeader
          : strings.quoteStudioCustomizeQuotation
      }
      icon={
        mode === "header" ? (
          <Building2 className="size-5" />
        ) : (
          <Palette className="size-5" />
        )
      }
      onClose={onClose}
      wide="extra"
      actions={
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-raised hover:text-primary"
          aria-label={strings.quoteStudioClose}
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      }
      footer={
        <div className="flex w-full items-center gap-3 px-1">
          <p
            className={cx(
              "mr-auto text-xs",
              saveError ? "text-danger" : "text-secondary",
            )}
          >
            {saveError || strings.quoteStudioChangesSavedAutomatically}
          </p>
          <Button onClick={onClose}>{strings.quoteStudioDone}</Button>
        </div>
      }
    >
      <div className="space-y-7 p-2">
        {mode === "header" && (
          <>
            <section className="flex flex-wrap items-center gap-5 rounded-2xl border border-default bg-raised/35 p-5">
              <div className="min-w-52 flex-1">
                <h3 className="text-base font-semibold text-primary">
                  {strings.quoteStudioBrandMark}
                </h3>
                <p className="mt-1 text-sm leading-relaxed text-secondary">
                  {strings.quoteStudioBrandMarkHelp}
                </p>
              </div>
              <button
                type="button"
                className="flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-default bg-surface p-3 text-sm font-semibold text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
                onClick={() => logoInput.current?.click()}
              >
                {design.logo ? (
                  <img
                    src={design.logo}
                    alt={strings.quoteStudioQuoteLogo}
                    className="max-h-20 max-w-full object-contain"
                  />
                ) : (
                  <span className="flex flex-col items-center gap-3 text-center">
                    <span className="grid size-10 place-items-center rounded-xl bg-accent-soft text-accent">
                      <Upload className="size-5" />
                    </span>
                    <span>
                      <strong className="sr-only">
                        {strings.quoteStudioUploadLogo}
                      </strong>
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
                  {design.logo
                    ? strings.quoteStudioReplace
                    : strings.quoteStudioChooseFile}
                </button>
                <button
                  type="button"
                  disabled={!design.logo}
                  className="min-h-9 rounded-lg px-3 text-sm font-semibold text-secondary transition-colors hover:bg-danger-tint hover:text-danger disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-40"
                  onClick={() =>
                    onChange((current) => ({ ...current, logo: "" }))
                  }
                >
                  {strings.quoteStudioRemove}
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
            <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
              <div className="flex flex-wrap items-center justify-between gap-5">
                <div className="flex items-start gap-4">
                  <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                    <QrCode className="size-5" aria-hidden="true" />
                  </span>
                  <div>
                    <h3 className="text-xl font-semibold tracking-tight text-primary">
                      {strings.quoteStudioQrTitle}
                    </h3>
                    <p className="mt-2 text-sm leading-relaxed text-secondary">
                      {strings.quoteStudioQrHelp}
                    </p>
                  </div>
                </div>
                <button
                  type="button"
                  role="switch"
                  aria-checked={design.showContactQr}
                  className={cx(
                    "relative h-7 w-12 shrink-0 rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
                    design.showContactQr
                      ? "border-accent bg-accent"
                      : "border-default bg-raised",
                  )}
                  onClick={() =>
                    onChange((current) => ({
                      ...current,
                      showContactQr: !current.showContactQr,
                    }))
                  }
                >
                  <span
                    className={cx(
                      "absolute top-1 size-5 rounded-full bg-white shadow-sm transition-[left]",
                      design.showContactQr ? "left-6" : "left-1",
                    )}
                  />
                  <span className="sr-only">{strings.quoteStudioShowQr}</span>
                </button>
              </div>
              <div className="mt-7 grid gap-7 xl:grid-cols-2">
                <fieldset>
                  <legend className="text-sm font-semibold text-primary">
                    {strings.quoteStudioPlacement}
                  </legend>
                  <p className="mt-1 text-xs text-secondary">
                    {strings.quoteStudioPlacementHelp}
                  </p>
                  <div className="mt-4 grid grid-cols-2 gap-3">
                    {(["left", "right"] as const).map((alignment) => (
                      <button
                        key={alignment}
                        type="button"
                        aria-pressed={design.contactQrAlignment === alignment}
                        aria-label={strings.quoteStudioQrPlacementA11y(
                          alignment === "left"
                            ? strings.quoteStudioLeft
                            : strings.quoteStudioRight,
                        )}
                        className={cx(
                          "group relative min-h-24 cursor-pointer rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
                          design.contactQrAlignment === alignment
                            ? "border-accent bg-accent-soft/40"
                            : "border-default bg-surface hover:border-accent hover:bg-accent-soft/20",
                        )}
                        onClick={() =>
                          onChange((current) => ({
                            ...current,
                            showContactQr: true,
                            contactQrAlignment: alignment,
                          }))
                        }
                      >
                        <span
                          className={cx(
                            "flex h-16 items-end gap-3 rounded-lg bg-raised p-3",
                            alignment === "right" && "flex-row-reverse",
                          )}
                          aria-hidden="true"
                        >
                          <span className="grid size-9 shrink-0 place-items-center rounded-md bg-surface text-accent ring-1 ring-default">
                            <QrCode className="size-6" />
                          </span>
                          <span className="mb-1 flex-1 space-y-2">
                            <span className="block h-1.5 w-full rounded-full bg-primary/15" />
                            <span className="block h-1.5 w-2/3 rounded-full bg-primary/10" />
                          </span>
                        </span>
                        <span className="mt-2 block text-center text-xs font-semibold text-primary">
                          {alignment === "left"
                            ? strings.quoteStudioLeft
                            : strings.quoteStudioRight}
                        </span>
                        <span
                          className={cx(
                            "absolute right-2 top-2 grid size-5 place-items-center rounded-full border transition-colors",
                            design.contactQrAlignment === alignment
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                          aria-hidden="true"
                        >
                          {design.contactQrAlignment === alignment && (
                            <Check className="size-3" strokeWidth={3} />
                          )}
                        </span>
                      </button>
                    ))}
                  </div>
                </fieldset>
                <fieldset>
                  <legend className="text-sm font-semibold text-primary">
                    {strings.quoteStudioSize}
                  </legend>
                  <p className="mt-1 text-xs text-secondary">
                    {strings.quoteStudioSizeHelp}
                  </p>
                  <div className="mt-4 grid grid-cols-3 gap-3">
                    {(["small", "medium", "large"] as const).map((size) => (
                      <button
                        key={size}
                        type="button"
                        aria-pressed={design.contactQrSize === size}
                        className={cx(
                          "group relative min-h-24 cursor-pointer rounded-xl border p-3 transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
                          design.contactQrSize === size
                            ? "border-accent bg-accent-soft/40"
                            : "border-default bg-surface hover:border-accent hover:bg-accent-soft/20",
                        )}
                        onClick={() =>
                          onChange((current) => ({
                            ...current,
                            showContactQr: true,
                            contactQrSize: size,
                          }))
                        }
                      >
                        <span
                          className="flex h-12 items-center justify-center"
                          aria-hidden="true"
                        >
                          <QrCode
                            className={cx(
                              "text-accent",
                              size === "small"
                                ? "size-6"
                                : size === "large"
                                  ? "size-11"
                                  : "size-8",
                            )}
                          />
                        </span>
                        <span className="mt-2 block text-center text-xs font-semibold text-primary">
                          {size === "small"
                            ? strings.quoteStudioSmall
                            : size === "medium"
                              ? strings.quoteStudioMedium
                              : strings.quoteStudioLarge}
                        </span>
                        <span
                          className={cx(
                            "absolute right-2 top-2 grid size-5 place-items-center rounded-full border transition-colors",
                            design.contactQrSize === size
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                          aria-hidden="true"
                        >
                          {design.contactQrSize === size && (
                            <Check className="size-3" strokeWidth={3} />
                          )}
                        </span>
                      </button>
                    ))}
                  </div>
                </fieldset>
                <div className="xl:col-span-2">
                  <ColorField
                    label={strings.quoteStudioQrColour}
                    help={strings.quoteStudioQrColourHelp}
                    value={design.contactQrColor}
                    onChange={(contactQrColor) =>
                      onChange((current) => ({
                        ...current,
                        showContactQr: true,
                        contactQrColor,
                      }))
                    }
                  />
                </div>
              </div>
            </section>
            <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
              <div>
                <div className="flex flex-wrap items-center justify-between gap-5">
                  <div className="flex items-start gap-4">
                    <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                      <Building2 className="size-5" aria-hidden="true" />
                    </span>
                    <div>
                      <h3 className="text-xl font-semibold tracking-tight text-primary">
                        {strings.quoteStudioCompanyInformation}
                      </h3>
                      <p className="mt-2 text-sm leading-relaxed text-secondary">
                        {strings.quoteStudioCompanyLinkedHelp}
                        <span className="block">
                          {strings.quoteStudioOverrideHelp}
                        </span>
                      </p>
                    </div>
                  </div>
                  {design.headerDetailsCustomized ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      icon={<RotateCcw aria-hidden="true" />}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                          headerDetailsCustomized: false,
                        }))
                      }
                    >
                      {strings.quoteStudioUseYourDetails}
                    </Button>
                  ) : (
                    <span className="inline-flex min-h-10 items-center gap-2 rounded-full bg-accent-soft px-4 text-sm font-semibold text-accent">
                      <Link className="size-4" aria-hidden="true" />
                      {strings.quoteStudioLinkedYourDetails}
                    </span>
                  )}
                </div>
              </div>
              <div className="mt-8 grid gap-x-8 gap-y-5 sm:grid-cols-2">
                <HeaderField
                  label={strings.quoteStudioCompanyName}
                  icon={<Building2 />}
                  value={displayedHeaderDetails.companyName}
                  placeholder={strings.quoteStudioCompanyNamePlaceholder}
                  onChange={(value) => setHeaderDetail("companyName", value)}
                />
                <HeaderField
                  label={strings.quoteStudioWebsite}
                  icon={<Globe2 />}
                  value={displayedHeaderDetails.website}
                  placeholder={strings.quoteStudioWebsitePlaceholder}
                  onChange={(value) => setHeaderDetail("website", value)}
                />
                <HeaderField
                  label={strings.quoteStudioEmail}
                  icon={<Mail />}
                  value={displayedHeaderDetails.email}
                  placeholder={strings.quoteStudioEmailPlaceholder}
                  onChange={(value) => setHeaderDetail("email", value)}
                />
                <HeaderField
                  label={strings.quoteStudioPhone}
                  icon={<Phone />}
                  value={displayedHeaderDetails.phone}
                  placeholder={strings.quoteStudioPhonePlaceholder}
                  onChange={(value) => setHeaderDetail("phone", value)}
                />
                <label className="grid gap-2 sm:col-span-2">
                  <span className="text-sm font-semibold text-primary">
                    {strings.quoteStudioAddress}
                  </span>
                  <textarea
                    className="min-h-32 resize-y rounded-xl border border-default bg-surface px-4 py-4 text-base leading-relaxed text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10"
                    value={displayedHeaderDetails.address}
                    placeholder={strings.quoteStudioAddressPlaceholder}
                    onChange={(event) =>
                      setHeaderDetail("address", event.target.value)
                    }
                  />
                </label>
                <HeaderField
                  label={strings.quoteStudioVatId}
                  value={displayedHeaderDetails.vatId}
                  placeholder={strings.quoteStudioVatPlaceholder}
                  onChange={(value) => setHeaderDetail("vatId", value)}
                />
                <HeaderField
                  label={strings.quoteStudioCompanyNumber}
                  value={displayedHeaderDetails.registrationNo}
                  placeholder={strings.quoteStudioCompanyNumberPlaceholder}
                  onChange={(value) => setHeaderDetail("registrationNo", value)}
                />
              </div>
            </section>
            <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
              <div className="flex flex-wrap items-center justify-between gap-5">
                <div className="flex items-start gap-4">
                  <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                    <ContactRound className="size-5" aria-hidden="true" />
                  </span>
                  <div>
                    <h3 className="text-xl font-semibold tracking-tight text-primary">
                      {strings.quoteStudioCustomerInformation}
                    </h3>
                    <p className="mt-2 text-sm leading-relaxed text-secondary">
                      {strings.quoteStudioCustomerInformationHelp}
                      <span className="block">
                        {strings.quoteStudioCustomerOverrideHelp}
                      </span>
                    </p>
                  </div>
                </div>
                {design.customerDetailsCustomized ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<RotateCcw aria-hidden="true" />}
                    onClick={() =>
                      onChange((current) => ({
                        ...current,
                        customerDetailsCustomized: false,
                      }))
                    }
                  >
                    {strings.quoteStudioUseSelectedCustomer}
                  </Button>
                ) : (
                  <span className="inline-flex min-h-10 items-center gap-2 rounded-full bg-accent-soft px-4 text-sm font-semibold text-accent">
                    <Link className="size-4" aria-hidden="true" />
                    {strings.quoteStudioLinkedSelectedCustomer}
                  </span>
                )}
              </div>
              <div className="mt-8 grid gap-x-8 gap-y-5 sm:grid-cols-2">
                <HeaderField
                  label={strings.quoteStudioCompanyName}
                  icon={<Building2 />}
                  value={displayedCustomerDetails.companyName}
                  placeholder={strings.quoteStudioCustomerCompanyPlaceholder}
                  onChange={(value) => setCustomerDetail("companyName", value)}
                />
                <HeaderField
                  label={strings.quoteStudioContactPerson}
                  icon={<ContactRound />}
                  value={displayedCustomerDetails.contactName}
                  placeholder={strings.quoteStudioContactNamePlaceholder}
                  onChange={(value) => setCustomerDetail("contactName", value)}
                />
                <HeaderField
                  label={strings.quoteStudioEmail}
                  icon={<Mail />}
                  value={displayedCustomerDetails.email}
                  placeholder={strings.quoteStudioCustomerEmailPlaceholder}
                  onChange={(value) => setCustomerDetail("email", value)}
                />
                <HeaderField
                  label={strings.quoteStudioPhone}
                  icon={<Phone />}
                  value={displayedCustomerDetails.phone}
                  placeholder={strings.quoteStudioPhonePlaceholder}
                  onChange={(value) => setCustomerDetail("phone", value)}
                />
                <label className="grid gap-2 sm:col-span-2">
                  <span className="text-sm font-semibold text-primary">
                    {strings.quoteStudioAddress}
                  </span>
                  <textarea
                    className="min-h-32 resize-y rounded-xl border border-default bg-surface px-4 py-4 text-base leading-relaxed text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10"
                    value={displayedCustomerDetails.address}
                    placeholder={strings.quoteStudioAddressPlaceholder}
                    onChange={(event) =>
                      setCustomerDetail("address", event.target.value)
                    }
                  />
                </label>
                <HeaderField
                  label={strings.quoteStudioVatId}
                  value={displayedCustomerDetails.vatId}
                  placeholder={strings.quoteStudioCustomerVatPlaceholder}
                  onChange={(value) => setCustomerDetail("vatId", value)}
                />
              </div>
            </section>
          </>
        )}
        <div className="min-w-0 space-y-7">
          {mode === "header" && (
            <>
              <section>
                <h3 className="text-base font-semibold text-primary">
                  {strings.quoteStudioHeaderStyle}
                </h3>
                <p className="mt-1 text-sm text-secondary">
                  {strings.quoteStudioHeaderStyleHelp}
                </p>
                <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-5">
                  {headerStyleChoices.map((choice) => (
                    <button
                      key={choice.id}
                      type="button"
                      aria-pressed={design.headerStyle === choice.id}
                      className={cx(
                        "relative min-h-40 rounded-2xl border p-4 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                        design.headerStyle === choice.id
                          ? "border-accent bg-accent-soft/25"
                          : "border-default bg-surface",
                      )}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                          headerStyle: choice.id,
                        }))
                      }
                    >
                      <HeaderStylePreview style={choice.id} />
                      <span className="mt-4 flex items-start justify-between gap-3">
                        <span>
                          <strong className="block text-sm font-semibold text-primary">
                            {choice.name}
                          </strong>
                          <small className="mt-1 block text-xs leading-relaxed text-secondary">
                            {choice.help}
                          </small>
                        </span>
                        <span
                          className={cx(
                            "grid size-5 shrink-0 place-items-center rounded-full border",
                            design.headerStyle === choice.id
                              ? "border-accent bg-accent text-white"
                              : "border-default",
                          )}
                        >
                          {design.headerStyle === choice.id && (
                            <Check className="size-3" />
                          )}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
              <section>
                <div>
                  <h3 className="text-base font-semibold text-primary">
                    {strings.quoteStudioHeaderArrangement}
                  </h3>
                  <p className="mt-1 text-sm text-secondary">
                    {strings.quoteStudioHeaderArrangementHelp}
                  </p>
                </div>
                <div className="mt-4 grid gap-4 sm:grid-cols-2">
                  {(["left", "right"] as const).map((alignment) => (
                    <button
                      key={alignment}
                      type="button"
                      aria-pressed={design.headerAlignment === alignment}
                      className={cx(
                        "group relative min-h-40 overflow-hidden rounded-2xl border !p-5 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                        design.headerAlignment === alignment
                          ? "border-accent bg-accent-soft/30"
                          : "border-default bg-surface",
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
                      <span className="flex items-start justify-between gap-5 pt-5">
                        <span>
                          <strong className="block text-sm font-semibold text-primary">
                            {alignment === "left"
                              ? strings.quoteStudioLogoLeft
                              : strings.quoteStudioLogoRight}
                          </strong>
                          <small className="mt-1 block text-xs font-normal leading-relaxed text-secondary">
                            {alignment === "left"
                              ? strings.quoteStudioLogoLeftHelp
                              : strings.quoteStudioLogoRightHelp}
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
              <section className="border-t border-subtle pt-7">
                <div>
                  <h3 className="text-base font-semibold text-primary">
                    {strings.quoteStudioColumnBalance}
                  </h3>
                  <p className="mt-1 text-sm text-secondary">
                    {strings.quoteStudioColumnBalanceHelp}
                  </p>
                </div>
                <div
                  className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-3"
                  role="radiogroup"
                  aria-label={strings.quoteStudioColumnBalanceA11y}
                >
                  {headerRatioChoices.map((choice) => {
                    const selected = design.headerRatio === choice.id;
                    const [company = "50", customer = "50"] =
                      choice.id.split("-");
                    return (
                      <button
                        key={choice.id}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        aria-label={strings.quoteStudioColumnRatioA11y(
                          company,
                          customer,
                        )}
                        className={cx(
                          "group relative rounded-2xl p-3 transition-colors hover:bg-accent-soft/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                          selected && "bg-accent-soft/40",
                        )}
                        onClick={() =>
                          onChange((current) => ({
                            ...current,
                            headerRatio: choice.id,
                          }))
                        }
                      >
                        <span
                          className={cx(
                            "grid h-24 gap-2 rounded-xl bg-raised p-3",
                            design.headerAlignment === "left"
                              ? choice.columns
                              : choice.reverseColumns,
                          )}
                          aria-hidden="true"
                        >
                          <span
                            className={cx(
                              "flex items-center justify-center rounded-lg bg-surface text-primary",
                              design.headerAlignment === "right" && "order-2",
                              selected && "ring-1 ring-accent/30",
                            )}
                          >
                            <Building2 className="size-6" strokeWidth={1.7} />
                          </span>
                          <span
                            className={cx(
                              "flex items-center justify-center rounded-lg bg-surface text-accent",
                              design.headerAlignment === "right" && "order-1",
                              selected && "ring-1 ring-accent/30",
                            )}
                          >
                            <ContactRound
                              className="size-6"
                              strokeWidth={1.7}
                            />
                          </span>
                        </span>
                        <span
                          className={cx(
                            "absolute right-2 top-2 grid size-6 place-items-center rounded-full border",
                            selected
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                          aria-hidden="true"
                        >
                          {selected && <Check className="size-3" />}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </section>
            </>
          )}
          {mode === "document" && (
            <>
              <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div>
                    <h3 className="text-2xl font-semibold tracking-tight text-primary">
                      {strings.quoteStudioDocumentPalette}
                    </h3>
                    <p className="mt-2 text-base text-secondary">
                      {strings.quoteStudioDocumentPaletteHelp}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<RotateCcw aria-hidden="true" />}
                    onClick={() =>
                      onChange((current) => ({
                        ...current,
                        colors: DEFAULT_COLORS,
                      }))
                    }
                  >
                    {strings.quoteStudioResetDefaults}
                  </Button>
                </div>
                <div className="mt-8 grid gap-8 xl:grid-cols-2 xl:gap-0">
                  <div>
                    <div className="mb-6 flex items-center gap-4">
                      <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                        <FileText className="size-5" aria-hidden="true" />
                      </span>
                      <div>
                        <h4 className="text-base font-semibold text-primary">
                          {strings.quoteStudioDocument}
                        </h4>
                        <p className="mt-1 text-sm text-secondary">
                          {strings.quoteStudioDocumentHelp}
                        </p>
                      </div>
                    </div>
                    <div className="grid gap-3">
                      <ColorField
                        label={strings.quoteStudioAccent}
                        help={strings.quoteStudioAccentHelp}
                        value={design.colors.accent}
                        onChange={(value) => setColor("accent", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioContactIcons}
                        help={strings.quoteStudioContactIconsHelp}
                        value={design.colors.contactIcons}
                        onChange={(value) => setColor("contactIcons", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioPage}
                        help={strings.quoteStudioPageHelp}
                        value={design.colors.background}
                        onChange={(value) => setColor("background", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioHeader}
                        help={strings.quoteStudioHeaderHelp}
                        value={design.colors.headerBackground}
                        onChange={(value) =>
                          setColor("headerBackground", value)
                        }
                      />
                      <ColorField
                        label={strings.quoteStudioText}
                        help={strings.quoteStudioTextHelp}
                        value={design.colors.text}
                        onChange={(value) => setColor("text", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioBulletDots}
                        help={strings.quoteStudioListMarkers}
                        value={design.colors.bulletMarker}
                        onChange={(value) => setColor("bulletMarker", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioNumberMarkers}
                        help={strings.quoteStudioNumberedSteps}
                        value={design.colors.numberMarker}
                        onChange={(value) => setColor("numberMarker", value)}
                      />
                    </div>
                  </div>
                  <div className="border-t border-subtle pt-8 xl:border-l xl:border-t-0 xl:pl-8 xl:pt-0">
                    <div className="mb-6 flex items-center gap-4">
                      <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                        <Table2 className="size-5" aria-hidden="true" />
                      </span>
                      <div>
                        <h4 className="text-base font-semibold text-primary">
                          {strings.quoteStudioPricingTables}
                        </h4>
                        <p className="mt-1 text-sm text-secondary">
                          {strings.quoteStudioPricingTablesHelp}
                        </p>
                      </div>
                    </div>
                    <div className="grid gap-3">
                      <ColorField
                        label={strings.quoteStudioTableHeading}
                        help={strings.quoteStudioTableHeadingHelp}
                        value={design.colors.tableHeader}
                        onChange={(value) => setColor("tableHeader", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioTableRows}
                        help={strings.quoteStudioTableRowsHelp}
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
                    {strings.quoteStudioTypography}
                  </h3>
                  <p className="mt-1 text-sm text-secondary">
                    {strings.quoteStudioTypographyHelp}
                  </p>
                </div>
                <div className="mt-5 grid gap-4 sm:grid-cols-3">
                  {themeChoices.map((theme) => (
                    <button
                      key={theme.id}
                      type="button"
                      aria-pressed={design.theme === theme.id}
                      className={cx(
                        "group relative min-h-52 overflow-hidden rounded-2xl border !p-4 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
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
                            theme.id === "modern" &&
                              "font-semibold tracking-tight",
                            theme.id === "editorial" && "font-editorial",
                            theme.id === "minimal" &&
                              "font-light uppercase tracking-[0.14em]",
                          )}
                        >
                          {strings.quoteStudioProposal}
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
            </>
          )}
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
      title={strings.quoteStudioTableSettings}
      icon={<Table2 className="size-5" />}
      onClose={onClose}
      wide
      actions={
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-accent-soft hover:text-accent"
          aria-label={strings.quoteStudioCloseTableSettings}
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
            {saveError || strings.quoteStudioTableChangesSavedAutomatically}
          </p>
          <Button onClick={onClose}>{strings.quoteStudioDone}</Button>
        </>
      }
    >
      <section>
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioChooseLayout}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioChooseLayoutHelp}
        </p>
        <div className="mt-5 grid gap-5 sm:grid-cols-3">
          {(
            [
              [
                "compact",
                strings.quoteStudioCompact,
                strings.quoteStudioCompactHelp,
              ],
              [
                "detailed",
                strings.quoteStudioDetailed,
                strings.quoteStudioDetailedHelp,
              ],
              [
                "catalogue",
                strings.quoteStudioCatalogue,
                strings.quoteStudioCatalogueHelp,
              ],
            ] as const
          ).map(([layout, label, help]) => (
            <button
              key={layout}
              type="button"
              className={cx(
                "group relative min-h-64 overflow-hidden rounded-2xl border bg-surface !p-5 text-left ring-1 transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
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
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioProductContent}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioProductContentHelp}
        </p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <TableToggle
            label={strings.quoteStudioProductImages}
            help={strings.quoteStudioProductImagesHelp}
            checked={design.showProductImages}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showProductImages: !current.showProductImages,
              }))
            }
          />
          <TableToggle
            label={strings.quoteStudioProductDescriptions}
            help={strings.quoteStudioProductDescriptionsHelp}
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
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioVisibleColumns}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioVisibleColumnsHelp}
        </p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {(
            [
              ["unit", strings.quoteStudioUnit],
              ["quantity", strings.quoteStudioQuantity],
              ["unitPrice", strings.quoteStudioUnitPrice],
              ["vat", strings.quoteStudioVatRate],
              ["net", strings.quoteStudioLineTotal],
            ] as const
          ).map(([key, label]) => (
            <TableToggle
              key={key}
              label={label}
              help={strings.quoteStudioShowColumn(label)}
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
          {strings.quoteStudioPricingTableTotals}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioPricingTableTotalsHelp}
        </p>
        <div className="mt-5 grid gap-5 sm:grid-cols-3">
          {(
            [
              [
                "summary",
                strings.quoteStudioSummaryCard,
                strings.quoteStudioSummaryCardHelp,
              ],
              [
                "full",
                strings.quoteStudioFullWidth,
                strings.quoteStudioFullWidthHelp,
              ],
              [
                "footer",
                strings.quoteStudioTableFooter,
                strings.quoteStudioTableFooterHelp,
              ],
            ] as const
          ).map(([placement, label, help]) => (
            <button
              key={placement}
              type="button"
              className={cx(
                "group min-h-40 rounded-2xl border !p-5 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
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
          {strings.quoteStudioAmountDetails}
        </h4>
        <div className="mt-5 grid gap-4 sm:grid-cols-3">
          {(
            [
              [
                "total",
                strings.quoteStudioTotalOnly,
                strings.quoteStudioTotalOnlyHelp,
              ],
              [
                "summary",
                strings.quoteStudioNetVatTotal,
                strings.quoteStudioNetVatTotalHelp,
              ],
              [
                "breakdown",
                strings.quoteStudioVatBreakdown,
                strings.quoteStudioVatBreakdownHelp,
              ],
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
            label={strings.quoteStudioCurrencyCode}
            help={strings.quoteStudioCurrencyCodeHelp}
            checked={design.showCurrencyCode}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showCurrencyCode: !current.showCurrencyCode,
              }))
            }
          />
          <TableToggle
            label={strings.quoteStudioEmphasizeTotal}
            help={strings.quoteStudioEmphasizeTotalHelp}
            checked={design.emphasizeTotal}
            onClick={() =>
              onChange((current) => ({
                ...current,
                emphasizeTotal: !current.emphasizeTotal,
              }))
            }
          />
          <TableToggle
            label={strings.quoteStudioVatNote}
            help={strings.quoteStudioVatNoteHelp}
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
        "flex min-h-24 items-center gap-4 rounded-xl border !px-5 !py-4 text-left transition-colors hover:border-accent hover:bg-accent-soft/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
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
  help,
  value,
  onChange,
}: {
  label: string;
  help: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const fieldId = `quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`;
  return (
    <div className="flex min-h-20 items-center gap-4 rounded-xl border border-default bg-surface px-4 py-3 focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/10">
      <ColorPicker
        label={`Choose ${label.toLowerCase()} colour`}
        value={value}
        onChange={onChange}
        triggerClassName="!size-14 !rounded-xl"
      />
      <div className="min-w-0 flex-1">
        <label
          className="block text-sm font-semibold text-primary"
          htmlFor={fieldId}
        >
          {label}
        </label>
        <p className="mt-1 text-xs text-secondary">{help}</p>
      </div>
      <input
        id={fieldId}
        value={value.toUpperCase()}
        aria-label={`${label} hex colour`}
        className="h-10 w-[6.25rem] shrink-0 rounded-lg border border-default bg-raised px-3 font-mono text-sm font-medium uppercase text-primary outline-none focus:border-accent focus:ring-2 focus:ring-accent/10"
        maxLength={7}
        spellCheck={false}
        onChange={(event) => {
          const next = event.target.value.startsWith("#")
            ? event.target.value
            : `#${event.target.value}`;
          onChange(next.slice(0, 7));
        }}
      />
      <IconButton
        label={`Copy ${label.toLowerCase()} colour`}
        icon={<Copy />}
        onClick={() => void navigator.clipboard?.writeText(value.toUpperCase())}
      />
    </div>
  );
}

function HeaderField({
  label,
  icon,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  icon?: ReactNode;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  const fieldId = `quote-header-${label.replace(/\s+/g, "-").toLowerCase()}`;
  return (
    <label className="grid gap-2" htmlFor={fieldId}>
      <span className="text-sm font-semibold text-primary">{label}</span>
      <span className="relative block">
        <input
          id={fieldId}
          className={cx(
            "h-12 w-full rounded-xl border border-default bg-surface px-4 text-base text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10",
            icon !== undefined && icon !== null && "pr-12",
          )}
          value={value}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
        />
        {icon && (
          <span className="pointer-events-none absolute inset-y-0 right-4 flex items-center text-secondary [&_svg]:size-5">
            {icon}
          </span>
        )}
      </span>
    </label>
  );
}
