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
pub mod ai;
pub mod api;
pub mod autoconfig;
pub mod blob;
pub mod calendar;
pub mod carddav;
pub mod contacts;
pub mod delegates;
pub mod docs;
pub mod error;
pub mod filters;
pub mod flagdue;
pub mod imap_import;
pub mod imap_import_route;
pub mod jtypes;
pub mod junk_learn;
pub mod mime;
pub mod mime_read;
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
pub mod snooze;
pub mod spaces;
pub mod state;
pub mod submission;
pub mod tasks;
pub mod unsubscribe;

pub use push::PushHub;
pub use server::{app, app_state, serve};
pub use state::{AppState, Limits};
