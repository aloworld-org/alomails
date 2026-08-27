import { describe, expect, it } from "vitest";
import {
  billingSettingsDraftFrom,
  billingSettingsFormOf,
} from "./billingSettingsForm";
import type { BillingSettings } from "./types";

const settings: BillingSettings = {
  stated: true,
  updatedBy: "user-1",
  updatedAt: "2026-08-27T12:00:00Z",
  legalName: "alo Studio GmbH",
  addressLine1: "Friedrichstrasse 88",
  addressLine2: "",
  postalCode: "10117",
  city: "Berlin",
  country: "DE",
  vatId: null,
  registrationNo: "HRB 248610 B",
  email: "billing@alo.example",
  phone: "+49 30 555 0182",
  website: "https://alo.example",
  iban: null,
  bic: null,
  bankName: "Demo Business Bank",
  accountHolder: "alo Studio GmbH",
  footerNote: "Thank you for your business.",
  baseCurrency: "EUR",
};

describe("billing settings form", () => {
  it("shows nullable values as empty form fields", () => {
    expect(billingSettingsFormOf(settings).vatId).toBe("");
  });

  it("sends only changes and clears nullable fields with null", () => {
    const stored = { ...settings, vatId: "DE123" };
    const form = billingSettingsFormOf(stored);
    form.legalName = "  alo Europe GmbH  ";
    form.vatId = "";
    expect(billingSettingsDraftFrom(form, stored)).toEqual({
      legalName: "alo Europe GmbH",
      vatId: null,
    });
  });
});
