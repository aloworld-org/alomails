// Public surface of the Billing area. The product surface mounts the module;
// nothing outside reaches into the views, the dialogs or the API client.
export { BillingModule } from "./BillingModule";

// Reading and writing the scaled integers the business modules store money in.
// Billing owns these rules — it is where money was first typed and printed —
// and a second module that shows an amount (CRM's deal value, B2.07) reads them
// from here rather than growing a second, slightly different formatter. The
// arithmetic that produces a total still lives on the server, always.
export { formatAmount, formatRate, hundredthsToInput, parseHundredths } from "./money";

// The same argument for a calendar day: a `YYYY-MM-DD` the server sent means
// that day in every zone, and the one rule that keeps it from sliding a day
// backwards for a reader west of Greenwich lives here. Finance's claims (B4.13)
// read it rather than parsing a day as an instant of their own.
export { formatDocumentDate } from "./dates";

// The calendar periods a report is asked for. A quarter is a calendar fact, not
// a billing one, and the pipeline report (B2.08) asks for exactly the same two
// days as the VAT summary — so the second caller reads these rather than
// growing a second definition of "last quarter" that disagrees at a boundary.
export { previousQuarterOf, quarterOf, type Period } from "./period";

// Who a tenant bills. A module outside Billing that has to name a customer —
// the engagement form in Projects (B3.07) — reads the list from here rather
// than growing a second client for `/billing/customers` with its own idea of
// whether archived ones are on offer.
export { useCustomers } from "./pickers";
export type { BillingCustomer } from "./types";

// The documents that can still take money. Finance's reconciliation screen
// (B4.13b) has to let a bookkeeper say by hand which invoice a bank line
// settled, and the list of candidates is Billing's — the same argument as
// `useCustomers`, one door up.
export { useOpenInvoices } from "./pickers";
export type { BillingInvoiceSummary } from "./types";
