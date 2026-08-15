//! Executing the Mail agent's **reading** tools — the answer half of the agent
//! of mail (ADR 0034, queue item A2.8).
//!
//! Until this module existed, every one of Mail's nine tools was a write. "Are
//! we in contact with ABC?" was answered from retrieval: the turn's numbered
//! sources happened to include two messages whose *subjects* matched the words
//! in the question, and the agent wrote a sentence over them. That answer can be
//! right, and it is right for the wrong reason — a search hit is a snippet that
//! mentions somebody, not the record of what passed between you and them. Ask it
//! "who last replied?" and there is nothing in a snippet to answer from; ask it
//! "what did we promise them?" and it will paraphrase a subject line.
//!
//! So the two tools here answer from the mailbox itself:
//!
//! - [`execute_correspondence`] — everything exchanged with one person or
//!   company, **in both directions**, newest first: who wrote, when, and whether
//!   the last word was theirs or yours. It is the answer to "are we in contact
//!   with X" and to "who last replied", and it opens the nearest few messages so
//!   the ordinary follow-up is already in hand.
//! - [`execute_message_read`] — one message of that exchange, in full, when the
//!   previews are not enough to say exactly what was said.
//!
//! Four rules shape it:
//!
//! - **The asker's own door, and only it.** Both run on
//!   [`alo_store::AccountStore`], whose every mail query is scoped to this
//!   tenant *and* this user, so an agent reaches exactly the mail the person who
//!   asked could open and a colleague's correspondence does not exist here.
//!   That includes the address book [`lookup_names`] reads: it is Mail's own,
//!   which is why `find_contact` is a Mail tool, so resolving a company name is
//!   the same door and not a wider one.
//! - **A message that was listed is not a message that was read.** Each entry
//!   says `opened` plainly, and the guidance in [`alo_ai::agent_mail`] forbids
//!   speaking to the contents of one that was not. A subject line is evidence of
//!   a subject and of nothing else.
//! - **Direction is a fact, not an inference from the words.** A message whose
//!   `From` is the account's own address was sent by us; failing that, one whose
//!   `From` names the counterpart came from them. Nothing here reads a folder
//!   name — mail filed by hand would answer differently on Tuesday.
//! - **Ids come from a result, never from the model's imagination.**
//!   [`execute_message_read`] takes the `id` of a message and the store refuses
//!   any that is not this account's, which is the same refusal a wrong-tenant id
//!   earns.

use axum::Json;
use serde_json::{Value, json};

use alo_store::{EmailFilter, EmailQuery, MessageId, MessageSummary, Page, SortDirection};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_reads::iso;
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::mime_read::{self, Attachment};
use crate::state::{Account, AppState};

/// How many messages of an exchange are listed by default, and the most that
/// may be asked for. Bounded because the whole result is rendered into a
/// model's context beside the question it is meant to answer: a year of a busy
/// correspondence would crowd the question out.
const DEFAULT_LIMIT: i64 = 8;
const MAX_LIMIT: i64 = 15;

/// How many of the listed messages are opened and their text read out. Opening
/// one means fetching and parsing its MIME, so this is deliberately smaller
/// than the list: an exchange can run to a folder's worth of mail, and the
/// question is almost always about the recent end of it.
const MAX_OPENED: usize = 3;

/// How much of an opened message's body goes into the list. Enough to see what
/// a message was about; [`execute_message_read`] is how the rest is reached.
const PREVIEW_CHARS: usize = 600;

/// How much of one message's body [`execute_message_read`] returns. Larger than
/// a preview, and still bounded — the tool result is truncated at
/// `agent_turn::MAX_RESULT_CHARS` whatever this says, and a bound that says so
/// in the payload is better than one that silently cuts prose in half.
const READ_CHARS: usize = 3_000;

/// The largest message this parses — the same ceiling `attachment_read` uses.
/// A 25 MB mail is a delivery of files, and there is no answer in it that a
/// preview would find.
const MAX_MESSAGE_BYTES: usize = 25 * 1024 * 1024;

/// How many strings one name is looked for under, the name itself included.
/// Each one costs a pair of queries over the mailbox, so a name that reaches a
/// crowded address book widens the search a little and never without bound.
const MAX_LOOKUPS: usize = 4;

/// `correspondence` — the exchange with one person or company, both ways.
///
/// # Errors
/// 422 when nobody was named; the store's own failure otherwise.
pub async fn execute_correspondence(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let who =
        string_arg(args, "who").ok_or_else(|| unprocessable("say who, by name or address"))?;
    // Optional: the same exchange, narrowed to the messages whose words match.
    // "What did we promise them about delivery" is one question, not two.
    let about = string_arg(args, "about");
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);

    // What the name is actually looked for under — see [`lookup_names`].
    let lookups = lookup_names(account, &who).await?;

    // Two queries per name because the mailbox stores the two directions in two
    // columns: one for the mail they sent, one for the mail addressed to them.
    // Merged and de-duplicated below — a message satisfies both when somebody is
    // copied on their own thread, and several when two names reach it.
    let mut found = Vec::new();
    for needle in &lookups {
        for by_sender in [true, false] {
            let filter = EmailFilter {
                from: by_sender.then(|| needle.clone()),
                to: (!by_sender).then(|| needle.clone()),
                text: about.clone(),
                ..EmailFilter::default()
            };
            found.extend(
                account
                    .acc
                    .query_emails(&EmailQuery {
                        filter,
                        sort: SortDirection::Desc,
                        page: Page::first(limit),
                    })
                    .await
                    .map_err(map_store_err)?,
            );
        }
    }
    let messages = newest_first(found, limit);

    // Whose mail this account is, so "we wrote" and "they wrote" are read off
    // the sender rather than guessed from a folder. Absent for an account with
    // no send address, which the fallback in [`direction`] covers.
    let mine = state
        .store
        .for_tenant(account.tenant.clone())
        .email_of(&account.user)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let mut listed = Vec::with_capacity(messages.len());
    for (position, summary) in messages.iter().enumerate() {
        let side = direction(&summary.from_addr, &lookups, &mine);
        let mut entry = json!({
            "id": summary.id.as_str(),
            "subject": summary.subject,
            "from": summary.from_addr,
            "at": iso(summary.sent_at.unwrap_or(summary.received_at)),
            "direction": side,
            // Said plainly: an email that was listed and not opened is evidence
            // of a subject line and of nothing else.
            "opened": position < MAX_OPENED,
        });
        if position < MAX_OPENED
            && let Some(raw) = raw_message(account, &summary.id).await
        {
            let parsed = mime_read::parse(&raw);
            let text = parsed.text.unwrap_or_default();
            entry["preview"] = json!(text.chars().take(PREVIEW_CHARS).collect::<String>());
            entry["previewTruncated"] = json!(text.chars().count() > PREVIEW_CHARS);
            entry["attachments"] = json!(names_of(&parsed.attachments));
        }
        listed.push(entry);
    }

    let last_from = |side: &str| {
        listed
            .iter()
            .find(|entry| entry["direction"] == json!(side))
            .map(|entry| {
                json!({
                    "id": entry["id"],
                    "subject": entry["subject"],
                    "at": entry["at"],
                })
            })
    };
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "correspondence",
            "who": who,
            // What was actually asked of the mailbox, so an empty answer reads
            // as "no mail matches this name" rather than as "there is nothing"
            // — and so an agent can say it looked under the company's address
            // as well as under the words it was given.
            "about": about,
            "lookedFor": lookups,
            // The question "are we in contact with them", answered as a fact
            // rather than left to be inferred from an empty list.
            "inContact": !listed.is_empty(),
            // The question "who last replied", answered from the newest message
            // in either direction: "them", "us", or nobody at all.
            "lastReplyBy": listed.first().map(|entry| entry["direction"].clone()),
            "lastFromThem": last_from("them"),
            "lastFromUs": last_from("us"),
            "messages": listed,
            // Said because the list is bounded: an exchange cut at the limit is
            // not the whole exchange, and an agent should say so.
            "limit": limit,
            "openedAtMost": MAX_OPENED,
        }
    })))
}

/// `message_read` — one message of the caller's own mail, in full.
///
/// # Errors
/// 422 when no message was named, when the id is not one of this account's, or
/// when the message is too large to parse.
pub async fn execute_message_read(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let wanted = string_arg(args, "message").ok_or_else(|| {
        unprocessable("say which message, by the id a correspondence result gave")
    })?;
    let id = MessageId::new(wanted);
    // The store's own scoping is the check: a message id from another tenant,
    // another user, or nowhere at all is equally not found.
    let message = account
        .acc
        .message(&id)
        .await
        .map_err(|_| unprocessable("that is not one of your messages"))?;
    let raw = raw_message(account, &id)
        .await
        .ok_or_else(|| unprocessable("that message is too large to read here"))?;
    let parsed = mime_read::parse(&raw);
    let text = parsed.text.unwrap_or_default();
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "messageRead",
            "id": message.id.as_str(),
            "subject": message.subject,
            "from": message.from_addr,
            "to": message.to_addrs,
            "cc": message.cc_addrs,
            "at": iso(message.sent_at.unwrap_or(message.received_at)),
            "text": text.chars().take(READ_CHARS).collect::<String>(),
            "truncated": text.chars().count() > READ_CHARS,
            // Named, never read out: pulling the text out of an attachment is
            // `attachment_read`, which belongs to the Drive agent.
            "attachments": names_of(&parsed.attachments),
        }
    })))
}

/// The raw bytes of one of the caller's messages, or `None` when it cannot be
/// opened or is too large to parse.
///
/// Shared with [`crate::agent_meeting`], whose briefing opens the mail that goes
/// with a meeting: two copies of "how big a message may an agent open" would
/// eventually disagree, and the larger one would be the real one.
pub(crate) async fn raw_message(account: &Account, id: &MessageId) -> Option<bytes::Bytes> {
    let raw = account.acc.message_bytes(id).await.ok()?;
    (raw.len() <= MAX_MESSAGE_BYTES).then_some(raw)
}

/// The strings a name is looked for under: what the person said, plus what
/// their own address book says that name means.
///
/// Without this the headline question does not work. A mailbox stores
/// `ilse@abc-supplies.test` and `orders@abc-supplies.test`; nobody asking "are
/// we in contact with ABC Supplies?" types a hyphen, and a substring search for
/// the words they did type matches nothing at all. So a name that reaches
/// somebody in the address book also reaches **their addresses**, and — only
/// when the name is recognisably the domain itself — that **domain**, which is
/// what makes a company's other people part of the company's correspondence.
///
/// The domain rule is deliberately narrow. Widening to a contact's domain
/// unconditionally would make everyone at a webmail provider a colleague of the
/// one person filed under it, which is a far worse answer than a narrow one:
/// [`domain_is_the_name`] is the guard.
async fn lookup_names(account: &Account, who: &str) -> Result<Vec<String>, Problem> {
    let asked = who.trim();
    let needle = asked.to_lowercase();
    let mut names = vec![asked.to_owned()];
    let mut domains = Vec::new();
    let mut addresses = Vec::new();
    for contact in account.acc.contacts().await.map_err(map_store_err)? {
        let known = contact.display_name.to_lowercase().contains(&needle)
            || contact
                .organization
                .as_deref()
                .is_some_and(|org| org.to_lowercase().contains(&needle))
            || contact
                .emails
                .iter()
                .any(|email| email.value.to_lowercase().contains(&needle));
        if !known {
            continue;
        }
        for email in &contact.emails {
            let address = email.value.trim();
            if address.is_empty() {
                continue;
            }
            match address.rsplit_once('@') {
                Some((_, domain)) if domain_is_the_name(domain, asked) => {
                    push_once(&mut domains, domain);
                }
                _ => push_once(&mut addresses, address),
            }
        }
    }
    // Domains first: one of them stands for every address under it, so it is
    // the entry worth keeping when the bound bites. Each is added once, so
    // asking under a domain outright does not spend the budget twice.
    for extra in domains.into_iter().chain(addresses) {
        push_once(&mut names, &extra);
    }
    names.truncate(MAX_LOOKUPS);
    Ok(names)
}

/// Whether a domain is what somebody meant by a name — its letters and digits,
/// in order, inside the domain's own. "ABC Supplies" is `abc-supplies.test`;
/// "Ilse Vermeer" is not `gmail.com`.
fn domain_is_the_name(domain: &str, asked: &str) -> bool {
    let squashed = squash(asked);
    !squashed.is_empty() && squash(domain).contains(&squashed)
}

/// A string with everything but its letters and digits removed, lowercased —
/// so a hyphen, a space and a dot are all the same nothing.
fn squash(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Append `value` unless the list already holds it, ignoring case.
fn push_once(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|held| held.eq_ignore_ascii_case(value)) {
        list.push(value.to_owned());
    }
}

/// Which side of the exchange a message came from.
///
/// The account's own address decides it, because that is what "we" means. When
/// the account has no send address to compare against, a `From` naming the
/// counterpart is theirs and anything else is ours — which is exactly the shape
/// of the two queries the list was built from.
fn direction(from_addr: &str, lookups: &[String], mine: &str) -> &'static str {
    let from = from_addr.to_lowercase();
    if !mine.is_empty() && from.contains(&mine.to_lowercase()) {
        return "us";
    }
    if lookups
        .iter()
        .any(|needle| from.contains(&needle.trim().to_lowercase()))
    {
        "them"
    } else {
        "us"
    }
}

/// The two directions' results as one list, newest first, each message once.
///
/// Pure, so the merge rule is testable without a mailbox. `query_emails`
/// answers each direction newest-first already; interleaving them is what makes
/// "the newest message either way" the first entry, which is what
/// `lastReplyBy` reads.
fn newest_first(found: Vec<MessageSummary>, limit: i64) -> Vec<MessageSummary> {
    let mut merged = found;
    merged.sort_by(|a, b| {
        b.sent_at
            .unwrap_or(b.received_at)
            .cmp(&a.sent_at.unwrap_or(a.received_at))
            .then_with(|| b.id.as_str().cmp(a.id.as_str()))
    });
    merged.dedup_by(|a, b| a.id == b.id);
    merged.truncate(usize::try_from(limit).unwrap_or(0));
    merged
}

/// What is attached to a message, by name and type — never its bytes.
fn names_of(attachments: &[Attachment]) -> Vec<Value> {
    attachments
        .iter()
        .map(|part| {
            json!({
                "name": part.name,
                "contentType": part.content_type,
                "size": part.size,
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alo_store::ThreadId;
    use time::{Date, Month, OffsetDateTime};

    fn at(day: u8) -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::August, day)
            .unwrap()
            .with_hms(9, 0, 0)
            .unwrap()
            .assume_utc()
    }

    fn summary(id: &str, day: u8, from: &str) -> MessageSummary {
        MessageSummary {
            id: MessageId::new(id.to_owned()),
            thread_id: ThreadId::new("t".to_owned()),
            subject: format!("about {id}"),
            from_addr: from.to_owned(),
            sent_at: Some(at(day)),
            received_at: at(day),
            size: 100,
        }
    }

    /// The two directions are one exchange: newest first, whichever side it came
    /// from, and a message that satisfied both queries appears once.
    #[test]
    fn the_two_directions_merge_into_one_exchange_newest_first() {
        let found = vec![
            summary("m-old", 3, "them@abc.test"),
            summary("m-new", 9, "them@abc.test"),
            // The same message, back from the other direction's query.
            summary("m-new", 9, "them@abc.test"),
            summary("m-mid", 6, "us@example.test"),
        ];
        let merged = newest_first(found, 10);
        assert_eq!(
            merged.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["m-new", "m-mid", "m-old"],
            "newest first, and the duplicate collapsed"
        );
        // The limit is the caller's, and it cuts the oldest end.
        let merged = newest_first(
            vec![
                summary("m-old", 3, "them@abc.test"),
                summary("m-new", 9, "them@abc.test"),
            ],
            1,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id.as_str(), "m-new");
        assert!(newest_first(Vec::new(), 5).is_empty());
    }

    /// The message with no `Date` header is placed by when it arrived rather
    /// than dropped to the bottom — a message with no date is still a message,
    /// and "who last replied" would be wrong if it sorted as the oldest.
    #[test]
    fn a_message_without_a_date_header_is_placed_by_when_it_arrived() {
        let mut undated = summary("m-undated", 9, "them@abc.test");
        undated.sent_at = None;
        let merged = newest_first(vec![summary("m-old", 3, "them@abc.test"), undated], 10);
        assert_eq!(merged[0].id.as_str(), "m-undated");
    }

    /// Which side a message came from — the account's own address deciding it,
    /// and the counterpart's name deciding it when there is no own address to
    /// compare against.
    fn under(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_message_is_ours_when_we_sent_it_and_theirs_otherwise() {
        let mine = "us@example.test";
        let abc = under(&["ABC"]);
        assert_eq!(
            direction("Us <US@Example.test>", &abc, mine),
            "us",
            "our own address decides it, whatever the case"
        );
        assert_eq!(direction("ilse@abc-supplies.test", &abc, mine), "them");
        // No send address on the account: the counterpart's name decides it.
        assert_eq!(direction("ilse@abc-supplies.test", &abc, ""), "them");
        assert_eq!(direction("someone@else.test", &abc, ""), "us");
        // A display name is matched as readily as an address, because the
        // header carries both and people say the name.
        assert_eq!(
            direction(
                "Ilse Vermeer <ilse@abc-supplies.test>",
                &under(&["ilse vermeer"]),
                ""
            ),
            "them"
        );
        // Blanks around what the user typed do not change whose message it is.
        assert_eq!(
            direction("ilse@abc-supplies.test", &under(&["  ABC  "]), ""),
            "them"
        );
        // A message from anybody the name was looked up under is theirs — which
        // is what makes a colleague of theirs part of the same exchange.
        let company = under(&["ABC Supplies", "abc-supplies.test"]);
        assert_eq!(
            direction("orders@abc-supplies.test", &company, mine),
            "them"
        );
        assert_eq!(
            direction("orders@abc-supplies.test", &under(&["ABC Supplies"]), mine),
            "us"
        );
    }

    /// The guard on widening a name to a whole domain. It is what stops one
    /// contact filed under a webmail address from making every stranger there
    /// part of their correspondence.
    #[test]
    fn a_domain_stands_for_a_name_only_when_it_is_recognisably_that_name() {
        assert!(domain_is_the_name("abc-supplies.test", "ABC Supplies"));
        assert!(domain_is_the_name("abc-supplies.test", "abc supplies"));
        assert!(domain_is_the_name("mail.abc-supplies.test", "ABC Supplies"));
        assert!(!domain_is_the_name("gmail.com", "Ilse Vermeer"));
        assert!(!domain_is_the_name("abc-supplies.test", "Delaunay"));
        // A name with no letters at all widens to nothing rather than to
        // everything — the empty needle is inside every string.
        assert!(!domain_is_the_name("abc-supplies.test", "  -  "));
        assert_eq!(squash("ABC Supplies!"), "abcsupplies");
    }

    #[test]
    fn a_lookup_name_is_kept_once_whatever_its_case() {
        let mut names = Vec::new();
        push_once(&mut names, "abc-supplies.test");
        push_once(&mut names, "ABC-Supplies.TEST");
        push_once(&mut names, "other.test");
        assert_eq!(names, ["abc-supplies.test", "other.test"]);
    }

    /// What is attached is named and never opened here.
    #[test]
    fn attachments_are_named_with_their_type_and_size() {
        let part = Attachment {
            index: 0,
            name: "quote.pdf".to_owned(),
            content_type: "application/pdf".to_owned(),
            size: 4_096,
            content_id: None,
            inline: false,
        };
        let named = names_of(&[part]);
        assert_eq!(named.len(), 1);
        assert_eq!(named[0]["name"], json!("quote.pdf"));
        assert_eq!(named[0]["contentType"], json!("application/pdf"));
        assert_eq!(named[0]["size"], json!(4_096));
        assert!(named[0].get("text").is_none(), "bytes are not read here");
        assert!(names_of(&[]).is_empty());
    }
}
