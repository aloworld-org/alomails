import { strings } from "../i18n";
import type { BadgeProps } from "../ds";
import type { InvoiceStatus, QuoteStatus } from "./types";

export type ChipTone = "neutral" | "info" | "good" | "warn" | "muted";

export const BADGE_TONE = {
  neutral: "neutral",
  info: "accent",
  good: "success",
  warn: "warning",
  muted: "neutral",
} as const satisfies Record<ChipTone, NonNullable<BadgeProps["tone"]>>;

export function statusLabel(status: InvoiceStatus): string {
  switch (status) {
    case "draft": return strings.billingStatusDraft;
    case "issued": return strings.billingStatusIssued;
    case "paid": return strings.billingStatusPaid;
    case "void": return strings.billingStatusVoid;
    default: return status;
  }
}

export function statusTone(status: InvoiceStatus): ChipTone {
  switch (status) {
    case "issued": return "info";
    case "paid": return "good";
    case "void": return "muted";
    default: return "neutral";
  }
}

export function quoteStatusLabel(status: QuoteStatus): string {
  switch (status) {
    case "draft": return strings.billingStatusDraft;
    case "sent": return strings.billingQuoteStatusSent;
    case "accepted": return strings.billingQuoteStatusAccepted;
    case "declined": return strings.billingQuoteStatusDeclined;
    case "expired": return strings.billingQuoteStatusExpired;
    default: return status;
  }
}

export function quoteStatusTone(status: QuoteStatus): ChipTone {
  switch (status) {
    case "sent": return "info";
    case "accepted": return "good";
    case "declined":
    case "expired": return "muted";
    default: return "neutral";
  }
}
