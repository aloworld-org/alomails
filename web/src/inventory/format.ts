// Reading what the warehouse stored: a quantity, a reference value, an instant,
// and the words for why something moved.
//
// Every function here **formats one stored value**. Nothing is summed, nothing
// is converted, nothing is derived from two fields at once — on-hand and its
// reference value both arrive computed (`docs/design/inventory.md`), and a
// browser that re-derived one would be the second opinion a stocktake argues
// with.
//
// The amount formatter and the quantity formatter are Billing's. Money and
// scaled quantities were first typed and printed there, and a second module
// that shows either reads them from that module rather than growing a second,
// slightly different rule about where the decimal point goes.
import { formatAmount, formatQty } from "../billing";
import { getLocale, strings } from "../i18n";
import type { AdjustReasonCode, LocationKind, MoveReason } from "./types";

/** A quantity the server stored, in milli-units ("1500" → "1.5"). */
export function qtyLabel(qtyMilli: number): string {
  return formatQty(qtyMilli, getLocale());
}

/** A reference value the server computed, in the tenant's own currency. No
 *  currency symbol: a purchase price is quoted in the tenant's currency, the
 *  same convention the price list uses. */
export function valueLabel(cents: number): string {
  return formatAmount(cents, getLocale());
}

/** An instant the server wrote (RFC 3339), read in the interface language. */
export function momentLabel(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return at.toLocaleString(getLocale(), { dateStyle: "medium", timeStyle: "short" });
}

/** What to call a place's kind. An unknown one — a kind a newer server learned
 *  first — is shown verbatim rather than blanked. */
export function locationKindLabel(kind: LocationKind): string {
  switch (kind) {
    case "stock":
      return strings.inventoryKindStock;
    case "transit":
      return strings.inventoryKindTransit;
    case "supplier":
      return strings.inventoryKindSupplier;
    case "customer":
      return strings.inventoryKindCustomer;
    case "adjust":
      return strings.inventoryKindAdjust;
    case "production":
      return strings.inventoryKindProduction;
    default:
      return kind;
  }
}

/** Why something moved, in words. Same rule for an unknown reason. */
export function moveReasonLabel(reason: MoveReason): string {
  switch (reason) {
    case "receipt":
      return strings.inventoryReasonReceipt;
    case "delivery":
      return strings.inventoryReasonDelivery;
    case "transfer":
      return strings.inventoryReasonTransfer;
    case "adjustment":
      return strings.inventoryReasonAdjustment;
    case "return":
      return strings.inventoryReasonReturn;
    case "shrinkage":
      return strings.inventoryReasonShrinkage;
    case "count":
      return strings.inventoryReasonCount;
    default:
      return reason;
  }
}

/** The reason code a person picked when they adjusted stock by hand. */
export function adjustReasonLabel(code: AdjustReasonCode): string {
  switch (code) {
    case "damaged":
      return strings.inventoryAdjustDamaged;
    case "lost":
      return strings.inventoryAdjustLost;
    case "found":
      return strings.inventoryAdjustFound;
    case "expired":
      return strings.inventoryAdjustExpired;
    case "theft":
      return strings.inventoryAdjustTheft;
    case "sample":
      return strings.inventoryAdjustSample;
    case "correction":
      return strings.inventoryAdjustCorrection;
    default:
      return code;
  }
}
