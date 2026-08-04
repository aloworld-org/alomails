//! WebDAV sync for the account: **CardDAV** (RFC 6352, contacts) **and CalDAV**
//! (RFC 4791, calendar) — the protocols phones and desktops (Apple Contacts /
//! Calendar, Thunderbird, DAVx5) sync against natively. One handler serves both
//! under `/dav`; contacts live at `addressbooks/<user>/default/<id>.vcf` and
//! events at `calendars/<user>/default/<id>.ics`, and the principal advertises
//! both home-sets so a client discovers whichever it asks for.
//!
//! CardDAV: address-object resources are `<contactId>.vcf`. Discovery is the
//! standard WebDAV dance (`.well-known/carddav` → principal →
//! addressbook-home-set → addressbook). Incremental sync uses the
//! account modseq as the RFC 6578 sync-token — every contact write
//! already bumps it — so `sync-collection` maps straight onto
//! `AccountStore::changes`. Per-object ETags are a content hash, so a
//! no-op PUT does not churn the client.
//!
//! Auth is HTTP Basic (what these clients speak), verified through the
//! same `authenticate_legacy` path as IMAP/POP3/SMTP-AUTH — a
//! 2FA-enabled account is refused there and must use an app-specific
//! flow later.
//!
//! Scope (recorded in `docs/interop.md`): `PROPFIND`, `REPORT`
//! (`addressbook-multiget`, `sync-collection`), `GET`, `PUT`, `DELETE`,
//! `OPTIONS`. `addressbook-query` filters are not evaluated — such a
//! REPORT returns the whole collection (a valid, unfiltered result the
//! client then narrows); clients sync fine via multiget + sync-collection.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use alo_store::{AccountStore, CalendarEvent, Contact, ContactId, EventId, ical, vcard};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;

use crate::state::AppState;

const NS: &str = "xmlns:d=\"DAV:\" xmlns:card=\"urn:ietf:params:xml:ns:carddav\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" xmlns:cs=\"http://calendarserver.org/ns/\"";
const SYNC_PREFIX: &str = "urn:alo:contacts:";
const CAL_SYNC_PREFIX: &str = "urn:alo:calendar:";
const MAX_SYNC: i64 = 5000;

/// `GET /.well-known/carddav` (and PROPFIND on it): send clients to the
/// DAV entry point, from which they discover the principal.
pub async fn well_known() -> Response {
    (
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, "/dav/")],
        "",
    )
        .into_response()
}

/// The single CardDAV entry point: every `/dav` and `/dav/*` request
/// (any WebDAV method) routes here and is dispatched by method + path.
pub async fn handle(State(state): State<AppState>, req: axum::extract::Request) -> Response {
    // HTTP Basic auth → (tenant, user). Anything else is a challenge.
    let principal = match basic_auth(&state, req.headers()).await {
        Some(p) => p,
        None => return challenge(),
    };
    let acc = state
        .store
        .for_account(principal.tenant.clone(), principal.user.clone());
    let uid = principal.user.as_str().to_owned();

    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let depth = depth_header(req.headers());
    let headers = req.headers().clone();
    let body = match axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return status(StatusCode::PAYLOAD_TOO_LARGE),
    };

    // Everything under /dav; the segment after `addressbooks/<uid>/` is the
    // collection ("default"), and one more segment is a `<id>.vcf` object.
    let rel = path
        .strip_prefix("/dav")
        .unwrap_or("")
        .trim_matches('/')
        .to_owned();
    let resource = classify(&rel, &uid);

    match method.as_str() {
        "OPTIONS" => options(),
        "PROPFIND" => propfind(&acc, &uid, &resource, depth, &body).await,
        "REPORT" => report(&acc, &uid, &resource, &body).await,
        "GET" | "HEAD" => get_object(&acc, &resource, method == Method::HEAD).await,
        "PUT" => put_object(&acc, &resource, &headers, &body).await,
        "DELETE" => delete_object(&acc, &resource, &headers).await,
        _ => status(StatusCode::METHOD_NOT_ALLOWED),
    }
}

/// What DAV resource a `/dav`-relative path names.
#[derive(Debug, PartialEq, Eq)]
enum Resource {
    /// The DAV root or a principal collection.
    Principal,
    /// The addressbook-home collection.
    Home,
    /// The one addressbook collection.
    Addressbook,
    /// One address object (a contact), by id.
    Object(String),
    /// The calendar-home collection.
    CalHome,
    /// The one calendar collection.
    Calendar,
    /// One calendar object (an event), by id.
    CalObject(String),
    /// A path that does not belong to this user.
    NotFound,
}

fn classify(rel: &str, uid: &str) -> Resource {
    let segs: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [] | ["principals"] => Resource::Principal,
        ["principals", u] if *u == uid => Resource::Principal,
        ["addressbooks", u] if *u == uid => Resource::Home,
        ["addressbooks", u, "default"] if *u == uid => Resource::Addressbook,
        ["addressbooks", u, "default", obj] if *u == uid => {
            Resource::Object(obj.trim_end_matches(".vcf").to_owned())
        }
        ["calendars", u] if *u == uid => Resource::CalHome,
        ["calendars", u, "default"] if *u == uid => Resource::Calendar,
        ["calendars", u, "default", obj] if *u == uid => {
            Resource::CalObject(obj.trim_end_matches(".ics").to_owned())
        }
        _ => Resource::NotFound,
    }
}

// ---- OPTIONS ----------------------------------------------------------

fn options() -> Response {
    let mut resp = status(StatusCode::OK);
    let h = resp.headers_mut();
    h.insert(
        "DAV",
        header::HeaderValue::from_static("1, 3, addressbook, calendar-access"),
    );
    h.insert(
        header::ALLOW,
        header::HeaderValue::from_static("OPTIONS, GET, HEAD, PUT, DELETE, PROPFIND, REPORT"),
    );
    resp
}

// ---- PROPFIND ---------------------------------------------------------

async fn propfind(
    acc: &AccountStore,
    uid: &str,
    resource: &Resource,
    depth: u8,
    _body: &[u8],
) -> Response {
    let mut responses = String::new();
    match resource {
        Resource::Principal => {
            responses.push_str(&principal_response(uid));
        }
        Resource::Home => {
            responses.push_str(&home_response(uid));
            if depth >= 1 {
                responses.push_str(&addressbook_response(acc, uid).await);
            }
        }
        Resource::Addressbook => {
            responses.push_str(&addressbook_response(acc, uid).await);
            if depth >= 1 {
                let contacts = match acc.contacts().await {
                    Ok(c) => c,
                    Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
                };
                for c in &contacts {
                    responses.push_str(&object_propstat(uid, c, false));
                }
            }
        }
        Resource::Object(id) => match fetch(acc, id).await {
            Some(c) => responses.push_str(&object_propstat(uid, &c, false)),
            None => return status(StatusCode::NOT_FOUND),
        },
        Resource::CalHome => {
            responses.push_str(&cal_home_response(uid));
            if depth >= 1 {
                responses.push_str(&calendar_response(acc, uid).await);
            }
        }
        Resource::Calendar => {
            responses.push_str(&calendar_response(acc, uid).await);
            if depth >= 1 {
                let events = match acc.all_events().await {
                    Ok(e) => e,
                    Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
                };
                for e in &events {
                    responses.push_str(&event_propstat(uid, e, false));
                }
            }
        }
        Resource::CalObject(id) => match fetch_event(acc, id).await {
            Some(e) => responses.push_str(&event_propstat(uid, &e, false)),
            None => return status(StatusCode::NOT_FOUND),
        },
        Resource::NotFound => return status(StatusCode::NOT_FOUND),
    }
    multistatus(&responses)
}

fn principal_response(uid: &str) -> String {
    let href = format!("/dav/principals/{uid}/");
    let props = format!(
        "<d:resourcetype><d:principal/><d:collection/></d:resourcetype>\
         <d:displayname>{uid}</d:displayname>\
         <d:current-user-principal><d:href>{href}</d:href></d:current-user-principal>\
         <card:addressbook-home-set><d:href>/dav/addressbooks/{uid}/</d:href></card:addressbook-home-set>\
         <cal:calendar-home-set><d:href>/dav/calendars/{uid}/</d:href></cal:calendar-home-set>",
        uid = esc(uid)
    );
    response(&href, &props)
}

fn home_response(uid: &str) -> String {
    let href = format!("/dav/addressbooks/{uid}/");
    let props = "<d:resourcetype><d:collection/></d:resourcetype>\
                 <d:displayname>Address books</d:displayname>";
    response(&href, props)
}

async fn addressbook_response(acc: &AccountStore, uid: &str) -> String {
    let href = format!("/dav/addressbooks/{uid}/default/");
    let ctag = collection_tag(acc).await;
    let props = format!(
        "<d:resourcetype><d:collection/><card:addressbook/></d:resourcetype>\
         <d:displayname>Contacts</d:displayname>\
         <cs:getctag>{ctag}</cs:getctag>\
         <d:sync-token>{ctag}</d:sync-token>\
         <card:supported-address-data>\
           <card:address-data-type content-type=\"text/vcard\" version=\"4.0\"/>\
         </card:supported-address-data>\
         <d:supported-report-set>\
           <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>\
           <d:supported-report><d:report><card:addressbook-multiget/></d:report></d:supported-report>\
         </d:supported-report-set>",
    );
    response(&href, &props)
}

/// A `<response>` for one contact object: its href + getetag, and (when
/// `with_data`) the vCard as `address-data`.
fn object_propstat(uid: &str, c: &Contact, with_data: bool) -> String {
    let href = object_href(uid, &c.id);
    let etag = etag(c);
    let mut props = format!(
        "<d:getetag>{etag}</d:getetag>\
         <d:getcontenttype>text/vcard; charset=utf-8</d:getcontenttype>"
    );
    if with_data {
        props.push_str(&format!(
            "<card:address-data>{}</card:address-data>",
            esc(&vcard::to_vcard(c))
        ));
    }
    response(&href, &props)
}

fn cal_home_response(uid: &str) -> String {
    let href = format!("/dav/calendars/{uid}/");
    let props = "<d:resourcetype><d:collection/></d:resourcetype>\
                 <d:displayname>Calendars</d:displayname>";
    response(&href, props)
}

async fn calendar_response(acc: &AccountStore, uid: &str) -> String {
    let href = format!("/dav/calendars/{uid}/default/");
    let ctag = cal_collection_tag(acc).await;
    let props = format!(
        "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
         <d:displayname>Calendar</d:displayname>\
         <cs:getctag>{ctag}</cs:getctag>\
         <d:sync-token>{ctag}</d:sync-token>\
         <cal:supported-calendar-component-set><cal:comp name=\"VEVENT\"/></cal:supported-calendar-component-set>\
         <cs:calendar-color>#e76f51ff</cs:calendar-color>\
         <d:supported-report-set>\
           <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:calendar-multiget/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:calendar-query/></d:report></d:supported-report>\
         </d:supported-report-set>",
    );
    response(&href, &props)
}

/// A `<response>` for one event object: its href + getetag, and (when
/// `with_data`) the iCalendar as `calendar-data`.
fn event_propstat(uid: &str, e: &CalendarEvent, with_data: bool) -> String {
    let href = event_href(uid, &e.id);
    let etag = event_etag(e);
    let mut props = format!(
        "<d:getetag>{etag}</d:getetag>\
         <d:getcontenttype>text/calendar; charset=utf-8; component=VEVENT</d:getcontenttype>"
    );
    if with_data {
        props.push_str(&format!(
            "<cal:calendar-data>{}</cal:calendar-data>",
            esc(&ical::to_ics(e))
        ));
    }
    response(&href, &props)
}

// ---- REPORT (multiget + sync-collection) ------------------------------

async fn report(acc: &AccountStore, uid: &str, resource: &Resource, body: &[u8]) -> Response {
    let text = String::from_utf8_lossy(body);
    match resource {
        Resource::Addressbook => report_contacts(acc, uid, &text).await,
        Resource::Calendar => report_events(acc, uid, &text).await,
        _ => status(StatusCode::NOT_FOUND),
    }
}

async fn report_contacts(acc: &AccountStore, uid: &str, text: &str) -> Response {
    if text.contains("sync-collection") {
        return sync_collection(acc, uid, text).await;
    }
    // addressbook-multiget, or an addressbook-query we answer unfiltered.
    let hrefs = extract_hrefs(text);
    let mut responses = String::new();
    if hrefs.is_empty() {
        // No explicit hrefs (an addressbook-query): return the whole book.
        let contacts = match acc.contacts().await {
            Ok(c) => c,
            Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
        };
        for c in &contacts {
            responses.push_str(&object_propstat(uid, c, true));
        }
    } else {
        for href in hrefs {
            match href_object_id(&href) {
                Some(id) => match fetch(acc, &id).await {
                    Some(c) => responses.push_str(&object_propstat(uid, &c, true)),
                    None => responses.push_str(&not_found_response(&href)),
                },
                None => responses.push_str(&not_found_response(&href)),
            }
        }
    }
    multistatus(&responses)
}

async fn report_events(acc: &AccountStore, uid: &str, text: &str) -> Response {
    if text.contains("sync-collection") {
        return cal_sync_collection(acc, uid, text).await;
    }
    // calendar-multiget, or a calendar-query we answer unfiltered.
    let hrefs = extract_hrefs(text);
    let mut responses = String::new();
    if hrefs.is_empty() {
        let events = match acc.all_events().await {
            Ok(e) => e,
            Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
        };
        for e in &events {
            responses.push_str(&event_propstat(uid, e, true));
        }
    } else {
        for href in hrefs {
            match cal_href_object_id(&href) {
                Some(id) => match fetch_event(acc, &id).await {
                    Some(e) => responses.push_str(&event_propstat(uid, &e, true)),
                    None => responses.push_str(&not_found_response(&href)),
                },
                None => responses.push_str(&not_found_response(&href)),
            }
        }
    }
    multistatus(&responses)
}

async fn cal_sync_collection(acc: &AccountStore, uid: &str, body: &str) -> Response {
    let since = extract_sync_token(body)
        .and_then(|t| t.strip_prefix(CAL_SYNC_PREFIX).map(str::to_owned))
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0);
    let changes = match acc.changes("Event", since, MAX_SYNC).await {
        Ok(c) => c,
        Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let mut responses = String::new();
    for id in changes.created.iter().chain(changes.updated.iter()) {
        if let Some(e) = fetch_event(acc, id).await {
            responses.push_str(&event_propstat(uid, &e, false));
        }
    }
    for id in &changes.destroyed {
        responses.push_str(&not_found_response(&event_href(
            uid,
            &EventId::new(id.clone()),
        )));
    }
    let token = format!("{CAL_SYNC_PREFIX}{}", changes.new_state);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <d:multistatus {NS}>{responses}<d:sync-token>{token}</d:sync-token></d:multistatus>"
    );
    xml_response(StatusCode::MULTI_STATUS, xml)
}

async fn sync_collection(acc: &AccountStore, uid: &str, body: &str) -> Response {
    let since = extract_sync_token(body)
        .and_then(|t| t.strip_prefix(SYNC_PREFIX).map(str::to_owned))
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0);
    let changes = match acc.changes("Contact", since, MAX_SYNC).await {
        Ok(c) => c,
        Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let mut responses = String::new();
    // Created/updated → 200 with the current etag; destroyed → 404.
    for id in changes.created.iter().chain(changes.updated.iter()) {
        // A contact that vanished between the change log and now is skipped.
        if let Some(c) = fetch(acc, id).await {
            responses.push_str(&object_propstat(uid, &c, false));
        }
    }
    for id in &changes.destroyed {
        responses.push_str(&not_found_response(&object_href(
            uid,
            &ContactId::new(id.clone()),
        )));
    }
    let token = format!("{SYNC_PREFIX}{}", changes.new_state);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <d:multistatus {NS}>{responses}<d:sync-token>{token}</d:sync-token></d:multistatus>"
    );
    xml_response(StatusCode::MULTI_STATUS, xml)
}

// ---- GET / PUT / DELETE ----------------------------------------------

async fn get_object(acc: &AccountStore, resource: &Resource, head: bool) -> Response {
    match resource {
        Resource::Object(id) => match fetch(acc, id).await {
            Some(c) => serve(
                head,
                vcard::to_vcard(&c),
                &etag(&c),
                "text/vcard; charset=utf-8",
            ),
            None => status(StatusCode::NOT_FOUND),
        },
        Resource::CalObject(id) => match fetch_event(acc, id).await {
            Some(e) => serve(
                head,
                ical::to_ics(&e),
                &event_etag(&e),
                "text/calendar; charset=utf-8",
            ),
            None => status(StatusCode::NOT_FOUND),
        },
        _ => status(StatusCode::NOT_FOUND),
    }
}

/// Serve a DAV object body (headers only for HEAD) with content-type + ETag.
fn serve(head: bool, body: String, etag: &str, content_type: &'static str) -> Response {
    let mut resp = if head {
        status(StatusCode::OK)
    } else {
        (StatusCode::OK, body).into_response()
    };
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type),
    );
    if let Ok(v) = header::HeaderValue::from_str(etag) {
        h.insert(header::ETAG, v);
    }
    resp
}

async fn put_object(
    acc: &AccountStore,
    resource: &Resource,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    match resource {
        Resource::Object(id) => put_contact_object(acc, id, headers, body).await,
        Resource::CalObject(id) => put_event_object(acc, id, headers, body).await,
        _ => status(StatusCode::NOT_FOUND),
    }
}

async fn put_contact_object(
    acc: &AccountStore,
    id: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let existing = fetch(acc, id).await;
    // Preconditions (RFC 7232): If-None-Match: * = create-only;
    // If-Match: <etag> = update-only-if-current.
    if header_has(headers, header::IF_NONE_MATCH, "*") && existing.is_some() {
        return status(StatusCode::PRECONDITION_FAILED);
    }
    if let Some(want) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()) {
        let current = existing.as_ref().map(etag);
        if current.as_deref() != Some(want.trim()) {
            return status(StatusCode::PRECONDITION_FAILED);
        }
    }
    let Some(mut contact) = vcard::from_vcard(&String::from_utf8_lossy(body)) else {
        return status(StatusCode::BAD_REQUEST);
    };
    // The href is authoritative in CardDAV: store under the path id,
    // whatever UID the card carries.
    contact.id = ContactId::new(id.to_owned());
    match acc
        .put_contact(&ContactId::new(id.to_owned()), &contact)
        .await
    {
        Ok(created) => created_or_updated(created, &etag(&contact)),
        // The href is taken by another account (global-id collision).
        Err(alo_store::StoreError::Conflict(_)) => status(StatusCode::CONFLICT),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn put_event_object(
    acc: &AccountStore,
    id: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let existing = fetch_event(acc, id).await;
    if header_has(headers, header::IF_NONE_MATCH, "*") && existing.is_some() {
        return status(StatusCode::PRECONDITION_FAILED);
    }
    if let Some(want) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()) {
        let current = existing.as_ref().map(event_etag);
        if current.as_deref() != Some(want.trim()) {
            return status(StatusCode::PRECONDITION_FAILED);
        }
    }
    let Some(mut event) = ical::from_ics(&String::from_utf8_lossy(body), id) else {
        return status(StatusCode::BAD_REQUEST);
    };
    // The href is authoritative: store under the path id (= iCalendar UID).
    event.id = EventId::new(id.to_owned());
    match acc.put_event(&EventId::new(id.to_owned()), &event).await {
        Ok(created) => created_or_updated(created, &event_etag(&event)),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// A PUT result: 201 (created) or 204 (updated), carrying the new ETag.
fn created_or_updated(created: bool, etag: &str) -> Response {
    let code = if created {
        StatusCode::CREATED
    } else {
        StatusCode::NO_CONTENT
    };
    let mut resp = status(code);
    if let Ok(v) = header::HeaderValue::from_str(etag) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    resp
}

async fn delete_object(acc: &AccountStore, resource: &Resource, headers: &HeaderMap) -> Response {
    let want = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok());
    match resource {
        Resource::Object(id) => {
            if let Some(want) = want {
                match fetch(acc, id).await {
                    Some(c) if etag(&c) == want.trim() => {}
                    Some(_) => return status(StatusCode::PRECONDITION_FAILED),
                    None => return status(StatusCode::NOT_FOUND),
                }
            }
            store_delete(acc.delete_contact(&ContactId::new(id.to_owned())).await)
        }
        Resource::CalObject(id) => {
            if let Some(want) = want {
                match fetch_event(acc, id).await {
                    Some(e) if event_etag(&e) == want.trim() => {}
                    Some(_) => return status(StatusCode::PRECONDITION_FAILED),
                    None => return status(StatusCode::NOT_FOUND),
                }
            }
            store_delete(acc.delete_event(&EventId::new(id.to_owned())).await)
        }
        _ => status(StatusCode::NOT_FOUND),
    }
}

fn store_delete(result: Result<(), alo_store::StoreError>) -> Response {
    match result {
        Ok(()) => status(StatusCode::NO_CONTENT),
        Err(alo_store::StoreError::NotFound) => status(StatusCode::NOT_FOUND),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ---- helpers ----------------------------------------------------------

async fn basic_auth(state: &AppState, headers: &HeaderMap) -> Option<alo_identity::Principal> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let b64 = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    let creds = String::from_utf8(decoded).ok()?;
    let (user, pass) = creds.split_once(':')?;
    state
        .identity
        .authenticate_legacy(user, pass)
        .await
        .ok()
        .flatten()
}

fn challenge() -> Response {
    let mut resp = status(StatusCode::UNAUTHORIZED);
    resp.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Basic realm=\"alo CardDAV\""),
    );
    resp
}

async fn fetch(acc: &AccountStore, id: &str) -> Option<Contact> {
    acc.contact(&ContactId::new(id.to_owned()))
        .await
        .ok()
        .flatten()
}

/// The collection's change tag = the account modseq (bumped on every
/// contact write). Doubles as the getctag and the sync-token base.
async fn collection_tag(acc: &AccountStore) -> String {
    let modseq = acc.state().await.unwrap_or_else(|_| "0".to_owned());
    format!("{SYNC_PREFIX}{modseq}")
}

/// A strong per-object ETag from a content hash of the serialized vCard,
/// so a no-op PUT does not change it.
fn etag(c: &Contact) -> String {
    let mut hasher = DefaultHasher::new();
    vcard::to_vcard(c).hash(&mut hasher);
    format!("\"{:016x}\"", hasher.finish())
}

fn object_href(uid: &str, id: &ContactId) -> String {
    format!("/dav/addressbooks/{uid}/default/{}.vcf", id.as_str())
}

fn href_object_id(href: &str) -> Option<String> {
    href.rsplit('/')
        .next()
        .map(|last| last.trim_end_matches(".vcf").to_owned())
        .filter(|s| !s.is_empty())
}

async fn fetch_event(acc: &AccountStore, id: &str) -> Option<CalendarEvent> {
    acc.event(&EventId::new(id.to_owned())).await.ok().flatten()
}

/// The calendar collection's change tag = the account modseq (bumped on every
/// event write). Doubles as the getctag and the sync-token base.
async fn cal_collection_tag(acc: &AccountStore) -> String {
    let modseq = acc.state().await.unwrap_or_else(|_| "0".to_owned());
    format!("{CAL_SYNC_PREFIX}{modseq}")
}

/// A per-event ETag from a hash of the event's fields — deliberately NOT the
/// serialized iCalendar, whose `DTSTAMP` changes each render and would churn.
fn event_etag(e: &CalendarEvent) -> String {
    let mut hasher = DefaultHasher::new();
    e.summary.hash(&mut hasher);
    e.description.hash(&mut hasher);
    e.location.hash(&mut hasher);
    e.starts_at.unix_timestamp().hash(&mut hasher);
    e.ends_at.unix_timestamp().hash(&mut hasher);
    e.all_day.hash(&mut hasher);
    e.recurrence.hash(&mut hasher);
    e.attendees.hash(&mut hasher);
    // Excluding an occurrence changes the event; the ETag must move so CalDAV
    // clients re-sync it.
    for ex in &e.exdates {
        ex.unix_timestamp().hash(&mut hasher);
    }
    format!("\"{:016x}\"", hasher.finish())
}

fn event_href(uid: &str, id: &EventId) -> String {
    format!("/dav/calendars/{uid}/default/{}.ics", id.as_str())
}

fn cal_href_object_id(href: &str) -> Option<String> {
    href.rsplit('/')
        .next()
        .map(|last| last.trim_end_matches(".ics").to_owned())
        .filter(|s| !s.is_empty())
}

/// Every `<href>…</href>` in a request body, namespace-prefix-agnostic.
fn extract_hrefs(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("href>") {
        let after = &rest[start + "href>".len()..];
        if let Some(end) = after.find("</") {
            let value = unescape(after[..end].trim());
            if !value.is_empty() {
                out.push(value);
            }
            rest = &after[end..];
        } else {
            break;
        }
    }
    out
}

/// The `<sync-token>` value from a sync-collection body, if present.
fn extract_sync_token(body: &str) -> Option<String> {
    let start = body.find("sync-token>")? + "sync-token>".len();
    let after = &body[start..];
    let end = after.find("</")?;
    let value = unescape(after[..end].trim());
    (!value.is_empty()).then_some(value)
}

fn depth_header(headers: &HeaderMap) -> u8 {
    match headers.get("depth").and_then(|v| v.to_str().ok()) {
        Some("0") => 0,
        Some("infinity") => 1, // we never nest beyond one level
        Some(_) => 1,
        None => 0,
    }
}

fn header_has(headers: &HeaderMap, name: header::HeaderName, want: &str) -> bool {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim() == want)
}

fn response(href: &str, props: &str) -> String {
    format!(
        "<d:response><d:href>{}</d:href><d:propstat><d:prop>{props}</d:prop>\
         <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
        esc(href)
    )
}

fn not_found_response(href: &str) -> String {
    format!(
        "<d:response><d:href>{}</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>",
        esc(href)
    )
}

fn multistatus(responses: &str) -> Response {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus {NS}>{responses}</d:multistatus>"
    );
    xml_response(StatusCode::MULTI_STATUS, xml)
}

fn xml_response(code: StatusCode, xml: String) -> Response {
    let mut resp = (code, xml).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    resp
}

fn status(code: StatusCode) -> Response {
    (code, Body::empty()).into_response()
}

fn esc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            c => vec![c],
        })
        .collect()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_classification() {
        assert_eq!(classify("", "u1"), Resource::Principal);
        assert_eq!(classify("principals/u1", "u1"), Resource::Principal);
        assert_eq!(classify("addressbooks/u1", "u1"), Resource::Home);
        assert_eq!(
            classify("addressbooks/u1/default", "u1"),
            Resource::Addressbook
        );
        assert_eq!(
            classify("addressbooks/u1/default/abc.vcf", "u1"),
            Resource::Object("abc".to_owned())
        );
        // Another user's path is refused.
        assert_eq!(
            classify("addressbooks/u2/default", "u1"),
            Resource::NotFound
        );
    }

    #[test]
    fn hrefs_are_extracted_prefix_agnostically() {
        let body = "<c:addressbook-multiget xmlns:d=\"DAV:\">\
            <d:href>/dav/addressbooks/u/default/a.vcf</d:href>\
            <d:href>/dav/addressbooks/u/default/b.vcf</d:href></c:addressbook-multiget>";
        let hrefs = extract_hrefs(body);
        assert_eq!(hrefs.len(), 2);
        assert_eq!(href_object_id(&hrefs[0]).as_deref(), Some("a"));
        assert_eq!(href_object_id(&hrefs[1]).as_deref(), Some("b"));
    }

    #[test]
    fn sync_token_round_trips() {
        let body = "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token>urn:alo:contacts:42</d:sync-token></d:sync-collection>";
        assert_eq!(
            extract_sync_token(body).as_deref(),
            Some("urn:alo:contacts:42")
        );
        // The empty/initial case.
        let init = "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token/></d:sync-collection>";
        assert_eq!(extract_sync_token(init), None);
    }

    #[test]
    fn etag_changes_with_content_only() {
        let mut c = Contact {
            id: ContactId::new("x"),
            display_name: "A".to_owned(),
            first_name: None,
            last_name: None,
            emails: vec![],
            phones: vec![],
            organization: None,
            job_title: None,
            notes: None,
        };
        let e1 = etag(&c);
        assert_eq!(e1, etag(&c), "stable for identical content");
        c.display_name = "B".to_owned();
        assert_ne!(e1, etag(&c), "changes when content changes");
    }
}
