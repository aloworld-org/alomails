import type {
  QuoteLineContent,
  QuoteTableLayout,
  QuoteTotalsDetail,
  QuoteTotalsPlacement,
  QuoteTotalsStyle,
} from "../quoteTableOptions";
import type { HeaderStyle } from "./HeaderStylePreview";
import type { QuoteStudioBlock } from "./QuoteStudioBlock";

export type QuoteStudioTheme = "modern" | "editorial" | "minimal";
export type HeaderAlignment = "left" | "right";
export type HeaderRatio = "40-60" | "50-50" | "60-40";
export type ContactQrSize = "small" | "medium" | "large";

export interface QuoteStudioColors {
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

export interface QuoteHeaderDetails {
  companyName: string;
  address: string;
  email: string;
  phone: string;
  website: string;
  vatId: string;
  registrationNo: string;
}

export interface QuoteCustomerHeaderDetails {
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

export const DEFAULT_QUOTE_COLORS: QuoteStudioColors = {
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

export interface QuoteStudioDesign {
  logo: string;
  headerStyle: HeaderStyle;
  headerAlignment: HeaderAlignment;
  headerRatio: HeaderRatio;
  headerDetails: QuoteHeaderDetails;
  headerDetailsCustomized: boolean;
  customerDetails: QuoteCustomerHeaderDetails;
  customerDetailsCustomized: boolean;
  showContactQr: boolean;
  contactQrAlignment: HeaderAlignment;
  contactQrSize: ContactQrSize;
  contactQrColor: string;
  theme: QuoteStudioTheme;
  colors: QuoteStudioColors;
  columns: QuoteColumns;
  tableLayout: QuoteTableLayout;
  showProductImages: boolean;
  showProductDescriptions: boolean;
  lineContent: Record<string, QuoteLineContent>;
  totalsPlacement: QuoteTotalsPlacement;
  totalsDetail: QuoteTotalsDetail;
  totalsStyle: QuoteTotalsStyle;
  showCurrencyCode: boolean;
  emphasizeTotal: boolean;
  showTaxNote: boolean;
  blocks: QuoteStudioBlock[];
}
