import type { BillingSettings, SettingsDraft } from "./types";

export type SettingsTextKey =
  | "legalName"
  | "addressLine1"
  | "addressLine2"
  | "postalCode"
  | "city"
  | "country"
  | "registrationNo"
  | "email"
  | "phone"
  | "website"
  | "bankName"
  | "accountHolder"
  | "footerNote"
  | "baseCurrency";

export type SettingsNullableKey = "vatId" | "iban" | "bic";
export type BillingSettingsForm = Record<
  SettingsTextKey | SettingsNullableKey,
  string
>;

const TEXT_KEYS: SettingsTextKey[] = [
  "legalName",
  "addressLine1",
  "addressLine2",
  "postalCode",
  "city",
  "country",
  "registrationNo",
  "email",
  "phone",
  "website",
  "bankName",
  "accountHolder",
  "footerNote",
  "baseCurrency",
];

const NULLABLE_KEYS: SettingsNullableKey[] = ["vatId", "iban", "bic"];

export const BLANK_BILLING_SETTINGS_FORM: BillingSettingsForm = {
  legalName: "",
  addressLine1: "",
  addressLine2: "",
  postalCode: "",
  city: "",
  country: "",
  registrationNo: "",
  email: "",
  phone: "",
  website: "",
  bankName: "",
  accountHolder: "",
  footerNote: "",
  baseCurrency: "",
  vatId: "",
  iban: "",
  bic: "",
};

export function billingSettingsFormOf(
  settings: BillingSettings,
): BillingSettingsForm {
  const form = { ...BLANK_BILLING_SETTINGS_FORM };
  for (const key of TEXT_KEYS) form[key] = settings[key];
  for (const key of NULLABLE_KEYS) form[key] = settings[key] ?? "";
  return form;
}

export function billingSettingsDraftFrom(
  form: BillingSettingsForm,
  stored: BillingSettings,
): SettingsDraft {
  const draft: SettingsDraft = {};
  for (const key of TEXT_KEYS) {
    const value = form[key].trim();
    if (value !== stored[key]) draft[key] = value;
  }
  for (const key of NULLABLE_KEYS) {
    const typed = form[key].trim();
    const value = typed === "" ? null : typed;
    if (value !== stored[key]) draft[key] = value;
  }
  return draft;
}
