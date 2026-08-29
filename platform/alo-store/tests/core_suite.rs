//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod agent_ground;
mod agent_tool_runs;
mod app_passwords_tenancy;
mod categories;
mod chat_agent_dm;
mod chat_agent_product;
mod chat_agent_seed;
mod chat_agents;
mod concurrency;
mod contacts;
mod crash_safety;
mod delegation;
mod documents_tenancy;
mod events;
mod evidence;
mod flag_due;
mod group_lists;
mod group_rename;
mod ical_corpus;
mod mapi_contents_tenancy;
mod mapi_ids;
mod meet;
mod missing_blob_is_logged;
mod out_of_office_window;
mod push_subscriptions_tenancy;
mod record_origins;
mod sieve;
mod snooze;
mod store_behaviors;
mod tenant_isolation;
mod tenant_roles;
mod threading;
mod user_invites;
mod user_modules;
