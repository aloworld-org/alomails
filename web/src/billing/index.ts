// Public surface of the Billing area. The product surface mounts the module;
// nothing outside reaches into the views, the dialogs or the API client.
export { BillingModule } from "./BillingModule";

// Reading and writing the scaled integers the business modules store money in.
// Billing owns these rules — it is where money was first typed and printed —
// and a second module that shows an amount (CRM's deal value, B2.07) reads them
// from here rather than growing a second, slightly different formatter. The
// arithmetic that produces a total still lives on the server, always.
// `formatQty` joined them for wave B5: a quantity is the same kind of scaled
// integer (milli-units), and a stock screen that wrote its own divide-by-1000
// would round a third of a kilo differently from an invoice line.
export {
  formatAmount,
  formatQty,
  formatRate,
  hundredthsToInput,
  parseHundredths,
} from "./money";

// The same argument for a calendar day: a `YYYY-MM-DD` the server sent means
// that day in every zone, and the one rule that keeps it from sliding a day
// backwards for a reader west of Greenwich lives here. Finance's claims (B4.13)
// read it rather than parsing a day as an instant of their own.
export { formatDocumentDate } from "./dates";

// The calendar periods a report is asked for. A quarter is a calendar fact, not
// a billing one, and the pipeline report (B2.08) asks for exactly the same two
// days as the VAT summary — so the second caller reads these rather than
// growing a second definition of "last quarter" that disagrees at a boundary.
// `yearOf`/`previousYearOf` joined at B4.13c for the same reason: a profit and
// loss and a chart of accounts are read for a year, and a second definition of
// "this year" would disagree with this one at a boundary for a reader west of
// Greenwich.
export { previousQuarterOf, previousYearOf, quarterOf, yearOf, type Period } from "./period";

// Who a tenant bills. A module outside Billing that has to name a customer —
// the engagement form in Projects (B3.07) — reads the list from here rather
// than growing a second client for `/billing/customers` with its own idea of
// whether archived ones are on offer.
export { useCustomers } from "./pickers";
export type { BillingCustomer } from "./types";

// The catalog, for the module that reads the same rows as things rather than
// as prices: Inventory's catalog screen (B5.09a).
//
// **It reuses this module's client and this module's product dialog** rather
// than growing a second one for `/billing/products`. A product is one row
// (`docs/design/inventory.md` § The catalog) — SKU, barcode, stocked-or-not
// and purchase price sit beside the sale price — and two forms over one row
// would eventually disagree about what a product is. The dialog takes the
// supplier choices Inventory has loaded and Billing has not, which is the only
// difference between the two screens' editors.
export { ProductDialog } from "./ProductDialog";
export type { SupplierChoice } from "./ProductDialog";
export type { BillingProduct, ProductDraft } from "./types";
export { billingMessage, useBillingApi } from "./api";

// The documents that can still take money. Finance's reconciliation screen
// (B4.13b) has to let a bookkeeper say by hand which invoice a bank line
// settled, and the list of candidates is Billing's — the same argument as
// `useCustomers`, one door up.
export { useOpenInvoices } from "./pickers";
export type { BillingInvoiceSummary } from "./types";
