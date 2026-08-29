//! The executors of alo Mail's verbs (ADR 0058, AC.4) — what runs when the
//! Mail agent uses one of the intents `alo_ai::mail_intents` describes.
//!
//! Every executor runs through the asker's account door. The verbs the old
//! tool set already had keep their executors where they were — the exchange
//! and the single message in [`crate::agent_correspondence`], the address
//! book in [`crate::agent_reads`], and the nine email actions beside the
//! agent routes in [`crate::agent`], where the draft, submission and folder
//! plumbing they reuse lives — and are dispatched from here so the agent has
//! one place to look. What this module itself executes is the mailbox as a
//! *subject*: what waits unread and where, one message's whole conversation,
//! and who the asker's own mail went to lately.
//!
//! Three bounds are deliberate:
//!
//! - **Unread is what arrived and waits.** `unread_summary` reads the same
//!   per-mailbox counters the JMAP `Mailbox/get` serves, and leaves out
//!   Drafts, Sent and the internal folders (Snoozed, Scheduled) — a draft
//!   is not waiting mail, and the snoozed return to the Inbox by
//!   themselves.
//! - **A thread is the store's own thread.** `thread_lookup` walks the
//!   message's `thread_id` (RFC 5322 references, see
//!   `alo_store::thread`), so what the agent calls a conversation is
//!   exactly what the mail screen threads — never a subject-line guess.
//! - **Sent mail is read from the Sent folder.** `who_i_emailed` queries
//!   the sent role mailbox over the same `query_emails` the JMAP query
//!   serves; an account that never sent anything gets an honest empty
//!   answer, not an error.

use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::{EmailFilter, EmailQuery, MessageId, Page, SortDirection};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_reads::iso;
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState};

/// The most folders one unread listing reports; past this the tail is named
/// by count so the model knows it is not looking at everything.
const MAX_FOLDERS: usize = 30;

/// The most thread messages one lookup lists, oldest first — a conversation
/// longer than this is cut at the newest end and says so.
const MAX_THREAD: usize = 30;

/// How many sent messages one `who_i_emailed` reads through, newest first.
const MAX_SENT: i64 = 100;

/// The default and ceiling for `who_i_emailed`'s look-back.
const DEFAULT_DAYS: i64 = 7;
const MAX_DAYS: i64 = 31;

type Reply = Result<axum::Json<Value>, Problem>;

fn ok(result: Value) -> Reply {
    Ok(axum::Json(json!({ "ok": true, "result": result })))
}

/// The caller's own send address, for saying which side of an exchange a
/// message came from. Empty for an account with no send address — the
/// comparison then never matches, and every message honestly reads "them".
async fn my_address(account: &Account, state: &AppState) -> String {
    state
        .store
        .for_tenant(account.tenant.clone())
        .email_of(&account.user)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_lowercase()
}

/// The folder roles an unread summary leaves out: a draft is not waiting
/// mail, sent mail was never unread for the sender, and the internal
/// folders return their messages to the Inbox by themselves.
fn not_waiting(role: Option<&str>) -> bool {
    matches!(role, Some("drafts" | "sent" | "snoozed" | "scheduled"))
}

/// `unread_summary` — what waits unread, folder by folder.
///
/// # Errors
/// The store's own failure only; an account with no unread mail is an honest
/// zero, never an error.
pub async fn execute_unread_summary(account: &Account, _args: &Value) -> Reply {
    let boxes = account
        .acc
        .mailboxes(Page::first(alo_store::MAX_PAGE))
        .await
        .map_err(map_store_err)?;
    let counted: Vec<&alo_store::Mailbox> = boxes
        .iter()
        .filter(|mailbox| !not_waiting(mailbox.role.as_deref()))
        .collect();
    let total_unread: i64 = counted.iter().map(|mailbox| mailbox.unread_messages).sum();
    // The Inbox is always listed, empty included — "nothing waits" is the
    // answer the question is usually hoping for. Other folders earn a line
    // by holding something unread.
    let listed: Vec<&&alo_store::Mailbox> = counted
        .iter()
        .filter(|mailbox| mailbox.unread_messages > 0 || mailbox.role.as_deref() == Some("inbox"))
        .collect();
    let folders: Vec<Value> = listed
        .iter()
        .take(MAX_FOLDERS)
        .map(|mailbox| {
            json!({
                "folder": mailbox.name,
                "role": mailbox.role,
                "unread": mailbox.unread_messages,
                "total": mailbox.total_messages,
            })
        })
        .collect();
    ok(json!({
        "kind": "unreadSummary",
        "totalUnread": total_unread,
        "folders": folders,
        "truncated": listed.len() > MAX_FOLDERS,
    }))
}

/// The resolved `message` argument — an id the caller was given by an
/// earlier result, looked up through the account door so a foreign or
/// invented id earns the same words.
async fn own_message(account: &Account, args: &Value) -> Result<alo_store::Message, Problem> {
    let id = string_arg(args, "message")
        .ok_or_else(|| unprocessable("say which message, by the \"id\" a result gave you"))?;
    account
        .acc
        .message(&MessageId::new(id))
        .await
        .map_err(|_| unprocessable("no message of yours has that id"))
}

/// One thread entry, in the fields a listing reports — metadata only; the
/// text of any one message is `message_read`'s to open.
fn thread_line(message: &alo_store::Message, mine: &str) -> Value {
    let ours = !mine.is_empty() && message.from_addr.to_lowercase().contains(mine);
    json!({
        "id": message.id.as_str(),
        "subject": message.subject,
        "from": message.from_addr,
        "at": iso(message.sent_at.unwrap_or(message.received_at)),
        "direction": if ours { "us" } else { "them" },
        "hasAttachment": message.has_attachment,
    })
}

/// `thread_lookup` — one message's whole conversation, oldest first.
///
/// # Errors
/// 422 when no message was named or the id is not one of this account's —
/// the account door makes a foreign id and an invented one the same words.
pub async fn execute_thread_lookup(account: &Account, args: &Value, state: &AppState) -> Reply {
    let opened = own_message(account, args).await?;
    let ids = account
        .acc
        .thread_messages(&opened.thread_id, Page::first(alo_store::MAX_PAGE))
        .await
        .map_err(map_store_err)?;
    let mut thread = Vec::with_capacity(ids.len());
    for id in &ids {
        // Each fetch goes back through the same door; a message deleted
        // between the two queries is skipped rather than an error.
        if let Ok(message) = account.acc.message(id).await {
            thread.push(message);
        }
    }
    thread.sort_by_key(|message| message.sent_at.unwrap_or(message.received_at));
    let mine = my_address(account, state).await;
    let total = thread.len();
    let listed: Vec<Value> = thread
        .iter()
        .take(MAX_THREAD)
        .map(|message| thread_line(message, &mine))
        .collect();
    ok(json!({
        "kind": "thread",
        "about": opened.subject,
        "count": total,
        "messages": listed,
        "truncated": total > MAX_THREAD,
    }))
}

/// `who_i_emailed` — who the asker's own sent mail went to lately.
///
/// # Errors
/// The store's own failure only. An account with no Sent folder, or nothing
/// sent in the period, is an honest empty list — that too answers the
/// question.
pub async fn execute_who_i_emailed(account: &Account, args: &Value, state: &AppState) -> Reply {
    let days = args
        .get("days")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_DAYS)
        .clamp(1, MAX_DAYS);
    let Some(sent) = account
        .acc
        .mailbox_by_role("sent")
        .await
        .map_err(map_store_err)?
    else {
        return ok(json!({
            "kind": "whoIEmailed",
            "days": days,
            "totalMessages": 0,
            "people": [],
            "truncated": false,
        }));
    };
    let after = OffsetDateTime::now_utc() - time::Duration::days(days);
    let summaries = account
        .acc
        .query_emails(&EmailQuery {
            filter: EmailFilter {
                in_mailbox: Some(sent),
                after: Some(after),
                ..EmailFilter::default()
            },
            sort: SortDirection::Desc,
            page: Page::first(MAX_SENT),
        })
        .await
        .map_err(map_store_err)?;
    let mine = my_address(account, state).await;
    // Grouped by address, newest first — the first time an address is seen
    // is its most recent message, so the group order is the recency order.
    let mut people: Vec<(String, i64, String, String)> = Vec::new();
    for summary in &summaries {
        let Ok(message) = account.acc.message(&summary.id).await else {
            continue;
        };
        let at = iso(message.sent_at.unwrap_or(message.received_at));
        for header in [&message.to_addrs, &message.cc_addrs] {
            for addr in crate::agent::addr_specs(header) {
                if !mine.is_empty() && addr == mine {
                    continue;
                }
                if let Some(person) = people.iter_mut().find(|(known, ..)| *known == addr) {
                    person.1 += 1;
                } else {
                    people.push((addr, 1, message.subject.clone(), at.clone()));
                }
            }
        }
    }
    let listed: Vec<Value> = people
        .iter()
        .map(|(address, count, last_subject, last_at)| {
            json!({
                "address": address,
                "messages": count,
                "lastSubject": last_subject,
                "lastAt": last_at,
            })
        })
        .collect();
    ok(json!({
        "kind": "whoIEmailed",
        "days": days,
        "totalMessages": summaries.len(),
        "people": listed,
        "truncated": summaries.len() == usize::try_from(MAX_SENT).unwrap_or(usize::MAX),
    }))
}

/// The module's verbs by name (A4.1c) — Mail's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The verbs the old tool set already
/// had keep their executors in [`crate::agent_correspondence`],
/// [`crate::agent_reads`] and [`crate::agent`].
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "correspondence" => Box::pin(crate::agent_correspondence::execute_correspondence(
            account, args, state,
        )),
        "message_read" => Box::pin(crate::agent_correspondence::execute_message_read(
            account, args,
        )),
        "unread_summary" => Box::pin(execute_unread_summary(account, args)),
        "thread_lookup" => Box::pin(execute_thread_lookup(account, args, state)),
        "who_i_emailed" => Box::pin(execute_who_i_emailed(account, args, state)),
        "find_contact" => Box::pin(crate::agent_reads::execute_find_contact(account, args)),
        "mark_read" => Box::pin(crate::agent::execute_set_keyword(
            account, args, "$seen", "read",
        )),
        "flag_email" => Box::pin(crate::agent::execute_set_keyword(
            account, args, "$flagged", "flagged",
        )),
        "archive_email" => Box::pin(crate::agent::execute_move_to_role(
            account, args, "archive", "Archive",
        )),
        "trash_email" => Box::pin(crate::agent::execute_move_to_role(
            account, args, "trash", "Trash",
        )),
        "snooze_email" => Box::pin(crate::agent::execute_snooze(account, args)),
        "draft_email" => Box::pin(crate::agent::execute_draft_email(account, args, state)),
        "draft_reply" => Box::pin(crate::agent::execute_draft_reply(account, args, state)),
        "send_email" => Box::pin(crate::agent::execute_send(account, args, state)),
        "move_to_folder" => Box::pin(crate::agent::execute_move_to_folder(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::mail_intents::MAIL;

    /// Every mail-side HTTP route the router registers is the adapter of a
    /// verb or excluded with a reason — the coverage ADR 0058 makes
    /// structural. Mail's own app surface is JMAP, so the prefixes here are
    /// the address book's and the autoconfig file's.
    #[test]
    fn every_mail_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        for prefix in ["/contacts", "/mail"] {
            let missing = MAIL.uncovered(router, prefix);
            assert!(
                missing.is_empty(),
                "routes with neither a verb nor a reason: {missing:?}"
            );
        }
        // …and every verb's route exists, so an intent cannot name a route
        // the app does not have.
        let routes = alo_ai::routes_in(router, "/contacts");
        for intent in MAIL.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("mail_intents.rs");
        for intent in MAIL.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Mail's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, and the two lists are the same
    /// length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("mail_intents::").count(),
            1,
            "agent.rs names Mail only in MODULES"
        );
        assert!(agent.contains("crate::mail_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    /// AC.4's rail, held structurally: this module adds no second send
    /// path. The one executor that delivers mail is [`crate::agent`]'s
    /// `execute_send`, which submits only a `$draft` through the same
    /// audited JMAP submission the compose screen uses — nothing here
    /// composes and sends in one move.
    #[test]
    fn there_is_no_second_send_path() {
        let source = include_str!("mail_intents.rs");
        // The needles are split so this test's own source does not match.
        let send = concat!("execute_", "send");
        assert_eq!(
            source.matches(send).count(),
            2,
            "a second send executor joined the module"
        );
        let submit = concat!("submission", "::set");
        assert!(
            !source.contains(submit),
            "this module reaches the submission path directly"
        );
    }
}
