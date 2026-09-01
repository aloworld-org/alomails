//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod audit_trail_tenancy;
mod campaign_audience_tenancy;
mod campaign_consent_tenancy;
mod campaign_dispatch_tenancy;
mod campaign_html_golden;
mod campaign_preview_tenancy;
mod campaign_record_tenancy;
mod campaign_segments_tenancy;
mod campaign_send_tenancy;
mod campaign_suppression_tenancy;
mod campaign_text_golden;
mod campaign_topic_optout_tenancy;
mod campaign_unsubscribe_tenancy;
mod crm_activities_tenancy;
mod crm_deal_projects_tenancy;
mod crm_deal_threads_tenancy;
mod crm_deals_tenancy;
mod crm_handoff_tenancy;
mod crm_lead_import_tenancy;
mod crm_lead_seam;
mod crm_pipelines_tenancy;
mod crm_report_tenancy;
mod hr_checklists_tenancy;
mod hr_documents_tenancy;
mod hr_employees_tenancy;
mod hr_holidays_tenancy;
mod hr_leave_policies_tenancy;
mod hr_leave_requests_tenancy;
mod hr_letters_tenancy;
mod hr_payroll_tenancy;
mod hr_recruitment_tenancy;
mod insight_dashboards_tenancy;
mod insight_overview_seed;
mod insight_query_tenancy;
mod inv_adjust;
mod inv_count;
mod inv_count_apply;
mod inv_locations_tenancy;
mod inv_po_lifecycle;
mod inv_po_receive;
mod inv_po_send;
mod inv_reorder;
mod inv_so_commit;
mod inv_so_deliver;
mod inv_so_invoice;
mod inv_so_quote_link;
mod inv_stock_ledger;
mod inv_stock_sale;
mod inv_suppliers_tenancy;
mod project_clients_tenancy;
mod project_hours_tenancy;
mod project_milestones_tenancy;
mod project_setup_tenancy;
mod project_templates_tenancy;
mod time_entries_tenancy;
mod time_invoice_tenancy;
mod time_report_tenancy;
mod time_timer_tenancy;
mod time_weeks_tenancy;
