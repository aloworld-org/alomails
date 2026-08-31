import type { BillingSettings } from "./types";

export function quotationAcceptanceQr(
  issuer: BillingSettings | null,
  number: string | null,
  subject: string,
  body: string,
): string | null {
  if (!issuer?.email.trim() || !number) return null;
  return `mailto:${encodeURIComponent(issuer.email.trim())}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}

export function invoicePaymentQr(
  issuer: BillingSettings | null,
  currency: string,
  grossCents: number,
  reference: string,
): string | null {
  if (!issuer?.iban || currency !== "EUR" || grossCents <= 0) return null;
  const amount = (grossCents / 100).toFixed(2);
  return [
    "BCD",
    "002",
    "1",
    "SCT",
    issuer.bic ?? "",
    issuer.accountHolder.trim() || issuer.legalName.trim(),
    issuer.iban.replace(/\s/g, "").toUpperCase(),
    `EUR${amount}`,
    "",
    "",
    reference.slice(0, 140),
    "",
  ].join("\n");
}
