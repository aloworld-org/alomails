//! # alo-smtp — mail transfer and submission
//!
//! Owns: SMTP receiving (port 25), client submission (587), queueing,
//! routing, and retries (ARCHITECTURE.md). Does not own: message
//! storage (`alo-store`), DKIM/SPF/DMARC (`alo-auth-mail`), or
//! client APIs (`alo-jmap`).
//!
//! Phase 0 scope (ROADMAP.md exit gate): a session skeleton that
//! greets, negotiates EHLO, and quits with RFC 5321-correct replies.
//! The full session state machine, queueing, and the trust stack are
//! Phase 1 items.
//!
//! Layering: [`session`] is the pure protocol state machine (no I/O),
//! [`server`] puts it on a TCP socket with the read limits and
//! timeouts RFC 5321 requires, [`reply`] and [`command`] are the wire
//! vocabulary.

pub mod address;
pub mod auth;
pub mod authmail;
pub mod backoff;
/// A canned loopback HTTP responder shared by the Rspamd-facing tests.
#[cfg(test)]
pub mod canned_http;
// The outbound SMTP client + reply/line parsing live in the shared
// `alo-smtp-client` crate (also used by alo-jmap's submission path).
// Re-exported here so `crate::client` / `crate::client_reply` keep resolving.
pub use alo_smtp_client::{client, client_reply};
pub mod clamav;
pub mod command;
pub mod config;
pub mod connlimit;
pub mod data;
pub mod dkim_install;
pub mod dmarc_reporter;
pub mod dsn;
pub mod egress;
pub mod envelope;
pub mod error;
pub mod healthcheck;
use alo_smtp_client::line;
pub mod local_delivery;
pub mod mta_sts;
pub mod queue;
pub mod queue_runner;
pub mod received;
pub mod reply;
pub mod resolver;
pub mod rspamd;
pub mod sendrate;
pub mod server;
pub mod session;
pub mod spool;
pub mod stream;
pub mod submission;
pub mod tls;
