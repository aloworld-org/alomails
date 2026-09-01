//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod audit_http;
mod audit_routes;
mod campaign_preview_http;
mod campaign_record_http;
mod campaign_unsubscribe_http;
mod campaign_unsubscribe_segment_http;
mod campaigns_http;
mod crm_activities_http;
mod crm_closing_http;
mod crm_http;
mod crm_import_http;
mod crm_threads_http;
mod insights_ask_http;
mod insights_http;
mod inventory_order_book_http;
mod inventory_suppliers_http;
mod orders_walkthrough_http;
mod projects_setup_http;
