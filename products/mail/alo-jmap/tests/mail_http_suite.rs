//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod app_passwords_http;
mod autoconfig;
mod bcc_wire;
mod caldav;
mod calendar_http;
mod carddav;
mod categories;
mod conformance;
mod contacts;
mod contacts_import;
mod delegated_send;
mod delegation;
mod flag_due;
mod identity;
mod identity_signatures;
mod imap_import;
mod invitations_http;
mod junk_training;
mod locale_preference_http;
mod meet_token;
mod out_of_office_http;
mod push_subscriptions_http;
mod quota;
mod reset_http;
mod search_snippet;
mod sieve;
mod signup_http;
mod submission_unconfigured;
mod tenant_isolation;
mod transcripts;
mod unsubscribe;
mod vacation;
mod working_hours_http;
