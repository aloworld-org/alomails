//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod agent_agenda_http;
mod agent_agenda_intents_http;
mod agent_billing_intents_http;
mod agent_chat_intents_http;
mod agent_correspondence_http;
mod agent_crm_intents_http;
mod agent_delegation_http;
mod agent_directory_http;
mod agent_dm_http;
mod agent_docs_http;
mod agent_docs_intents_http;
mod agent_drive_http;
mod agent_drive_intents_http;
mod agent_events_http;
mod agent_finance_intents_http;
mod agent_hr_intents_http;
mod agent_insights_http;
mod agent_insights_intents_http;
mod agent_instructions_http;
mod agent_inventory_intents_http;
mod agent_isolation_http;
mod agent_mail_intents_http;
mod agent_meet_http;
mod agent_meet_intents_http;
mod agent_memory_http;
mod agent_orchestrate_http;
mod agent_projects_intents_http;
mod agent_reads_answer_http;
mod agent_seed_http;
mod agent_sheets_http;
mod agent_sheets_intents_http;
mod agent_sites_http;
mod agent_sites_intents_http;
mod agent_tasks_http;
mod agent_tasks_intents_http;
mod agent_two_questions_http;
