//! Executing `attachment_read` — the Drive agent pulling text out of what is
//! attached to one of the caller's own emails (ADR 0034, queue item A2.5).
//!
//! It lives beside [`crate::agent_drive`] rather than in it because it reads a
//! different thing: a Drive node is a row and a blob, while an attachment is a
//! MIME part that has to be parsed out of a message. One file, one reason to
//! change — the day mail parsing moves, this file moves with it and Drive's does
//! not.
//!
//! Why an attachment is Drive's tool and not Mail's: an attachment is a file
//! that has not been filed yet. The Mail agent's subject matter is
//! correspondence — who wrote, what we promised, who has not replied (A2.8) —
//! and "what does the spreadsheet they sent say" is a question about a file. The
//! two products do not share a tool name, which is the property
//! `alo_ai::agent_product`'s workspace test holds the registry to.
//!
//! Three rules shape this module:
//!
//! - **The email is named the way the user named it.** No message id is ever
//!   asked of the model: the subject goes through the caller's own workspace
//!   search ([`alo_store::AccountStore::workspace_search`]), which is scoped to
//!   this person's mail, and the resolver picks out of what it returns. A
//!   message of another tenant's — or of a colleague's mailbox — is not among
//!   the things that can be named.
//! - **Not naming an attachment is a question, not a guess.** Left out, the tool
//!   answers with the list: what is attached, what type each part is, and how
//!   big. The model then asks which one, or reads the only one there is.
//! - **A part whose bytes are not text is refused by name and by type.** There
//!   is no PDF or office extractor in this repo, and a lossy decode of one reads
//!   like text and summarises like nonsense — the one failure mode that turns an
//!   agent into a liar.

use axum::Json;
use serde_json::{Value, json};

use alo_store::MessageId;

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::agent_drive::{
    DEFAULT_TEXT_CHARS, MAX_TEXT_CHARS, looks_textual, not_text_named, textual_type,
};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::mime_read::{self, Attachment};
use crate::state::Account;

/// How many of the caller's messages a subject is matched against.
const MAX_CANDIDATES: i64 = 20;

/// The largest message this parses. A mail store holds the odd multi-megabyte
/// message and parsing one costs more than the turn is worth.
const MAX_MESSAGE_BYTES: usize = 25 * 1024 * 1024;

/// `attachment_read` — what is attached to an email, and the text of one part.
///
/// # Errors
/// `422` when no message of the caller's matches the subject, when the message
/// carries no such attachment, or when the named part's bytes are not text; the
/// store's own failure otherwise.
pub async fn execute_attachment_read(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let wanted = string_arg(args, "email")
        .ok_or_else(|| unprocessable("say which email, by its subject"))?;
    let (id, subject) = resolve_message(account, &wanted).await?;

    let raw = account
        .acc
        .message_bytes(&MessageId::new(id.clone()))
        .await
        .map_err(map_store_err)?;
    if raw.len() > MAX_MESSAGE_BYTES {
        return Err(unprocessable(format!(
            "the email {subject} is too large for the agent to open"
        )));
    }
    let parts = mime_read::parse(&raw).attachments;
    let email = json!({ "id": id, "subject": subject });

    // Nothing attached is an answer, not a failure: the model says so rather
    // than inventing what "the attachment" said.
    if parts.is_empty() {
        return Ok(Json(json!({
            "ok": true,
            "result": {
                "kind": "emailAttachments",
                "email": email,
                "attachments": [],
                "reason": "noAttachments",
            }
        })));
    }

    let Some(named) = string_arg(args, "attachment") else {
        // Which ones there are, so the next turn can name one. Deliberately not
        // "read the only one automatically": a person who said "the attachment"
        // about a message with three of them meant a specific one.
        return Ok(Json(json!({
            "ok": true,
            "result": {
                "kind": "emailAttachments",
                "email": email,
                "attachments": parts.iter().map(part_ref).collect::<Vec<_>>(),
                "reason": Value::Null,
            }
        })));
    };

    let part = pick(
        &named,
        parts
            .iter()
            .map(|part| (part.name.as_str(), part))
            .collect(),
        "attachment",
    )?;
    // Refused on what the message itself declares, before any bytes are
    // decoded — so a 20 MB PDF is never read to discover it is a PDF.
    if !readable(part) {
        return Err(not_text_named(&part.name, &describe(part)));
    }
    let (bytes, content_type, name) =
        mime_read::attachment_bytes(&raw, part.index).ok_or_else(|| {
            unprocessable(format!("{} could not be read out of that email", part.name))
        })?;
    let text = String::from_utf8(bytes).map_err(|_| not_text_named(&name, &describe(part)))?;

    let wanted_chars = args
        .get("chars")
        .and_then(Value::as_u64)
        .and_then(|chars| usize::try_from(chars).ok())
        .unwrap_or(DEFAULT_TEXT_CHARS)
        .clamp(1, MAX_TEXT_CHARS);
    let truncated = text.chars().count() > wanted_chars;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "emailAttachmentText",
            "email": email,
            "attachment": { "name": name, "contentType": content_type, "size": part.size },
            "text": text.chars().take(wanted_chars).collect::<String>(),
            "truncated": truncated,
            "words": text.split_whitespace().count(),
        }
    })))
}

/// The message a subject names, out of the caller's own mail.
///
/// `workspace_search` is the same door the search box is: personal, tenant
/// scoped, and returning mail only where this person is a participant. Drive
/// nodes and tasks come back from it too and are dropped here — an email is what
/// was asked for.
async fn resolve_message(account: &Account, wanted: &str) -> Result<(String, String), Problem> {
    let hits = account
        .acc
        .workspace_search(wanted, MAX_CANDIDATES)
        .await
        .map_err(map_store_err)?;
    let messages: Vec<(String, String)> = hits
        .into_iter()
        .filter(|hit| hit.kind == "message")
        .map(|hit| (hit.id, hit.title))
        .collect();
    if messages.is_empty() {
        return Err(unprocessable(format!("no email of yours matches {wanted}")));
    }
    let picked = pick(
        wanted,
        messages
            .iter()
            .map(|(id, subject)| (subject.as_str(), id.clone()))
            .collect(),
        "email",
    )?;
    let subject = messages
        .iter()
        .find(|(id, _)| *id == picked)
        .map_or_else(String::new, |(_, subject)| subject.clone());
    Ok((picked, subject))
}

/// Whether a part's own headers say it is text.
///
/// The part's declared type first, its filename second — the same two facts
/// [`looks_textual`] weighs for a Drive node, so "what can the agent read" has
/// one answer in this product rather than two that drift.
fn readable(part: &Attachment) -> bool {
    textual_type(&part.content_type.to_lowercase()) || looks_textual(&part.name, None)
}

/// What a part is, in the words a refusal uses.
fn describe(part: &Attachment) -> String {
    if part.content_type.is_empty() {
        "not a text file".to_owned()
    } else {
        format!("a {} attachment", part.content_type)
    }
}

/// One attachment, as the list reports it.
fn part_ref(part: &Attachment) -> Value {
    json!({
        "name": part.name,
        "contentType": part.content_type,
        "size": part.size,
        // Said so the model does not offer to read a signature image as if it
        // were a document somebody attached on purpose.
        "inline": part.inline,
        "readable": readable(part),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, content_type: &str) -> Attachment {
        Attachment {
            index: 0,
            name: name.to_owned(),
            content_type: content_type.to_owned(),
            size: 10,
            content_id: None,
            inline: false,
        }
    }

    /// What the agent will open, and what it will refuse by name. The PDF and
    /// the spreadsheet are the cases this whole module is careful about: there
    /// is no extractor for either, so the honest answer is a refusal.
    #[test]
    fn a_part_is_readable_by_its_type_or_its_filename_and_office_files_are_neither() {
        assert!(readable(&part("notes.txt", "text/plain")));
        assert!(readable(&part("data.csv", "text/csv")));
        assert!(readable(&part("payload", "application/json")));
        // Mislabelled by the sender, still plainly a text file by its name.
        assert!(readable(&part("notes.md", "application/octet-stream")));
        assert!(!readable(&part("report.pdf", "application/pdf")));
        assert!(!readable(&part("q3.xlsx", "application/octet-stream")));
        assert!(!readable(&part("scan.png", "image/png")));
    }

    /// A refusal names the part and what it is — never "unsupported".
    #[test]
    fn a_refusal_names_the_attachment_and_what_it_is() {
        let problem = not_text_named(
            "report.pdf",
            &describe(&part("report.pdf", "application/pdf")),
        );
        let detail = problem.detail.unwrap_or_default();
        assert!(detail.contains("report.pdf"), "{detail}");
        assert!(detail.contains("application/pdf"), "{detail}");
        assert!(detail.contains("say so rather than describing"), "{detail}");
    }
}
