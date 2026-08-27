import type { BillingCustomer, BillingSettings } from "../types";
import type {
  HeaderAlignment,
  HeaderRatio,
  QuoteCustomerHeaderDetails,
  QuoteHeaderDetails,
} from "./QuoteStudioDesign";
import {
  DEFAULT_QUOTE_CUSTOMER_DETAILS,
  DEFAULT_QUOTE_HEADER_DETAILS,
} from "./quoteStudioNormalization";

const HEADER_RATIO_CLASSES: Record<HeaderRatio, string> = {
  "40-60": "md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]",
  "50-50": "md:grid-cols-2",
  "60-40": "md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]",
};

export function quotationHeaderRatioClass(
  ratio: HeaderRatio,
  alignment: HeaderAlignment,
) {
  if (alignment === "left" || ratio === "50-50")
    return HEADER_RATIO_CLASSES[ratio];
  return ratio === "40-60"
    ? HEADER_RATIO_CLASSES["60-40"]
    : HEADER_RATIO_CLASSES["40-60"];
}

export function formatQuoteDocumentDate(
  value: string | null | undefined,
  locale: string,
) {
  if (!value) return null;
  const [year, month, day] = value.split("-").map(Number);
  if (!year || !month || !day) return value;
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(Date.UTC(year, month - 1, day)));
}

function escapeVCardValue(value: string) {
  return value
    .replace(/\\/g, "\\\\")
    .replace(/\n/g, "\\n")
    .replace(/;/g, "\\;")
    .replace(/,/g, "\\,");
}

export function createContactVCard(details: QuoteHeaderDetails) {
  return [
    "BEGIN:VCARD",
    "VERSION:3.0",
    `FN:${escapeVCardValue(details.companyName)}`,
    `ORG:${escapeVCardValue(details.companyName)}`,
    details.phone && `TEL;TYPE=WORK,VOICE:${escapeVCardValue(details.phone)}`,
    details.email && `EMAIL;TYPE=WORK:${escapeVCardValue(details.email)}`,
    details.website && `URL:${escapeVCardValue(details.website)}`,
    details.address &&
      `ADR;TYPE=WORK:;;${escapeVCardValue(details.address).replace(/\\n/g, ";")};;;`,
    "END:VCARD",
  ]
    .filter(Boolean)
    .join("\n");
}

export function quoteHeaderDetailsFromSettings(
  settings?: BillingSettings | null,
): QuoteHeaderDetails {
  if (!settings) return DEFAULT_QUOTE_HEADER_DETAILS;
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

export function quoteCustomerDetailsFromCustomer(
  customer?: BillingCustomer | null,
  fallbackName = "",
): QuoteCustomerHeaderDetails {
  if (!customer)
    return { ...DEFAULT_QUOTE_CUSTOMER_DETAILS, companyName: fallbackName };
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
