//! JMAP JSON representations of store entities (RFC 8621). Keeps the
//! wire shapes — the public contract — in one place.

use alo_store::{Category, Contact, ContactField, Mailbox, Message};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;

/// A JMAP `UTCDate`: `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_date(dt: OffsetDateTime) -> String {
    let dt = dt.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

/// A `Category` (alo extension): a user-defined colored label. `keyword` is
/// the `$category_<id>` a message carries when tagged — clients set, clear, and
/// `hasKeyword`-filter on it directly, so the mapping never has to be guessed.
pub fn category_json(c: &Category) -> Value {
    json!({
        "id": c.id.as_str(),
        "name": c.name,
        "color": c.color,
        "sortOrder": c.sort_order,
        "keyword": alo_store::category_keyword(&c.id),
    })
}

/// A `Contact` (alo address book; RFC 9610-shaped). Multi-valued
/// `emails`/`phones` are arrays of `{kind?, value}`. `firstName`/
/// `lastName` are the `N` components; `name` is the formatted `FN`.
pub fn contact_json(c: &Contact) -> Value {
    json!({
        "id": c.id.as_str(),
        "name": c.display_name,
        "firstName": c.first_name,
        "lastName": c.last_name,
        "emails": c.emails.iter().map(contact_field_json).collect::<Vec<_>>(),
        "phones": c.phones.iter().map(contact_field_json).collect::<Vec<_>>(),
        "organization": c.organization,
        "jobTitle": c.job_title,
        "notes": c.notes,
    })
}

fn contact_field_json(f: &ContactField) -> Value {
    json!({ "kind": f.kind, "value": f.value })
}

/// A JMAP `Mailbox` (RFC 8621 §2). Counters come straight from the
/// store's transactional totals — never recomputed here. Thread counts
/// are approximated by the email counts for now (documented in
/// `docs/interop.md`).
pub fn mailbox_json(m: &Mailbox) -> Value {
    json!({
        "id": m.id.as_str(),
        "name": m.name,
        "parentId": m.parent_id.as_ref().map(|p| p.as_str()),
        "role": m.role,
        "color": m.color,
        "sortOrder": 0,
        "totalEmails": m.total_messages,
        "unreadEmails": m.unread_messages,
        "totalThreads": m.total_messages,
        "unreadThreads": m.unread_messages,
        "myRights": {
            "mayReadItems": true, "mayAddItems": true, "mayRemoveItems": true,
            "maySetSeen": true, "maySetKeywords": true, "mayCreateChild": true,
            "mayRename": true, "mayDelete": true, "maySubmit": true
        },
        "isSubscribed": true
    })
}

/// One JMAP `EmailAddress` parsed from a raw header value (best effort):
/// the address inside `<...>` (or the whole string), plus any display
/// name before it.
fn address_list(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return json!([]);
    }
    let mut out = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, email) = match (part.find('<'), part.find('>')) {
            (Some(lt), Some(gt)) if gt > lt => {
                let email = part[lt + 1..gt].trim().to_owned();
                let name = part[..lt].trim().trim_matches('"').to_owned();
                (if name.is_empty() { None } else { Some(name) }, email)
            }
            _ => (None, part.to_owned()),
        };
        out.push(json!({ "name": name, "email": email }));
    }
    json!(out)
}

/// One attachment on a JMAP `Email` (RFC 8621 §4.1.4). `blob_id` is the
/// composite id the download route resolves back to the part.
pub struct AttachmentJson {
    pub blob_id: String,
    pub content_type: String,
    pub name: String,
    pub size: usize,
    /// `Content-ID` (no angle brackets) — an HTML `cid:` reference resolves to
    /// the part with this id, so the client can render inline images.
    pub content_id: Option<String>,
    /// `Content-Disposition: inline` (embedded), else a downloadable attachment.
    pub inline: bool,
}

/// The parsed reading view passed to [`email_json`] when the client asked for
/// the body: the decoded plain-text (with its truncation flag) and/or HTML
/// body, and the attachment list.
pub struct ReadBody {
    pub text: Option<(String, bool)>,
    pub html: Option<String>,
    pub attachments: Vec<AttachmentJson>,
    /// Parsed List-Unsubscribe options, surfaced as `alo:listUnsubscribe`.
    pub unsubscribe: Option<crate::mime_read::Unsubscribe>,
    /// An inbound calendar invitation, surfaced as `alo:invitation`.
    pub invitation: Option<Invitation>,
}

/// An inbound calendar invitation (an iMIP `METHOD:REQUEST` in the message's
/// `text/calendar` part), summarised for the reading pane so it can show an
/// Accept/Decline card without parsing iCalendar in the client. Times are
/// RFC 3339 (UTC). The RSVP endpoint re-reads the raw part from the message, so
/// this is display-only.
pub struct Invitation {
    /// The scheduling method: `REQUEST` (an invitation), `CANCEL` (the organizer
    /// withdrew it), or `REPLY` (a guest responded). Drives whether the reading
    /// pane shows an Accept/Decline card, a cancellation notice, or a reply.
    pub method: String,
    pub uid: String,
    pub summary: String,
    pub organizer: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
    pub all_day: bool,
    pub location: Option<String>,
    /// For a `REPLY`: the replying guest's email; `None` otherwise.
    pub attendee: Option<String>,
    /// For a `REPLY`: their status (`accepted`/`declined`/`tentative`); `None`
    /// otherwise.
    pub partstat: Option<String>,
}

/// Derive a short preview from the text body, else a crude tag-stripped HTML
/// snippet, else empty.
pub fn preview_of(body: &ReadBody) -> String {
    if let Some((text, _)) = &body.text {
        return text.chars().take(256).collect();
    }
    if let Some(html) = &body.html {
        let mut out = String::new();
        let mut in_tag = false;
        for ch in html.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
            if out.chars().count() >= 256 {
                break;
            }
        }
        return out.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    String::new()
}

/// Builds a full JMAP `Email` object from the stored metadata plus the
/// resolved mailbox ids, keywords, and (optional) parsed body.
///
/// `body` is `None` for header-only fetches (the mailbox list); when present
/// it drives `textBody`/`htmlBody`/`bodyValues`, the attachment list, the
/// preview, and `hasAttachment`.
pub fn email_json(
    m: &Message,
    mailbox_ids: &[String],
    keywords: &[String],
    body: Option<&ReadBody>,
    flag_due: Option<OffsetDateTime>,
) -> Value {
    let mut mailboxes = Map::new();
    for id in mailbox_ids {
        mailboxes.insert(id.clone(), json!(true));
    }
    let mut kw = Map::new();
    for k in keywords {
        kw.insert(k.clone(), json!(true));
    }

    let preview = body.map(preview_of).unwrap_or_default();
    // When the body is loaded (full read) attachments are authoritative; the
    // header-only list path falls back to the stored flag (message.rs).
    let has_attachment = body
        .map(|b| !b.attachments.is_empty())
        .unwrap_or_else(|| m.has_attachment.unwrap_or(false));

    // textBody/htmlBody reference body-value parts by a stable partId; the
    // attachment parts are listed separately with their download blob ids.
    let mut text_body = Vec::new();
    let mut html_body = Vec::new();
    let mut body_values = Map::new();
    if let Some(b) = body {
        if let Some((value, truncated)) = &b.text {
            text_body.push(json!({ "partId": "text", "type": "text/plain" }));
            body_values.insert(
                "text".to_owned(),
                json!({ "value": value, "isEncodingProblem": false, "isTruncated": truncated }),
            );
        }
        if let Some(html) = &b.html {
            html_body.push(json!({ "partId": "html", "type": "text/html" }));
            body_values.insert(
                "html".to_owned(),
                json!({ "value": html, "isEncodingProblem": false, "isTruncated": false }),
            );
        }
    }
    let attachments: Vec<Value> = body
        .map(|b| {
            b.attachments
                .iter()
                .map(|a| {
                    json!({
                        "blobId": a.blob_id,
                        "type": a.content_type,
                        "name": a.name,
                        "size": a.size,
                        "cid": a.content_id,
                        "disposition": if a.inline { "inline" } else { "attachment" }
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut email = json!({
        "id": m.id.as_str(),
        "blobId": m.blob_id.as_str(),
        "threadId": m.thread_id.as_str(),
        "mailboxIds": Value::Object(mailboxes),
        "keywords": Value::Object(kw),
        "size": m.size,
        "receivedAt": utc_date(m.received_at),
        "sentAt": m.sent_at.map(utc_date),
        "subject": m.subject,
        "from": address_list(&m.from_addr),
        "to": address_list(&m.to_addrs),
        "cc": address_list(&m.cc_addrs),
        // Bcc is populated only on the sender's own copy; a received copy has it
        // empty, so this never discloses another recipient's blind copies.
        "bcc": address_list(&m.bcc_addrs),
        "preview": preview,
        "hasAttachment": has_attachment,
        "messageId": m.message_id_hdr.as_ref().map(|v| vec![v.clone()]),
        "textBody": text_body,
        "htmlBody": html_body,
        "attachments": attachments,
        // alo exposes the parsed auth verdict as a non-standard
        // property so clients can render a trust banner without a header
        // fetch (additive, `alo:` namespaced).
        "alo:authentication": {
            "spf": m.auth_spf, "dkim": m.auth_dkim, "dmarc": m.auth_dmarc
        },
        // A flagged message's follow-up due-date (alo extension, additive).
        // Null/absent when there is no due-date set.
        "alo:flagDue": flag_due.map(utc_date)
    });

    if body.is_some() {
        email["bodyValues"] = Value::Object(body_values);
    }
    // alo exposes the parsed unsubscribe options (RFC 2369 / RFC 8058) on the
    // full email so the reading pane can offer an Unsubscribe action without a
    // header fetch. Present only when the message actually carries one.
    if let Some(u) = body.and_then(|b| b.unsubscribe.as_ref()) {
        email["alo:listUnsubscribe"] = json!({
            "http": u.http,
            "mailto": u.mailto,
            "oneClick": u.one_click,
        });
    }
    // An inbound invitation, so the reading pane can offer Accept/Decline. RSVP
    // acts on the message id, re-reading the raw part — this is display-only.
    if let Some(inv) = body.and_then(|b| b.invitation.as_ref()) {
        email["alo:invitation"] = json!({
            "method": inv.method,
            "uid": inv.uid,
            "summary": inv.summary,
            "organizer": inv.organizer,
            "startsAt": inv.starts_at,
            "endsAt": inv.ends_at,
            "allDay": inv.all_day,
            "location": inv.location,
            // For a REPLY: who responded and how, so the reply card can say
            // "Ann accepted" instead of a nameless "someone responded".
            "attendee": inv.attendee,
            "partstat": inv.partstat,
        });
    }
    email
}

/// A JMAP `Thread` object (§3): id + ordered email ids.
pub fn thread_json(thread_id: &str, email_ids: &[String]) -> Value {
    json!({ "id": thread_id, "emailIds": email_ids })
}
