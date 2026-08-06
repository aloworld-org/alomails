// Create or edit one price-list item.
//
// The money edge lives here: the user types a price and a VAT rate, and the
// API takes integer cents and basis points. `parseHundredths` does that one
// conversion (and refuses text it cannot turn into a number) — everything a
// document is actually worth is computed by the server, never here.
//
// Editing a price never rewrites a document already raised: a line copies
// name, unit, price and rate at the moment the item is picked.
import { useState } from "react";
import { Package } from "lucide-react";

import { strings } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { hundredthsToInput, parseHundredths } from "./money";
import { DialogFrame, Field } from "./parts";
import type { BillingProduct, ProductDraft } from "./types";
import styles from "./BillingModule.module.css";

interface Props {
  /** The record being edited, or `null` to create one. */
  product: BillingProduct | null;
  onClose: () => void;
  /** The list reloads from the server, so the saved record is not passed on. */
  onSaved: () => void;
}

export function ProductDialog({ product, onClose, onSaved }: Props) {
  const api = useBillingApi();
  const [name, setName] = useState(product?.name ?? "");
  const [unit, setUnit] = useState(product?.unit ?? "");
  const [price, setPrice] = useState(
    product === null ? "" : hundredthsToInput(product.unitPriceCents),
  );
  const [rate, setRate] = useState(product === null ? "" : hundredthsToInput(product.vatRateBp));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Blank means "not stated": on a create the server's default (free,
  // zero-rated) applies, on an edit the stored value stays.
  const priceCents = price.trim() === "" ? null : parseHundredths(price);
  const rateBp = rate.trim() === "" ? null : parseHundredths(rate);
  const priceError = price.trim() !== "" && priceCents === null;
  const rateError = rate.trim() !== "" && rateBp === null;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const draft: ProductDraft = {};
      const trimmedName = name.trim();
      const trimmedUnit = unit.trim();
      if (product === null ? trimmedName !== "" : trimmedName !== product.name) {
        draft.name = trimmedName;
      }
      if (product === null ? trimmedUnit !== "" : trimmedUnit !== product.unit) {
        draft.unit = trimmedUnit;
      }
      if (priceCents !== null && (product === null || priceCents !== product.unitPriceCents)) {
        draft.unitPriceCents = priceCents;
      }
      if (rateBp !== null && (product === null || rateBp !== product.vatRateBp)) {
        draft.vatRateBp = rateBp;
      }
      if (product === null) await api.createProduct(draft);
      else await api.updateProduct(product.id, draft);
      onSaved();
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Package}
      title={product === null ? strings.billingNewProduct : strings.billingEditProduct}
      subtitle={strings.billingProductSubtitle}
      error={error}
      busy={busy}
      canSubmit={name.trim() !== "" && !priceError && !rateError}
      submitLabel={product === null ? strings.billingCreate : strings.billingSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.billingFieldName}>
        <input
          className={styles.input}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
          required
        />
      </Field>

      <Field label={strings.billingFieldUnit} hint={strings.billingUnitHint}>
        <input
          className={styles.input}
          value={unit}
          onChange={(e) => setUnit(e.target.value)}
          placeholder={strings.billingUnitPlaceholder}
        />
      </Field>

      <div className={styles.row}>
        <Field
          label={strings.billingFieldUnitPrice}
          hint={strings.billingPriceHint}
          error={priceError ? strings.billingNotAnAmount : undefined}
        >
          <input
            className={styles.input}
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            placeholder={strings.billingAmountPlaceholder}
            inputMode="decimal"
            aria-invalid={priceError}
          />
        </Field>
        <Field
          label={strings.billingFieldVatRate}
          hint={strings.billingRateHint}
          error={rateError ? strings.billingNotARate : undefined}
        >
          <input
            className={styles.input}
            value={rate}
            onChange={(e) => setRate(e.target.value)}
            placeholder={strings.billingRatePlaceholder}
            inputMode="decimal"
            aria-invalid={rateError}
          />
        </Field>
      </div>
    </DialogFrame>
  );
}
