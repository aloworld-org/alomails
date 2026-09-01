//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod accountant_role_http;
mod billing_bills_http;
mod billing_cii_golden;
mod billing_facturx_http;
mod billing_fx_http;
mod billing_http;
mod billing_invoice_http;
mod billing_pdf_http;
mod billing_print_http;
mod billing_quote_design_http;
mod billing_quote_http;
mod billing_report_http;
mod billing_schedules_http;
mod billing_send_http;
mod billing_sepa_golden;
mod billing_sepa_http;
mod billing_ubl_golden;
mod billing_xrechnung_http;
mod fin_aged_http;
mod fin_balance_http;
mod fin_chart_http;
mod fin_report_http;
mod fin_report_schedules_http;
mod fin_vat_http;
