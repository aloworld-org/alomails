// Public surface of the Billing area. The product surface mounts the module;
// nothing outside reaches into the views, the dialogs or the API client.
export { BillingModule } from "./BillingModule";

// Reading and writing the scaled integers the business modules store money in.
// Billing owns these rules — it is where money was first typed and printed —
// and a second module that shows an amount (CRM's deal value, B2.07) reads them
// from here rather than growing a second, slightly different formatter. The
// arithmetic that produces a total still lives on the server, always.
export { formatAmount, hundredthsToInput, parseHundredths } from "./money";
