// Create or edit one catalog item — the price a document charges for it, and
// the facts a warehouse knows it by.
//
// **One form for both halves, and that is the decision.** A product is one row
// (`docs/design/inventory.md` § The catalog): its SKU, barcode, purchase price
// and whether it is stocked at all live on the same record as its sale price,
// so Inventory's catalog screen opens THIS dialog rather than growing a second
// product form that would eventually disagree with this one about what a
// product is.
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

/** A supplier as this form needs one: a name to show and an id to send.
 *  Deliberately not the supplier record — the price list has no business
 *  knowing a supplier's IBAN. */
export interface SupplierChoice {
  id: string;
  name: string;
}

interface Props {
  /** The record being edited, or `null` to create one. */
  product: BillingProduct | null;
  /**
   * Who this tenant buys from, for the default-supplier picker.
   *
   * Absent on Billing's own price list — that screen is about what we charge,
   * and it has no supplier list loaded. Inventory's catalog passes them
   * (`docs/design/inventory.md` § As built B5.03: "the picker is still
   * B5.09a's"), and the field appears only where there is something to pick.
   */
  suppliers?: SupplierChoice[];
  onClose: () => void;
  /** The list reloads from the server, so the saved record is not passed on. */
  onSaved: () => void;
}

export function ProductDialog({ product, suppliers, onClose, onSaved }: Props) {
  const api = useBillingApi();
  const [name, setName] = useState(product?.name ?? "");
  const [unit, setUnit] = useState(product?.unit ?? "");
  const [price, setPrice] = useState(
    product === null ? "" : hundredthsToInput(product.unitPriceCents),
  );
  const [rate, setRate] = useState(product === null ? "" : hundredthsToInput(product.vatRateBp));
  const [sku, setSku] = useState(product?.sku ?? "");
  const [barcode, setBarcode] = useState(product?.barcode ?? "");
  const [stocked, setStocked] = useState(product?.stocked ?? false);
  const [purchase, setPurchase] = useState(
    product === null ? "" : hundredthsToInput(product.purchasePriceCents),
  );
  const [supplier, setSupplier] = useState(product?.defaultSupplierId ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Blank means "not stated": on a create the server's default (free,
  // zero-rated) applies, on an edit the stored value stays.
  const priceCents = price.trim() === "" ? null : parseHundredths(price);
  const rateBp = rate.trim() === "" ? null : parseHundredths(rate);
  const purchaseCents = purchase.trim() === "" ? null : parseHundredths(purchase);
  const priceError = price.trim() !== "" && priceCents === null;
  const rateError = rate.trim() !== "" && rateBp === null;
  const purchaseError = purchase.trim() !== "" && purchaseCents === null;

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
      // The catalog half. A code cleared on an edit IS a change and is sent as
      // the empty string the server reads as "no code"; on a create a blank one
      // is simply not stated.
      const trimmedSku = sku.trim();
      const trimmedBarcode = barcode.trim();
      if (product === null ? trimmedSku !== "" : trimmedSku !== product.sku) {
        draft.sku = trimmedSku;
      }
      if (product === null ? trimmedBarcode !== "" : trimmedBarcode !== product.barcode) {
        draft.barcode = trimmedBarcode;
      }
      if (product === null ? stocked : stocked !== product.stocked) {
        draft.stocked = stocked;
      }
      if (
        purchaseCents !== null &&
        (product === null || purchaseCents !== product.purchasePriceCents)
      ) {
        draft.purchasePriceCents = purchaseCents;
      }
      // Only where there was a picker to answer with: a screen that does not
      // offer suppliers must not send a field that clears one.
      if (suppliers !== undefined && supplier !== (product?.defaultSupplierId ?? "")) {
        draft.defaultSupplierId = supplier === "" ? null : supplier;
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
      canSubmit={name.trim() !== "" && !priceError && !rateError && !purchaseError}
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

      {/* The warehouse's half of the same record (B5.02/B5.03). It is shown on
          both screens rather than behind a toggle: an SKU is as ordinary a fact
          about a product as its price, and a form that hides half of itself is
          a form people fill in twice. */}
      <div className={styles.row}>
        <Field label={strings.inventoryFieldSku} hint={strings.inventorySkuHint}>
          <input
            className={styles.input}
            value={sku}
            onChange={(e) => setSku(e.target.value)}
          />
        </Field>
        <Field label={strings.inventoryFieldBarcode} hint={strings.inventoryBarcodeHint}>
          <input
            className={styles.input}
            value={barcode}
            onChange={(e) => setBarcode(e.target.value)}
            inputMode="numeric"
          />
        </Field>
      </div>

      <div className={styles.row}>
        <Field
          label={strings.inventoryFieldPurchasePrice}
          hint={strings.inventoryPurchasePriceHint}
          error={purchaseError ? strings.billingNotAnAmount : undefined}
        >
          <input
            className={styles.input}
            value={purchase}
            onChange={(e) => setPurchase(e.target.value)}
            placeholder={strings.billingAmountPlaceholder}
            inputMode="decimal"
            aria-invalid={purchaseError}
          />
        </Field>
        {/* Only where there is something to pick — Billing's price list loads no
            suppliers, and an empty picker is a promise of a list that is not
            there. */}
        {suppliers !== undefined && (
          <Field
            label={strings.inventoryFieldDefaultSupplier}
            hint={strings.inventoryDefaultSupplierHint}
          >
            <select
              className={styles.input}
              value={supplier}
              onChange={(e) => setSupplier(e.target.value)}
            >
              <option value="">{strings.inventoryNoSupplier}</option>
              {suppliers.map((choice) => (
                <option key={choice.id} value={choice.id}>
                  {choice.name}
                </option>
              ))}
            </select>
          </Field>
        )}
      </div>

      {/* The one field that decides whether this product can move at all: the
          ledger refuses a movement of a service, and turning it off again on a
          product that already moved is the server's `409`. Said here, where the
          box is, rather than discovered on a receipt. */}
      <Field label={strings.inventoryFieldStocked} hint={strings.inventoryStockedHint}>
        <span className={styles.toggle}>
          <input
            type="checkbox"
            checked={stocked}
            onChange={(e) => setStocked(e.target.checked)}
          />
          {strings.inventoryStockedLabel}
        </span>
      </Field>
    </DialogFrame>
  );
}
