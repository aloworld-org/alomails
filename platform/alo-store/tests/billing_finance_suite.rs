//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod bank_camt_fixtures;
mod bank_csv_fixtures;
mod bank_import_tenancy;
mod bank_manual;
mod bank_mt940_fixtures;
mod bank_reconcile;
mod bank_suggest;
mod billing_bills;
mod billing_by_number;
mod billing_catalog_seam;
mod billing_credit_notes;
mod billing_customers_tenancy;
mod billing_demo;
mod billing_fx;
mod billing_invoice_designs_tenancy;
mod billing_invoice_issue;
mod billing_invoice_lifecycle;
mod billing_invoices_tenancy;
mod billing_payments;
mod billing_products_tenancy;
mod billing_quote_designs_tenancy;
mod billing_quote_lifecycle;
mod billing_quote_routing;
mod billing_quote_to_invoice;
mod billing_quotes_tenancy;
mod billing_schedules;
mod billing_sepa;
mod billing_settings_tenancy;
mod billing_vat_report;
mod fin_accounts_tenancy;
mod fin_aged;
mod fin_anomalies;
mod fin_balance_sheet;
mod fin_categorise;
mod fin_credit_note_posting;
mod fin_expense_flow;
mod fin_expenses_tenancy;
mod fin_invoice_posting;
mod fin_journal_properties;
mod fin_journal_tenancy;
mod fin_mileage_tenancy;
mod fin_payment_posting;
mod fin_periods;
mod fin_pl_report;
mod fin_receipt_fixtures;
mod fin_receipt_read;
mod fin_vat_return;
