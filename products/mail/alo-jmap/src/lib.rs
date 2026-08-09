//! # alo-jmap — the JMAP API (RFC 8620 core, RFC 8621 mail)
//!
//! An HTTP service over [`alo_store`]. **A public contract from
//! merge:** the web app, desktop cache, and compat adapters speak it, so
//! every surface changes additively forever (see
//! `docs/design/jmap-api.md`).
//!
//! Every request reaches data only through the store's `for_account`
//! door: the bearer token resolves to `(tenant, account)` via
//! [`alo_identity`] and the tenant claim is never read from a request
//! body. The OpenID Connect / OAuth 2.0 provider is mounted alongside
//! (see [`server::app`]), so one HTTP service serves both JMAP and the
//! IdP.

pub mod admin;
pub mod agent;
pub mod agent_args;
pub mod agent_billing;
pub mod agent_crm;
pub mod agent_projects;
pub mod agent_timesheet;
pub mod ai;
pub mod api;
pub mod audit;
pub mod audit_action;
pub mod audit_record;
pub mod autoconfig;
pub mod base;
pub mod billing;
pub mod billing_bills;
pub mod billing_cii;
pub mod billing_customers;
pub mod billing_document;
pub mod billing_einvoice;
pub mod billing_einvoice_rules;
pub mod billing_fx;
pub mod billing_invoices;
pub mod billing_pain001;
pub mod billing_pain001_rules;
pub mod billing_payments;
pub mod billing_pdf;
pub mod billing_print;
pub mod billing_products;
pub mod billing_quotes;
pub mod billing_reminder;
pub mod billing_reports;
pub mod billing_schedules;
pub mod billing_send;
pub mod billing_sepa;
pub mod billing_settings;
pub mod billing_ubl;
pub mod billing_xml;
pub mod billing_xrechnung_rules;
pub mod blob;
pub mod calendar;
pub mod carddav;
pub mod chat;
pub mod chat_agent;
pub mod chat_agent_routes;
pub mod chat_turns;
pub mod contacts;
pub mod crm;
pub mod crm_activities;
pub mod crm_deals;
pub mod crm_handoff;
pub mod crm_imports;
pub mod crm_next_steps;
pub mod crm_pipelines;
pub mod crm_reports;
pub mod crm_stages;
pub mod crm_threads;
pub mod csv;
pub mod delegates;
pub mod docs;
pub mod drafts;
pub mod drive;
pub mod error;
pub mod filters;
pub mod finance_approvals;
pub mod finance_bank;
pub mod finance_bank_match;
pub mod finance_expenses;
pub mod finance_mileage;
pub mod finance_periods;
pub mod finance_receipts;
pub mod finance_report_balance;
pub mod finance_report_pl;
pub mod finance_reports;
pub mod flagdue;
pub mod imap_import;
pub mod imap_import_route;
pub mod insights;
pub mod insights_ask;
pub mod insights_eval;
pub mod insights_gallery;
pub mod jtypes;
pub mod junk_learn;
pub mod mime;
pub mod mime_read;
pub mod projects;
pub mod projects_clients;
pub mod projects_invoices;
pub mod projects_plan;
pub mod projects_reports;
pub mod projects_templates;
pub mod projects_time;
pub mod projects_weeks;
pub mod push;
pub mod reset_route;
pub mod schedule;
pub mod security;
pub mod server;
pub mod session;
pub mod settings;
pub mod share;
pub mod sieve;
pub mod signup_route;
pub mod site_notify;
pub mod sites;
pub mod snooze;
pub mod spaces;
pub mod state;
pub mod submission;
pub mod tasks;
pub mod unsubscribe;
pub mod wopi;
pub mod workspace_search;

pub use push::PushHub;
pub use server::{app, app_state, app_with_site_domain_dns, serve};
pub use state::{AppState, Limits};
