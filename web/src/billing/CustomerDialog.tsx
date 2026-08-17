// Create or edit one billing customer.
//
// It holds no rules of its own: name, country, currency, VAT id, email and
// payment terms are all judged by the store, and what comes back on a refusal
// is shown as-is (those messages name the rule they broke and never echo a
// stored value). The form's only client-side job is turning blank fields into
// the `null` that clears a nullable one, and sending an edit as the fields
// that actually changed rather than a full overwrite — the surface is
// last-writer-wins, so a field nobody touched should not be written.
import { useState } from "react";
import { Building2 } from "lucide-react";

import { strings } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { BillingCustomer, CustomerDraft } from "./types";
import styles from "./billingStyles";

interface Props {
  /** The record being edited, or `null` to create one. */
  customer: BillingCustomer | null;
  onClose: () => void;
  /** The list reloads from the server, so the saved record is not passed on. */
  onSaved: () => void;
}

/** The form's own state — every field a string, as typed. */
interface FormState {
  name: string;
  addressLine1: string;
  addressLine2: string;
  postalCode: string;
  city: string;
  country: string;
  vatId: string;
  email: string;
  paymentTermsDays: string;
  currency: string;
}

function initial(c: BillingCustomer | null): FormState {
  return {
    name: c?.name ?? "",
    addressLine1: c?.addressLine1 ?? "",
    addressLine2: c?.addressLine2 ?? "",
    postalCode: c?.postalCode ?? "",
    city: c?.city ?? "",
    country: c?.country ?? "",
    vatId: c?.vatId ?? "",
    email: c?.email ?? "",
    paymentTermsDays: c === null ? "" : String(c.paymentTermsDays),
    currency: c?.currency ?? "",
  };
}

/** The draft to send: on a create everything stated, on an edit only what the
 *  user actually changed. A field left blank on a create is simply absent, so
 *  the server's own default (EUR, 30-day terms) applies. */
function draftFrom(form: FormState, stored: BillingCustomer | null): CustomerDraft {
  const draft: CustomerDraft = {};
  const text = (
    key: "name" | "addressLine1" | "addressLine2" | "postalCode" | "city" | "country" | "currency",
  ) => {
    const value = form[key].trim();
    if (stored === null ? value !== "" : value !== stored[key]) draft[key] = value;
  };
  text("name");
  text("addressLine1");
  text("addressLine2");
  text("postalCode");
  text("city");
  text("country");
  text("currency");

  // A cleared box means `null`, which is how a VAT id or an invoice address is
  // taken off a customer again.
  const nullable = (key: "vatId" | "email") => {
    const typed = form[key].trim();
    const value = typed === "" ? null : typed;
    if (stored === null ? value !== null : value !== stored[key]) draft[key] = value;
  };
  nullable("vatId");
  nullable("email");

  const days = form.paymentTermsDays.trim();
  if (/^-?[0-9]+$/.test(days)) {
    const parsed = Number(days);
    if (stored === null || parsed !== stored.paymentTermsDays) draft.paymentTermsDays = parsed;
  }
  return draft;
}

export function CustomerDialog({ customer, onClose, onSaved }: Props) {
  const api = useBillingApi();
  const [form, setForm] = useState<FormState>(() => initial(customer));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const set = (key: keyof FormState) => (value: string) =>
    setForm((f) => ({ ...f, [key]: value }));

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const draft = draftFrom(form, customer);
      if (customer === null) await api.createCustomer(draft);
      else await api.updateCustomer(customer.id, draft);
      onSaved();
    } catch (err) {
      // A refusal keeps the form up with everything the user typed: the point
      // of showing the reason is that they can fix it in place.
      setError(billingMessage(err, strings.billingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Building2}
      title={customer === null ? strings.billingNewCustomer : strings.billingEditCustomer}
      subtitle={strings.billingCustomerSubtitle}
      error={error}
      busy={busy}
      canSubmit={form.name.trim() !== ""}
      submitLabel={customer === null ? strings.billingCreate : strings.billingSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.billingFieldName}>
        <input
          className={styles.input}
          value={form.name}
          onChange={(e) => set("name")(e.target.value)}
          autoFocus
          required
        />
      </Field>

      <Field label={strings.billingFieldEmail}>
        <input
          className={styles.input}
          type="email"
          value={form.email}
          onChange={(e) => set("email")(e.target.value)}
          placeholder={strings.billingEmailPlaceholder}
          inputMode="email"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
        />
      </Field>

      <Field label={strings.billingFieldAddress}>
        <input
          className={styles.input}
          value={form.addressLine1}
          onChange={(e) => set("addressLine1")(e.target.value)}
          placeholder={strings.billingAddressPlaceholder}
        />
      </Field>
      <Field label={strings.billingFieldAddress2}>
        <input
          className={styles.input}
          value={form.addressLine2}
          onChange={(e) => set("addressLine2")(e.target.value)}
        />
      </Field>

      <div className={styles.row}>
        <Field label={strings.billingFieldPostalCode}>
          <input
            className={styles.input}
            value={form.postalCode}
            onChange={(e) => set("postalCode")(e.target.value)}
          />
        </Field>
        <Field label={strings.billingFieldCity}>
          <input
            className={styles.input}
            value={form.city}
            onChange={(e) => set("city")(e.target.value)}
          />
        </Field>
        <Field label={strings.billingFieldCountry} hint={strings.billingCountryHint}>
          <input
            className={styles.input}
            value={form.country}
            onChange={(e) => set("country")(e.target.value)}
            placeholder={strings.billingCountryPlaceholder}
            maxLength={2}
            autoCapitalize="characters"
            autoCorrect="off"
            spellCheck={false}
          />
        </Field>
      </div>

      <Field label={strings.billingFieldVatId} hint={strings.billingVatIdHint}>
        <input
          className={styles.input}
          value={form.vatId}
          onChange={(e) => set("vatId")(e.target.value)}
          placeholder={strings.billingVatIdPlaceholder}
          autoCapitalize="characters"
          autoCorrect="off"
          spellCheck={false}
        />
      </Field>

      <div className={styles.row}>
        <Field label={strings.billingFieldTerms} hint={strings.billingTermsHint}>
          <input
            className={styles.input}
            value={form.paymentTermsDays}
            onChange={(e) => set("paymentTermsDays")(e.target.value)}
            placeholder={strings.billingTermsPlaceholder}
            inputMode="numeric"
          />
        </Field>
        <Field label={strings.billingFieldCurrency}>
          <input
            className={styles.input}
            value={form.currency}
            onChange={(e) => set("currency")(e.target.value)}
            placeholder={strings.billingCurrencyPlaceholder}
            maxLength={3}
            autoCapitalize="characters"
            autoCorrect="off"
            spellCheck={false}
          />
        </Field>
      </div>
    </DialogFrame>
  );
}
