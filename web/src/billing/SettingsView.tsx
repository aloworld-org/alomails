// Who the tenant invoices *as* — the issuer side of every printed document
// (alo Billing, ADR 0035, wave B1.16).
//
// A page rather than a dialog, because it is not one record among many: there
// is exactly one of these per tenant, and it is what the top of every invoice,
// credit note and quote is made of.
//
// It keeps the module's three rules. **No validation lives here**: the legal
// name, the country, the VAT id, the IBAN and the BIC are all judged by the
// store (an IBAN by its country's length and its mod-97 check digits), and a
// refusal is shown in the server's own words with everything the user typed
// still on screen. **Only what changed is sent**, so a field nobody touched is
// not written — the surface is last-writer-wins. And **a cleared box sends
// `null`**, which is how a VAT id or a bank account comes off the record.
import { useCallback, useEffect, useState } from "react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { ErrorBanner, Field } from "./parts";
import type { BillingSettings, SettingsDraft } from "./types";
import styles from "./BillingModule.module.css";

/** The form's own state — every field a string, as typed. */
type FormState = Record<TextKey | NullableKey, string>;

/** The fields the server stores as plain text: blank means blank. */
type TextKey =
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
  | "footerNote";

/** The fields the server stores as nullable: blank means `null`. */
type NullableKey = "vatId" | "iban" | "bic";

const TEXT_KEYS: TextKey[] = [
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
];

const NULLABLE_KEYS: NullableKey[] = ["vatId", "iban", "bic"];

const BLANK: FormState = {
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
  vatId: "",
  iban: "",
  bic: "",
};

/** The stored record as the form shows it. A `null` is an empty box — the two
 *  are the same thing to a person, and `draftFrom` turns it back. */
function formOf(settings: BillingSettings): FormState {
  const form = { ...BLANK };
  for (const key of TEXT_KEYS) form[key] = settings[key];
  for (const key of NULLABLE_KEYS) form[key] = settings[key] ?? "";
  return form;
}

/** What to send: only the fields that actually differ from the stored record.
 *  A cleared nullable box becomes `null`, which clears it on the server. */
function draftFrom(form: FormState, stored: BillingSettings): SettingsDraft {
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

export function SettingsView() {
  const api = useBillingApi();
  const [stored, setStored] = useState<BillingSettings | null>(null);
  const [form, setForm] = useState<FormState>(BLANK);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);

  const adopt = useCallback((settings: BillingSettings) => {
    setStored(settings);
    setForm(formOf(settings));
  }, []);

  useEffect(() => {
    let live = true;
    api
      .settings()
      .then((settings) => {
        if (live) adopt(settings);
      })
      .catch((err: unknown) => {
        if (live) setError(billingMessage(err, strings.billingSettingsLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api, adopt]);

  const set = (key: keyof FormState) => (value: string) => {
    setForm((f) => ({ ...f, [key]: value }));
    setSaved(false);
  };

  async function save() {
    if (stored === null) return;
    setBusy(true);
    setError(null);
    try {
      // The whole form is sent when nothing has been stated yet: the first
      // save creates the record, and "nothing changed" is not a save.
      adopt(await api.saveSettings(draftFrom(form, stored)));
      setSaved(true);
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  if (stored === null) {
    return (
      <div className={styles.page}>
        {error !== null ? (
          <ErrorBanner message={error} />
        ) : (
          <div className={styles.loading}>
            <Spinner size={20} />
          </div>
        )}
      </div>
    );
  }

  const text = (key: TextKey | NullableKey) => ({
    className: styles.input,
    value: form[key],
    onChange: (e: { target: { value: string } }) => set(key)(e.target.value),
  });

  return (
    <div className={styles.page}>
      <p className={styles.hint}>
        {stored.stated ? strings.billingSettingsIntro : strings.billingSettingsFirstRun}
      </p>
      {error !== null && <ErrorBanner message={error} />}

      <section className={styles.lines}>
        <h2 className={styles.sectionTitle}>{strings.billingSettingsIdentity}</h2>
        <Field label={strings.billingFieldLegalName} hint={strings.billingLegalNameHint}>
          <input {...text("legalName")} required />
        </Field>
        <Field label={strings.billingFieldAddress}>
          <input {...text("addressLine1")} placeholder={strings.billingAddressPlaceholder} />
        </Field>
        <Field label={strings.billingFieldAddress2}>
          <input {...text("addressLine2")} />
        </Field>
        <div className={styles.row}>
          <Field label={strings.billingFieldPostalCode}>
            <input {...text("postalCode")} />
          </Field>
          <Field label={strings.billingFieldCity}>
            <input {...text("city")} />
          </Field>
          <Field label={strings.billingFieldCountry} hint={strings.billingCountryHint}>
            <input
              {...text("country")}
              placeholder={strings.billingCountryPlaceholder}
              maxLength={2}
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
            />
          </Field>
        </div>
        <div className={styles.row}>
          <Field label={strings.billingFieldVatId} hint={strings.billingIssuerVatIdHint}>
            <input
              {...text("vatId")}
              placeholder={strings.billingVatIdPlaceholder}
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
            />
          </Field>
          <Field label={strings.billingFieldRegistrationNo} hint={strings.billingRegistrationHint}>
            <input {...text("registrationNo")} />
          </Field>
        </div>
      </section>

      <section className={styles.lines}>
        <h2 className={styles.sectionTitle}>{strings.billingSettingsContact}</h2>
        <div className={styles.row}>
          <Field label={strings.billingFieldEmail}>
            <input
              {...text("email")}
              type="email"
              inputMode="email"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
            />
          </Field>
          <Field label={strings.billingFieldPhone}>
            <input {...text("phone")} inputMode="tel" />
          </Field>
          <Field label={strings.billingFieldWebsite}>
            <input {...text("website")} autoCapitalize="none" autoCorrect="off" spellCheck={false} />
          </Field>
        </div>
      </section>

      <section className={styles.lines}>
        <h2 className={styles.sectionTitle}>{strings.billingSettingsBank}</h2>
        <div className={styles.row}>
          <Field label={strings.billingFieldIban} hint={strings.billingIbanHint}>
            <input
              {...text("iban")}
              placeholder={strings.billingIbanPlaceholder}
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
            />
          </Field>
          <Field label={strings.billingFieldBic}>
            <input
              {...text("bic")}
              placeholder={strings.billingBicPlaceholder}
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
            />
          </Field>
        </div>
        <div className={styles.row}>
          <Field label={strings.billingFieldBankName}>
            <input {...text("bankName")} />
          </Field>
          <Field label={strings.billingFieldAccountHolder} hint={strings.billingAccountHolderHint}>
            <input {...text("accountHolder")} />
          </Field>
        </div>
      </section>

      <section className={styles.lines}>
        <h2 className={styles.sectionTitle}>{strings.billingSettingsFooter}</h2>
        <Field label={strings.billingFieldFooterNote} hint={strings.billingFooterNoteHint}>
          <textarea
            className={`${styles.input} ${styles.textarea}`}
            value={form.footerNote}
            rows={2}
            onChange={(e) => set("footerNote")(e.target.value)}
          />
        </Field>
      </section>

      <div className={styles.createBar}>
        <p className={styles.hint} role="status">
          {saved ? strings.billingSettingsSaved : ""}
        </p>
        <Button onClick={() => void save()} disabled={busy || form.legalName.trim() === ""}>
          {strings.billingSave}
        </Button>
      </div>
    </div>
  );
}
