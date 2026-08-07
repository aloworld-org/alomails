//! **Server-composed drafts** — the one path by which a message alo writes on
//! a user's behalf reaches their mailbox.
//!
//! Everything the product composes for a user — an agent's reply (ADR 0034), a
//! covering email for an invoice ([`crate::billing_send`]), a payment reminder
//! — lands in **Drafts**, marked `$draft`, for the user to read and send
//! themselves. Nothing here leaves the server: sending is the ordinary
//! submission path ([`crate::submission`]), which the user triggers, which
//! signs with DKIM and which refuses anything that is not a `$draft`.
//!
//! Two rules are held here rather than at each caller, because a caller that
//! got either wrong would be a security bug rather than a cosmetic one:
//!
//! - **The author is resolved server-side.** [`from_address`] reads the
//!   caller's own canonical address out of the store; no composer — model,
//!   route or client — chooses who a draft is from, so a draft can never
//!   impersonate another author even before it is sent.
//! - **The mailbox is the caller's own.** Every write goes through
//!   `account.acc`, the tenant-scoped door, so a draft can only ever be
//!   written into the mailbox of the account that asked for it.

use axum::http::StatusCode;

use alo_store::{MailboxId, MessageId};

use crate::error::Problem;
use crate::mime::Outgoing;
use crate::state::{Account, AppState};

/// Get-or-create the account's mailbox for a standard `role` (creating it with
/// `name` on first use). Every standard role is provisioned on demand: a first
/// draft on an account that never had a Drafts folder should succeed, not fail.
pub async fn role_mailbox(account: &Account, role: &str, name: &str) -> Result<MailboxId, Problem> {
    if let Some(id) = account
        .acc
        .mailbox_by_role(role)
        .await
        .map_err(|_| Problem::server_error())?
    {
        return Ok(id);
    }
    account
        .acc
        .create_mailbox(None, name, Some(role))
        .await
        .map_err(|_| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "could not open the mailbox",
            )
        })
}

/// The caller's own canonical send-from address.
///
/// Resolved from the store, never from the request, so nothing that composes a
/// message can choose its author. An account with no send address is a `422`
/// naming that fact — the only honest answer, since a draft with no `From` is
/// not a message anyone can send.
pub async fn from_address(account: &Account, state: &AppState) -> Result<String, Problem> {
    state
        .store
        .for_tenant(account.tenant.clone())
        .email_of(&account.user)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "this account has no send address",
            )
        })
}

/// Builds the message and saves it into Drafts (created on first use) with the
/// `$draft` keyword — the same path the JMAP `Email/set` create uses.
///
/// The user reviews it and sends it through the normal submission path; **this
/// never sends**.
pub async fn save(account: &Account, outgoing: &Outgoing) -> Result<MessageId, Problem> {
    let raw = crate::mime::build(outgoing);
    let drafts = role_mailbox(account, "drafts", "Drafts").await?;
    let id =
        account.acc.ingest(&drafts, &raw).await.map_err(|_| {
            Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "could not save the draft")
        })?;
    account
        .acc
        .set_keyword(&id, "$draft", true)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(id)
}
