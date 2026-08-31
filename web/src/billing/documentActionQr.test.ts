import { describe, expect, test } from "vitest";

import type { BillingSettings } from "./types";
import { invoicePaymentQr, quotationAcceptanceQr } from "./documentActionQr";

const issuer = {
  email: "billing@example.com",
  iban: "BE68 5390 0754 7034",
  bic: "KREDBEBB",
  accountHolder: "Example BV",
  legalName: "Example BV",
} as BillingSettings;

describe("customer document QR payloads", () => {
  test("opens a prepared quotation acceptance message", () => {
    expect(quotationAcceptanceQr(issuer, "Q-42", "Accept Q-42", "I accept Q-42"))
      .toBe("mailto:billing%40example.com?subject=Accept%20Q-42&body=I%20accept%20Q-42");
  });

  test("builds a standard EPC payment payload for a euro invoice", () => {
    const payload = invoicePaymentQr(issuer, "EUR", 12345, "INV-42");
    expect(payload).toContain("\nSCT\nKREDBEBB\nExample BV\nBE68539007547034\nEUR123.45");
    expect(payload).toContain("\nINV-42\n");
  });

  test("does not offer a payment code without supported bank details", () => {
    expect(invoicePaymentQr(issuer, "USD", 12345, "INV-42")).toBeNull();
    expect(quotationAcceptanceQr(issuer, null, "", "")).toBeNull();
  });
});
