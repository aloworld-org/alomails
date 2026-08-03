//! Recent-correspondent autocomplete for compose (`GET /contacts`). Mines the
//! address headers of the account's recent messages, ranks distinct addresses by
//! how often they appear (ties broken by recency), and returns the top few
//! hundred as `{name, email}` for the client to filter locally as the user
//! types. The account's own addresses are excluded (you don't autocomplete
//! yourself). Tenant/user-scoped through the account door; no address is logged.

use std::collections::{HashMap, HashSet};

use alo_store::{AddressHeaders, vcard};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// The largest `.vcf` upload accepted for import (a generous whole-address-
/// book export; the per-card cap in `vcard::from_vcards` bounds the rest).
const MAX_IMPORT_BYTES: usize = 16 * 1024 * 1024;

/// How many recent messages to mine, and how many contacts to return.
const SCAN_MESSAGES: i64 = 2000;
const MAX_CONTACTS: usize = 200;

/// One aggregated correspondent while ranking.
struct Contact {
    name: Option<String>,
    count: u32,
    first_seen: usize, // index of the most-recent message it appeared in (lower = newer)
}

/// `GET /contacts` → `{"contacts": [{"name": ?, "email": "..."}]}`. Newest,
/// most-frequent correspondents first.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let rows = account
        .acc
        .recent_address_headers(SCAN_MESSAGES)
        .await
        .map_err(|_| Problem::server_error())?;
    let own = own_addresses(&account, &state).await;
    let mut contacts = rank(&rows, &own);
    // Saved address-book contacts are surfaced first (a deliberate save
    // outranks a mined correspondent), then mined ones fill in — deduped
    // by address so a saved contact never appears twice. Best-effort: a
    // read failure just falls back to mined correspondents.
    if let Ok(saved) = account.acc.contacts().await {
        contacts = merge_saved(saved, contacts, &own);
    }
    Ok(Json(json!({ "contacts": contacts })))
}

/// Prepends saved contacts (each of their addresses, name-carried) to the
/// mined list, dropping the account's own addresses and any mined entry a
/// saved contact already covers. Order: saved first (in name order from
/// the store), then the remaining mined correspondents.
fn merge_saved(saved: Vec<alo_store::Contact>, mined: Vec<Value>, own: &[String]) -> Vec<Value> {
    let mut seen: std::collections::HashSet<String> = HashSet::new();
    let mut out: Vec<Value> = Vec::new();
    for contact in &saved {
        for email in &contact.emails {
            let key = email.value.to_lowercase();
            if own.iter().any(|o| o == &key) || !seen.insert(key) {
                continue;
            }
            out.push(json!({ "name": contact.display_name, "email": email.value }));
        }
    }
    for entry in mined {
        let key = entry
            .get("email")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        if seen.insert(key) {
            out.push(entry);
        }
    }
    out.truncate(MAX_CONTACTS);
    out
}

/// `POST /contacts/import` — a `.vcf` body (one or many vCards, e.g. a
/// Gmail/Outlook/Apple export). Each parseable card becomes a saved
/// contact; unparseable or nameless cards are skipped. Returns
/// `{"imported": n, "skipped": m}`. Contacts are created through the
/// account door, so the import is tenant/user-scoped by construction.
pub async fn import(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_IMPORT_BYTES {
        return Err(Problem::too_large());
    }
    let text = String::from_utf8_lossy(&body);
    let parsed = vcard::from_vcards(&text);
    let total_blocks = text.match_indices("BEGIN:VCARD").count();
    let mut imported = 0u32;
    for contact in &parsed {
        match account.acc.create_contact(contact).await {
            Ok(_) => imported += 1,
            Err(error) => {
                tracing::warn!(%error, "contact import: one card failed to store");
            }
        }
    }
    // Skipped = cards we couldn't parse into a contact, plus any that
    // failed to store — reported honestly so the UI can surface it.
    let skipped = total_blocks.saturating_sub(imported as usize);
    Ok(Json(json!({ "imported": imported, "skipped": skipped })))
}

/// `GET /contacts/export` — the account's whole address book as a
/// single `.vcf` (vCard 4.0) attachment, for backup or migration.
pub async fn export(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let account = match authenticate(&state, &headers).await {
        Ok(account) => account,
        Err(problem) => return problem.into_response(),
    };
    let contacts = match account.acc.contacts().await {
        Ok(contacts) => contacts,
        Err(_) => return Problem::server_error().into_response(),
    };
    let body = vcard::to_vcards(&contacts);
    let mut resp = (StatusCode::OK, body).into_response();
    let h = resp.headers_mut();
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/vcard; charset=utf-8"),
    );
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"contacts.vcf\""),
    );
    resp
}

/// The account's own addresses (canonical + aliases), lowercased — excluded from
/// suggestions. Best-effort: a lookup failure just means self may appear, never
/// an error to the user.
async fn own_addresses(account: &Account, state: &AppState) -> Vec<String> {
    let ts = state.store.for_tenant(account.tenant.clone());
    let mut own = Vec::new();
    if let Ok(Some(canonical)) = ts.email_of(&account.user).await {
        own.push(canonical.to_lowercase());
    }
    if let Ok(aliases) = ts.aliases_of(&account.user).await {
        own.extend(aliases.into_iter().map(|a| a.to_lowercase()));
    }
    own
}

/// Aggregate every address across the from/to/cc/bcc headers of the scanned
/// messages and rank them: most frequent first, ties broken by most-recent.
fn rank(rows: &[AddressHeaders], own: &[String]) -> Vec<Value> {
    let mut seen: HashMap<String, Contact> = HashMap::new();
    let mut index = 0usize;
    for row in rows {
        for header in [&row.from, &row.to, &row.cc, &row.bcc] {
            for (name, email) in parse_addresses(header) {
                let key = email.to_lowercase();
                if own.iter().any(|o| o == &key) {
                    continue;
                }
                let entry = seen.entry(key).or_insert_with(|| Contact {
                    name: None,
                    count: 0,
                    first_seen: index,
                });
                entry.count += 1;
                // Rows are newest-first, so the first name we see is the most
                // recent display name for this address — keep it.
                if entry.name.is_none() && name.is_some() {
                    entry.name = name;
                }
            }
        }
        index += 1;
    }

    let mut ranked: Vec<(String, Contact)> = seen.into_iter().collect();
    ranked.sort_by(|(ae, a), (be, b)| {
        b.count
            .cmp(&a.count)
            .then(a.first_seen.cmp(&b.first_seen))
            .then(ae.cmp(be))
    });
    ranked
        .into_iter()
        .take(MAX_CONTACTS)
        .map(|(email, c)| json!({ "name": c.name, "email": email }))
        .collect()
}

/// Extract `(display name, addr-spec)` pairs from one raw address-header string.
/// Naive comma split (matching the rest of the codebase's header handling), then
/// the address inside `<...>` (or the whole token) and any name before it. Only
/// tokens that look like a real address (`@`, no whitespace) are kept.
fn parse_addresses(raw: &str) -> Vec<(Option<String>, String)> {
    if raw.trim().is_empty() {
        return Vec::new();
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
                let name = part[..lt].trim().trim_matches('"').trim().to_owned();
                (if name.is_empty() { None } else { Some(name) }, email)
            }
            _ => (None, part.to_owned()),
        };
        if is_plausible_addr(&email) {
            out.push((name, email));
        }
    }
    out
}

/// A minimal sanity check so header noise (empty parts, malformed tokens) never
/// becomes a suggestion: has an `@` with something either side and no internal
/// whitespace or control characters.
fn is_plausible_addr(addr: &str) -> bool {
    let bytes = addr.as_bytes();
    addr.len() >= 3
        && addr.len() <= 320
        && !bytes.iter().any(|&b| b <= 0x20 || b == 0x7f)
        && matches!(addr.find('@'), Some(at) if at > 0 && at < addr.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::{parse_addresses, rank};
    use alo_store::AddressHeaders;

    fn hdr(from: &str, to: &str) -> AddressHeaders {
        AddressHeaders {
            from: from.to_owned(),
            to: to.to_owned(),
            cc: String::new(),
            bcc: String::new(),
        }
    }

    #[test]
    fn parses_named_and_bare_addresses() {
        let got = parse_addresses("Alice <alice@example.eu>, bob@example.eu");
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            (Some("Alice".to_owned()), "alice@example.eu".to_owned())
        );
        assert_eq!(got[1], (None, "bob@example.eu".to_owned()));
        // Malformed tokens are dropped.
        assert!(parse_addresses("not-an-address, <>, @nope").is_empty());
    }

    #[test]
    fn ranks_by_frequency_then_recency_and_excludes_self() {
        // Newest first: message 0 has carol; messages 1 & 2 have bob.
        let rows = vec![
            hdr("Carol <carol@x.eu>", "me@x.eu"),
            hdr("Bob <bob@x.eu>", "me@x.eu"),
            hdr("bob@x.eu", "me@x.eu"),
        ];
        let out = rank(&rows, &["me@x.eu".to_owned()]);
        // Bob (count 2) outranks Carol (count 1); self is excluded entirely.
        assert_eq!(out[0]["email"], "bob@x.eu");
        assert_eq!(out[0]["name"], "Bob");
        assert_eq!(out[1]["email"], "carol@x.eu");
        assert!(out.iter().all(|c| c["email"] != "me@x.eu"));
    }

    #[test]
    fn keeps_the_most_recent_display_name() {
        // Newest message spells the name "Robert"; an older one says "Bob".
        let rows = vec![hdr("Robert <bob@x.eu>", ""), hdr("Bob <bob@x.eu>", "")];
        let out = rank(&rows, &[]);
        assert_eq!(out[0]["email"], "bob@x.eu");
        assert_eq!(out[0]["name"], "Robert");
    }
}
