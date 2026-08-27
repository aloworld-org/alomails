import { DEFAULT_QUOTE_COLORS, DEFAULT_QUOTE_COLUMNS } from "./QuoteStudioDesign";
import type {
  QuoteCustomerHeaderDetails,
  QuoteHeaderDetails,
  QuoteStudioDesign,
} from "./QuoteStudioDesign";

export const DEFAULT_QUOTE_HEADER_DETAILS: QuoteHeaderDetails = {
  companyName: "",
  address: "",
  email: "",
  phone: "",
  website: "",
  vatId: "",
  registrationNo: "",
};

export const DEFAULT_QUOTE_CUSTOMER_DETAILS: QuoteCustomerHeaderDetails = {
  companyName: "",
  contactName: "",
  address: "",
  email: "",
  phone: "",
  vatId: "",
};

export const EMPTY_QUOTE_STUDIO_DESIGN: QuoteStudioDesign = {
  logo: "",
  headerStyle: "signature",
  headerAlignment: "left",
  headerRatio: "50-50",
  headerDetails: DEFAULT_QUOTE_HEADER_DETAILS,
  headerDetailsCustomized: false,
  customerDetails: DEFAULT_QUOTE_CUSTOMER_DETAILS,
  customerDetailsCustomized: false,
  showContactQr: false,
  contactQrAlignment: "right",
  contactQrSize: "medium",
  contactQrColor: "#102a43",
  theme: "modern",
  colors: DEFAULT_QUOTE_COLORS,
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

export function ensurePricingTable(design: QuoteStudioDesign): QuoteStudioDesign {
  return design.blocks.some((block) => block.kind === "pricing")
    ? design
    : {
        ...design,
        blocks: [
          ...design.blocks,
          { id: "pricing-table", kind: "pricing" },
        ],
      };
}

export function normalizeSavedQuoteDesign(
  saved: Partial<QuoteStudioDesign>,
): QuoteStudioDesign {
  const headerDetails = {
    ...DEFAULT_QUOTE_HEADER_DETAILS,
    ...saved.headerDetails,
  };
  const customerDetails = {
    ...DEFAULT_QUOTE_CUSTOMER_DETAILS,
    ...saved.customerDetails,
  };

  return ensurePricingTable({
    ...EMPTY_QUOTE_STUDIO_DESIGN,
    ...saved,
    colors: { ...DEFAULT_QUOTE_COLORS, ...saved.colors },
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
