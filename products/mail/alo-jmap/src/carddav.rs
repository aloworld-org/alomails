//! WebDAV sync for the account: **CardDAV** (RFC 6352, contacts) **and CalDAV**
//! (RFC 4791, calendar) — the protocols phones and desktops (Apple Contacts /
//! Calendar, Thunderbird, DAVx5) sync against natively. One handler serves both
//! under `/dav`; contacts live at `addressbooks/<user>/default/<id>.vcf` and
//! events at `calendars/<user>/<coll>/<id>.ics` — `coll` is `default` (personal,
//! kept stable for existing clients) or a calendar id (a shared/team calendar);
//! one collection per calendar the user can see. The principal advertises
//! both home-sets so a client discovers whichever it asks for.
//!
//! A **room** ([`alo_store::CalendarResource`]) is a collection too, at its own
//! id, read-only to every member of the tenant: its members are the meetings
//! that booked it, whoever owns them, and every write into it is `403` — a
//! room's schedule is written by booking it. An `ATTENDEE` that names a room is
//! served with `CUTYPE=ROOM` and, arriving on a PUT, takes the booking.
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

use alo_store::{
    AccountStore, Calendar, CalendarEvent, CalendarId, CalendarResource, Contact, ContactId,
    EventId, ical, vcard,
};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use time::OffsetDateTime;

use crate::state::AppState;

const NS: &str = "xmlns:d=\"DAV:\" xmlns:card=\"urn:ietf:params:xml:ns:carddav\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" xmlns:cs=\"http://calendarserver.org/ns/\"";
const SYNC_PREFIX: &str = "urn:alo:contacts:";
const CAL_SYNC_PREFIX: &str = "urn:alo:calendar:";
/// A room collection's token prefix. Deliberately a different scheme from
/// [`CAL_SYNC_PREFIX`]: a room's members are other people's meetings, so the
/// account modseq cannot describe its state and the token is a hash of it
/// instead (see [`room_tag`]).
const ROOM_SYNC_PREFIX: &str = "urn:alo:room:";
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
        "PUT" => put_object(&acc, &uid, &resource, &headers, &body).await,
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
    /// One calendar collection, by its path segment (`default` = personal, else
    /// a calendar id — a shared/team calendar the user can see).
    Calendar(String),
    /// One calendar object (an event): `(collection segment, event id)`.
    CalObject(String, String),
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
        // The collection segment is `default` (the personal calendar) or a
        // calendar id — any calendar the user can see (own, shared, or team).
        ["calendars", u, coll] if *u == uid => Resource::Calendar((*coll).to_owned()),
        ["calendars", u, coll, obj] if *u == uid => {
            Resource::CalObject((*coll).to_owned(), obj.trim_end_matches(".ics").to_owned())
        }
        _ => Resource::NotFound,
    }
}

/// The personal calendar's stable id for a user (matches
/// `ensure_personal_calendar`), served as the backward-compatible `default`
/// CalDAV collection.
fn personal_cal_id(uid: &str) -> String {
    format!("cal_personal_{uid}")
}

/// A calendar id → its CalDAV collection segment: `default` for the personal
/// calendar (so existing clients keep working), else the calendar id itself.
fn collection_for(uid: &str, cal_id: &str) -> String {
    if cal_id == personal_cal_id(uid) {
        "default".to_owned()
    } else {
        cal_id.to_owned()
    }
}

/// A CalDAV collection segment → the calendar id it addresses.
fn resolve_collection(uid: &str, coll: &str) -> String {
    if coll == "default" {
        personal_cal_id(uid)
    } else {
        coll.to_owned()
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
                // One collection per calendar the user can see (own + shared)…
                let cals = acc.calendars().await.unwrap_or_default();
                for cal in &cals {
                    responses.push_str(&calendar_response(acc, uid, cal).await);
                }
                // …then the tenant's rooms, read-only, so a phone can subscribe
                // to a room's schedule the way it subscribes to a colleague's.
                for room in acc.calendar_resources().await.unwrap_or_default() {
                    let bookings = room_bookings(acc, &room.id).await;
                    responses.push_str(&room_response(uid, &room, &room_tag(&bookings)));
                }
            }
        }
        Resource::Calendar(coll) => {
            if let Some(room) = room_of(acc, coll).await {
                let bookings = room_bookings(acc, &room.id).await;
                responses.push_str(&room_response(uid, &room, &room_tag(&bookings)));
                if depth >= 1 {
                    let rooms = room_addresses(acc).await;
                    for (e, ovs) in &bookings {
                        responses.push_str(&event_propstat(uid, coll, e, false, ovs, &rooms));
                    }
                }
                return multistatus(&responses);
            }
            let cal_id = resolve_collection(uid, coll);
            if let Some(cal) = acc
                .calendars()
                .await
                .unwrap_or_default()
                .into_iter()
                .find(|c| c.id.as_str() == cal_id)
            {
                responses.push_str(&calendar_response(acc, uid, &cal).await);
            } else {
                return status(StatusCode::NOT_FOUND);
            }
            if depth >= 1 {
                let events = match acc
                    .events_of_calendar(&alo_store::CalendarId::new(cal_id))
                    .await
                {
                    Ok(e) => e,
                    Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
                };
                let rooms = room_addresses(acc).await;
                for e in &events {
                    // The ETag covers the override set, so it is loaded even
                    // without calendar-data (cheap: only recurring series hit
                    // the query).
                    let ovs = overrides_for_ics(acc, e).await;
                    responses.push_str(&event_propstat(uid, coll, e, false, &ovs, &rooms));
                }
            }
        }
        Resource::CalObject(coll, id) => match fetch_in_collection(acc, coll, id).await {
            Some(e) => {
                let ovs = overrides_for_ics(acc, &e).await;
                let rooms = room_addresses(acc).await;
                responses.push_str(&event_propstat(uid, coll, &e, false, &ovs, &rooms));
            }
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

async fn calendar_response(acc: &AccountStore, uid: &str, cal: &Calendar) -> String {
    let coll = collection_for(uid, cal.id.as_str());
    let href = format!("/dav/calendars/{uid}/{coll}/");
    let ctag = cal_collection_tag(acc).await;
    let name = esc(&cal.name);
    let color = cal
        .color
        .as_deref()
        .filter(|c| c.starts_with('#'))
        .unwrap_or("#e76f51");
    // A shared/team calendar the viewer can only read advertises no write
    // privileges, so clients present it read-only.
    let privileges = if cal.role == "viewer" {
        "<d:current-user-privilege-set><d:privilege><d:read/></d:privilege></d:current-user-privilege-set>"
    } else {
        ""
    };
    let props = format!(
        "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
         <d:displayname>{name}</d:displayname>\
         <cs:getctag>{ctag}</cs:getctag>\
         <d:sync-token>{ctag}</d:sync-token>\
         <cal:supported-calendar-component-set><cal:comp name=\"VEVENT\"/></cal:supported-calendar-component-set>\
         <cs:calendar-color>{color}ff</cs:calendar-color>\
         {privileges}\
         <d:supported-report-set>\
           <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:calendar-multiget/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:calendar-query/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:free-busy-query/></d:report></d:supported-report>\
         </d:supported-report-set>",
    );
    response(&href, &props)
}

/// A `<response>` for one event object: its href + getetag, and (when
/// `with_data`) the iCalendar as `calendar-data`.
///
/// `coll` is the collection the object is being listed **in**, not the calendar
/// the event sits on: the same meeting appears under its own calendar and under
/// every room it booked, and a client must be handed the href it asked about —
/// an href built from the event's own calendar would point a room's client at a
/// colleague's collection it cannot read.
fn event_propstat(
    uid: &str,
    coll: &str,
    e: &CalendarEvent,
    with_data: bool,
    overrides: &[CalendarEvent],
    rooms: &[String],
) -> String {
    let href = format!("/dav/calendars/{uid}/{coll}/{}.ics", e.id.as_str());
    let etag = event_etag(e, overrides);
    let mut props = format!(
        "<d:getetag>{etag}</d:getetag>\
         <d:getcontenttype>text/calendar; charset=utf-8; component=VEVENT</d:getcontenttype>"
    );
    if with_data {
        // The master plus one VEVENT per RECURRENCE-ID override, so a client sees
        // per-occurrence edits (equals to_ics when there are none).
        props.push_str(&format!(
            "<cal:calendar-data>{}</cal:calendar-data>",
            esc(&ical::to_ics_series_with_rooms(e, overrides, rooms))
        ));
    }
    response(&href, &props)
}

/// The room a collection segment names, or `None` when the segment is an
/// ordinary calendar. Every member of the tenant may see every room, so this
/// asks only the resource table — no grant, no ownership.
async fn room_of(acc: &AccountStore, coll: &str) -> Option<CalendarResource> {
    if coll == "default" {
        return None;
    }
    acc.calendar_resource(&CalendarId::new(coll.to_owned()))
        .await
        .ok()
        .flatten()
}

/// A room collection's members: every meeting that booked it — whoever owns
/// it — each with the override set its served body and ETag cover.
async fn room_bookings(
    acc: &AccountStore,
    room: &CalendarId,
) -> Vec<(CalendarEvent, Vec<CalendarEvent>)> {
    let events = acc.events_of_calendar(room).await.unwrap_or_default();
    let mut out = Vec::with_capacity(events.len());
    for e in events {
        let ovs = overrides_for_ics(acc, &e).await;
        out.push((e, ovs));
    }
    out
}

/// A room collection's change tag, doubling as its sync-token.
///
/// The account modseq — every other collection's tag — cannot serve here: a
/// room's members are other people's meetings, and their writes never touch
/// this caller's modseq, so a modseq-based tag would sit still while the room
/// filled up. A hash of the members' own ETags moves exactly when the room's
/// schedule does.
fn room_tag(bookings: &[(CalendarEvent, Vec<CalendarEvent>)]) -> String {
    let mut hasher = DefaultHasher::new();
    for (e, ovs) in bookings {
        e.id.as_str().hash(&mut hasher);
        event_etag(e, ovs).hash(&mut hasher);
    }
    format!("{ROOM_SYNC_PREFIX}{:016x}", hasher.finish())
}

/// Every room address in the tenant, so a served `ATTENDEE` that names one
/// carries `CUTYPE=ROOM` (RFC 5545 §3.2.3).
async fn room_addresses(acc: &AccountStore) -> Vec<String> {
    acc.calendar_resources()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.email)
        .collect()
}

/// A `<response>` for one room collection: a calendar like any other, minus
/// every write privilege. `calendar-description` carries where the room is —
/// the one fact a phone can show that a name cannot, and data rather than
/// prose, so nothing here needs translating.
fn room_response(uid: &str, room: &CalendarResource, tag: &str) -> String {
    let href = format!("/dav/calendars/{uid}/{}/", room.id.as_str());
    let name = esc(&room.name);
    let description = room.location.as_deref().map_or_else(String::new, |loc| {
        format!(
            "<cal:calendar-description>{}</cal:calendar-description>",
            esc(loc)
        )
    });
    let props = format!(
        "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
         <d:displayname>{name}</d:displayname>\
         {description}\
         <cs:getctag>{tag}</cs:getctag>\
         <d:sync-token>{tag}</d:sync-token>\
         <cal:supported-calendar-component-set><cal:comp name=\"VEVENT\"/></cal:supported-calendar-component-set>\
         <d:current-user-privilege-set><d:privilege><d:read/></d:privilege></d:current-user-privilege-set>\
         <d:supported-report-set>\
           <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:calendar-multiget/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:calendar-query/></d:report></d:supported-report>\
           <d:supported-report><d:report><cal:free-busy-query/></d:report></d:supported-report>\
         </d:supported-report-set>",
    );
    response(&href, &props)
}

/// The event a `<coll>/<id>.ics` path names. Under a room's collection that is
/// the booking itself, whoever owns it — the point of a shared room calendar;
/// anywhere else it is the caller's own visible event, as it has always been
/// (an href whose collection segment has gone stale still resolves).
async fn fetch_in_collection(acc: &AccountStore, coll: &str, id: &str) -> Option<CalendarEvent> {
    match room_of(acc, coll).await {
        Some(room) => acc
            .event_in_calendar(&room.id, &EventId::new(id.to_owned()))
            .await
            .ok()
            .flatten(),
        None => fetch_event(acc, id).await,
    }
}

/// The overrides to include alongside a recurring event's `calendar-data` (empty
/// for a one-off or an un-edited series).
async fn overrides_for_ics(acc: &AccountStore, e: &CalendarEvent) -> Vec<CalendarEvent> {
    if e.recurrence.is_none() && e.rdates.is_empty() {
        return Vec::new();
    }
    acc.override_occurrences(&e.id).await.unwrap_or_default()
}

// ---- REPORT (multiget + sync-collection) ------------------------------

async fn report(acc: &AccountStore, uid: &str, resource: &Resource, body: &[u8]) -> Response {
    let text = String::from_utf8_lossy(body);
    match resource {
        Resource::Addressbook => report_contacts(acc, uid, &text).await,
        Resource::Calendar(coll) => report_events(acc, uid, coll, &text).await,
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

async fn report_events(acc: &AccountStore, uid: &str, coll: &str, text: &str) -> Response {
    if let Some(room) = room_of(acc, coll).await {
        return report_room(acc, uid, coll, &room, text).await;
    }
    if text.contains("sync-collection") {
        return cal_sync_collection(acc, uid, coll, text).await;
    }
    if text.contains("free-busy-query") {
        return free_busy_query(acc, uid, coll, text).await;
    }
    // calendar-multiget (explicit hrefs), or a calendar-query — which we answer
    // by its <C:time-range> when present, else the whole collection.
    let hrefs = extract_hrefs(text);
    let rooms = room_addresses(acc).await;
    let mut responses = String::new();
    if hrefs.is_empty() {
        let cal_id = resolve_collection(uid, coll);
        let events = match acc.events_of_calendar(&CalendarId::new(cal_id)).await {
            Ok(e) => e,
            Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
        };
        let range = extract_time_range(text);
        for e in &events {
            let ovs = overrides_for_ics(acc, e).await;
            if !in_window(e, &ovs, range) {
                continue;
            }
            responses.push_str(&event_propstat(uid, coll, e, true, &ovs, &rooms));
        }
    } else {
        for href in hrefs {
            match cal_href_object_id(&href) {
                Some(id) => match fetch_event(acc, &id).await {
                    Some(e) => {
                        let ovs = overrides_for_ics(acc, &e).await;
                        responses.push_str(&event_propstat(uid, coll, &e, true, &ovs, &rooms));
                    }
                    None => responses.push_str(&not_found_response(&href)),
                },
                None => responses.push_str(&not_found_response(&href)),
            }
        }
    }
    multistatus(&responses)
}

/// Whether an event belongs in a `calendar-query`'s answer. No range → every
/// member. With one, the store's expansion decides (the same function the
/// Agenda uses), and a per-occurrence override is checked too: an instance
/// moved INTO the window would otherwise be dropped with its series.
fn in_window(
    e: &CalendarEvent,
    overrides: &[CalendarEvent],
    range: Option<(OffsetDateTime, OffsetDateTime)>,
) -> bool {
    let Some((start, end)) = range else {
        return true;
    };
    event_overlaps(e, start, end)
        || overrides
            .iter()
            .any(|o| o.starts_at < end && o.ends_at > start)
}

/// Every REPORT a room's collection answers. Its members are read once and the
/// whole report is served from that list — a multiget over a room is scoped by
/// construction then, because an href the room does not hold is simply not in
/// it.
async fn report_room(
    acc: &AccountStore,
    uid: &str,
    coll: &str,
    room: &CalendarResource,
    text: &str,
) -> Response {
    if text.contains("free-busy-query") {
        return room_free_busy(acc, room, text).await;
    }
    let bookings = room_bookings(acc, &room.id).await;
    if text.contains("sync-collection") {
        return room_sync_collection(uid, coll, &bookings, text);
    }
    let rooms = room_addresses(acc).await;
    let hrefs = extract_hrefs(text);
    let mut responses = String::new();
    if hrefs.is_empty() {
        let range = extract_time_range(text);
        for (e, ovs) in &bookings {
            if in_window(e, ovs, range) {
                responses.push_str(&event_propstat(uid, coll, e, true, ovs, &rooms));
            }
        }
    } else {
        for href in hrefs {
            let found = cal_href_object_id(&href)
                .and_then(|id| bookings.iter().find(|(e, _)| e.id.as_str() == id));
            match found {
                Some((e, ovs)) => {
                    responses.push_str(&event_propstat(uid, coll, e, true, ovs, &rooms));
                }
                None => responses.push_str(&not_found_response(&href)),
            }
        }
    }
    multistatus(&responses)
}

/// `sync-collection` on a room (RFC 6578). The token is a state hash, not a
/// sequence, so this answers the two cases it can answer exactly and refuses
/// the third rather than guessing: no token → an initial sync carrying every
/// member; the current token → nothing changed; any other token → `403`
/// `DAV:valid-sync-token` (§3.2), which sends the client to a full listing —
/// the same round it would make on a changed ctag.
fn room_sync_collection(
    uid: &str,
    coll: &str,
    bookings: &[(CalendarEvent, Vec<CalendarEvent>)],
    body: &str,
) -> Response {
    let token = room_tag(bookings);
    match extract_sync_token(body) {
        Some(sent) if sent == token => sync_multistatus(String::new(), &token),
        Some(_) => xml_response(
            StatusCode::FORBIDDEN,
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <d:error {NS}><d:valid-sync-token/></d:error>"
            ),
        ),
        None => {
            let mut responses = String::new();
            for (e, ovs) in bookings {
                responses.push_str(&event_propstat(uid, coll, e, false, ovs, &[]));
            }
            sync_multistatus(responses, &token)
        }
    }
}

/// A room collection's `free-busy-query` (RFC 4791 §7.10): when the room is
/// taken, never by whom or for what. The bookings come back through the one
/// expansion, so a moved instance is busy where it moved to.
async fn room_free_busy(acc: &AccountStore, room: &CalendarResource, body: &str) -> Response {
    let Some((from, to)) = extract_time_range(body) else {
        return status(StatusCode::BAD_REQUEST);
    };
    if to <= from {
        return status(StatusCode::BAD_REQUEST);
    }
    let held = match acc.resource_bookings_in_range(&room.id, from, to).await {
        Ok(e) => e,
        Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let busy = alo_store::merged_busy_spans(&held, from, to);
    let ics = ical::to_vfreebusy(
        &format!("freebusy-{}", room.id.as_str()),
        from,
        to,
        &busy,
        OffsetDateTime::now_utc(),
    );
    let mut resp = (StatusCode::OK, ics).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    resp
}

async fn cal_sync_collection(acc: &AccountStore, uid: &str, coll: &str, body: &str) -> Response {
    let cal_id = resolve_collection(uid, coll);
    let since = extract_sync_token(body)
        .and_then(|t| t.strip_prefix(CAL_SYNC_PREFIX).map(str::to_owned))
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0);
    // Event changes are account-wide (one modseq); keep only those on this
    // collection's calendar so each collection syncs independently.
    let changes = match acc.changes("Event", since, MAX_SYNC).await {
        Ok(c) => c,
        Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let mut responses = String::new();
    for id in changes.created.iter().chain(changes.updated.iter()) {
        if let Some(e) = fetch_event(acc, id).await
            && e.calendar_id.as_str() == cal_id
        {
            let ovs = overrides_for_ics(acc, &e).await;
            responses.push_str(&event_propstat(uid, coll, &e, false, &ovs, &[]));
        }
    }
    for id in &changes.destroyed {
        // A destroyed event's calendar is gone; report the removal under this
        // collection (the client drops it wherever it held it).
        responses.push_str(&not_found_response(&format!(
            "/dav/calendars/{uid}/{coll}/{id}.ics"
        )));
    }
    sync_multistatus(
        responses,
        &format!("{CAL_SYNC_PREFIX}{}", changes.new_state),
    )
}

/// A `sync-collection` answer: the member responses plus the token the client
/// sends back next time.
fn sync_multistatus(responses: String, token: &str) -> Response {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <d:multistatus {NS}>{responses}<d:sync-token>{token}</d:sync-token></d:multistatus>"
    );
    xml_response(StatusCode::MULTI_STATUS, xml)
}

/// `free-busy-query` REPORT (RFC 4791 §7.10): the collection's busy time in
/// the requested window, answered as an RFC 5545 §3.6.4 `VFREEBUSY` — merged
/// spans, clamped to the window, and nothing else. The store's one expansion
/// function supplies the instances (recurrence, moved occurrences, EXDATEs all
/// honoured), and the serializer has no field for event detail, so a viewer of
/// a shared calendar learns *when*, never *what*.
async fn free_busy_query(acc: &AccountStore, uid: &str, coll: &str, body: &str) -> Response {
    // The report carries exactly one time-range (RFC 4791 §9.11); without a
    // parseable one there is no window to answer.
    let Some((from, to)) = extract_time_range(body) else {
        return status(StatusCode::BAD_REQUEST);
    };
    if to <= from {
        return status(StatusCode::BAD_REQUEST);
    }
    let cal_id = resolve_collection(uid, coll);
    // The collection must be a calendar the caller can see — an unshared or
    // foreign calendar id stays unprobeable (404), exactly as PROPFIND.
    if !acc
        .calendars()
        .await
        .unwrap_or_default()
        .iter()
        .any(|c| c.id.as_str() == cal_id)
    {
        return status(StatusCode::NOT_FOUND);
    }
    // events_in_range materializes per-occurrence instances (overrides in
    // place, cancelled ones absent) across the account's visible calendars;
    // keep this collection's, then reduce to spans.
    let events = match acc.events_in_range(from, to).await {
        Ok(e) => e,
        Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let on_calendar: Vec<CalendarEvent> = events
        .into_iter()
        .filter(|e| e.calendar_id.as_str() == cal_id)
        .collect();
    let busy = alo_store::merged_busy_spans(&on_calendar, from, to);
    let ics = ical::to_vfreebusy(
        &format!("freebusy-{cal_id}"),
        from,
        to,
        &busy,
        OffsetDateTime::now_utc(),
    );
    // The response is the iCalendar object itself (RFC 4791 §7.10), not a
    // multistatus.
    let mut resp = (StatusCode::OK, ics).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    resp
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
    sync_multistatus(responses, &format!("{SYNC_PREFIX}{}", changes.new_state))
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
        Resource::CalObject(coll, id) => match fetch_in_collection(acc, coll, id).await {
            Some(e) => {
                let ovs = overrides_for_ics(acc, &e).await;
                let rooms = room_addresses(acc).await;
                serve(
                    head,
                    ical::to_ics_series_with_rooms(&e, &ovs, &rooms),
                    &event_etag(&e, &ovs),
                    "text/calendar; charset=utf-8",
                )
            }
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
    uid: &str,
    resource: &Resource,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    match resource {
        Resource::Object(id) => put_contact_object(acc, id, headers, body).await,
        Resource::CalObject(coll, id) => put_event_object(acc, uid, coll, id, headers, body).await,
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
    uid: &str,
    coll: &str,
    id: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    // A room's collection is read-only to everyone, admin included: its members
    // are meetings that booked it, and the only door into a room's schedule is
    // that booking. Refused here rather than through the store's `can_edit`, so
    // the answer is the permission it is (403) and no write is ever attempted.
    if room_of(acc, coll).await.is_some() {
        return status(StatusCode::FORBIDDEN);
    }
    let existing = fetch_event(acc, id).await;
    if header_has(headers, header::IF_NONE_MATCH, "*") && existing.is_some() {
        return status(StatusCode::PRECONDITION_FAILED);
    }
    if let Some(want) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()) {
        let current = match &existing {
            Some(e) => Some(event_etag(e, &overrides_for_ics(acc, e).await)),
            None => None,
        };
        if current.as_deref() != Some(want.trim()) {
            return status(StatusCode::PRECONDITION_FAILED);
        }
    }
    // The whole resource is read (RFC 4791 §4.1): the master VEVENT plus any
    // RECURRENCE-ID override instances a phone-originated per-occurrence edit
    // carries (RFC 5545 §3.8.4.4); a STATUS:CANCELLED instance has already
    // been folded into the master's EXDATEs by the parser.
    let Some(series) = ical::from_ics_series(&String::from_utf8_lossy(body), id) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let mut event = series.master;
    // The href is authoritative: store under the path id (= iCalendar UID).
    event.id = EventId::new(id.to_owned());
    // The event lands on the collection's calendar (iCalendar carries no
    // grouping). put_event refuses a calendar the caller can't edit, so a PUT
    // to a read-only shared collection is denied rather than misfiled.
    event.calendar_id = if coll == "default" {
        match acc.ensure_personal_calendar().await {
            Ok(c) => c,
            Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        CalendarId::new(resolve_collection(uid, coll))
    };
    // Overrides ride only on a recurring master — on a one-off they would
    // never be served, so the stored set is cleared instead of fed.
    let overrides = if event.recurrence.is_some() || !event.rdates.is_empty() {
        series.overrides
    } else {
        Vec::new()
    };
    // An ATTENDEE that names a room books it — the same one check the Agenda and
    // the JSON API run, so a phone cannot take a room the web app would have
    // refused. The hold is taken BEFORE the write, so a refusal leaves no
    // half-made meeting behind; RFC 4791 §5.3.2 sanctions a PUT answering 409.
    let rooms = match resources_named(acc, &event.attendees).await {
        Ok(r) => r,
        Err(()) => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    if rooms.is_empty() {
        // The PUT body is the whole resource: a room dropped from the guest list
        // is a room let go of.
        if acc.unbook_event(&event.id).await.is_err() {
            return status(StatusCode::INTERNAL_SERVER_ERROR);
        }
    } else if let Err(err) = acc.book_resources(&event.id, &event, &rooms).await {
        return booking_refusal(err);
    }
    match acc.put_event(&event.id, &event).await {
        Ok(created) => {
            // The PUT body is the whole resource: reconcile the stored
            // override set to what the client sent — an override it no longer
            // includes is removed (replace_overrides is edit-gated too, but
            // put_event just proved edit access; an error here is a race).
            if let Err(err) = acc.replace_overrides(&event.id, &overrides).await {
                return match err {
                    alo_store::StoreError::NotFound => {
                        cal_write_denial(acc, event.calendar_id.as_str()).await
                    }
                    _ => status(StatusCode::INTERNAL_SERVER_ERROR),
                };
            }
            // The response ETag must equal what a later GET computes: hash
            // over the served override set.
            let ovs = overrides_for_ics(acc, &event).await;
            created_or_updated(created, &event_etag(&event, &ovs))
        }
        // The store refuses a calendar the caller can't edit as NotFound. On
        // the wire that is 403 when the collection is visible (a read-only
        // grant — the denial is a permission, not existence) and 404 when it
        // isn't, so an unshared calendar id stays unprobeable.
        Err(alo_store::StoreError::NotFound) => {
            // The room was held for a meeting that never happened — give it back.
            let _ = acc.unbook_event(&event.id).await;
            cal_write_denial(acc, event.calendar_id.as_str()).await
        }
        Err(_) => {
            let _ = acc.unbook_event(&event.id).await;
            status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// The rooms a guest list names, in order and without repeats. An attendee that
/// is nobody's room is just a guest — one list carries both, which is what makes
/// booking a room one act rather than two.
///
/// # Errors
/// `Err(())` when the lookup fails (the caller answers 500 — refusing is the
/// only safe answer when we cannot tell whether a room was named).
async fn resources_named(acc: &AccountStore, attendees: &[String]) -> Result<Vec<CalendarId>, ()> {
    let mut out: Vec<CalendarId> = Vec::new();
    for attendee in attendees {
        let found = acc
            .calendar_resource_by_email(attendee)
            .await
            .map_err(|_| ())?;
        if let Some(resource) = found
            && !out.iter().any(|id| id.as_str() == resource.id.as_str())
        {
            out.push(resource.id);
        }
    }
    Ok(out)
}

/// A refused room booking on the wire. `409` is what RFC 4791 §5.3.2 leaves a
/// PUT for a state the server cannot accept; the store's own words ride along
/// as the body, naming the room and the slot it is already taken for, because a
/// client that shows the server's reason beats one that says "error".
fn booking_refusal(err: alo_store::StoreError) -> Response {
    let detail = match err {
        alo_store::StoreError::Conflict(detail) => detail,
        // A room retired between reading the guest list and taking the lock.
        alo_store::StoreError::NotFound => "that resource is no longer bookable".to_owned(),
        _ => return status(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let mut resp = (StatusCode::CONFLICT, detail).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}

/// The wire status for a refused calendar write: `403` when the caller can
/// see the calendar (a read-only grant), else `404`.
async fn cal_write_denial(acc: &AccountStore, cal_id: &str) -> Response {
    let visible = acc
        .calendars()
        .await
        .unwrap_or_default()
        .iter()
        .any(|c| c.id.as_str() == cal_id);
    if visible {
        status(StatusCode::FORBIDDEN)
    } else {
        status(StatusCode::NOT_FOUND)
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
        Resource::CalObject(coll, id) => {
            // Cancelling out of a room is done by editing the meeting, not by
            // deleting it from the room's collection — which is somebody else's
            // meeting as often as not.
            if room_of(acc, coll).await.is_some() {
                return status(StatusCode::FORBIDDEN);
            }
            if let Some(want) = want {
                match fetch_event(acc, id).await {
                    Some(e) => {
                        let ovs = overrides_for_ics(acc, &e).await;
                        if event_etag(&e, &ovs) != want.trim() {
                            return status(StatusCode::PRECONDITION_FAILED);
                        }
                    }
                    None => return status(StatusCode::NOT_FOUND),
                }
            }
            match acc.delete_event(&EventId::new(id.to_owned())).await {
                // delete_event removes only from an editable calendar; an
                // event the caller can *see* but not edit is a permission
                // denial, not a missing resource.
                Err(alo_store::StoreError::NotFound) if fetch_event(acc, id).await.is_some() => {
                    status(StatusCode::FORBIDDEN)
                }
                other => store_delete(other),
            }
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

/// A per-event ETag from a hash of the event's fields and its per-occurrence
/// override set — deliberately NOT the serialized iCalendar, whose `DTSTAMP`
/// changes each render and would churn. The overrides are part of the served
/// body, so an edit touching only one instance must move the tag — otherwise
/// a second device would keep serving its cached copy of the series.
fn event_etag(e: &CalendarEvent, overrides: &[CalendarEvent]) -> String {
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
    // Overrides arrive slot-sorted (`override_occurrences`), so the hash is
    // stable across reads.
    for ov in overrides {
        ov.recurrence_id
            .map(|t| t.unix_timestamp())
            .hash(&mut hasher);
        ov.summary.hash(&mut hasher);
        ov.description.hash(&mut hasher);
        ov.location.hash(&mut hasher);
        ov.starts_at.unix_timestamp().hash(&mut hasher);
        ov.ends_at.unix_timestamp().hash(&mut hasher);
        ov.all_day.hash(&mut hasher);
    }
    format!("\"{:016x}\"", hasher.finish())
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

/// The `<C:time-range start=.. end=..>` of a calendar-query, as UTC instants.
/// `None` (→ unfiltered) if absent or either bound is missing/unparseable.
fn extract_time_range(body: &str) -> Option<(OffsetDateTime, OffsetDateTime)> {
    let seg = &body[body.find("time-range")?..];
    let start = attr_value(seg, "start").and_then(parse_ical_utc)?;
    let end = attr_value(seg, "end").and_then(parse_ical_utc)?;
    Some((start, end))
}

/// The value of an XML attribute `name="..."` (or `'...'`) in `seg`.
fn attr_value<'a>(seg: &'a str, name: &str) -> Option<&'a str> {
    let at = seg.find(&format!("{name}="))? + name.len() + 1;
    let rest = seg.get(at..)?;
    let quote = rest.chars().next()?;
    let inner = &rest[1..];
    inner.find(quote).map(|end| &inner[..end])
}

/// Parse a compact iCalendar UTC timestamp (`YYYYMMDDTHHMMSSZ`) to UTC.
fn parse_ical_utc(v: &str) -> Option<OffsetDateTime> {
    let d = v.trim().trim_end_matches('Z');
    let n = |a: usize, b: usize| d.get(a..b)?.parse::<u32>().ok();
    let date = time::Date::from_calendar_date(
        d.get(0..4)?.parse().ok()?,
        time::Month::try_from(n(4, 6)? as u8).ok()?,
        n(6, 8)? as u8,
    )
    .ok()?;
    let (h, mi, s) = if d.contains('T') {
        (
            n(9, 11)? as u8,
            n(11, 13)? as u8,
            n(13, 15).unwrap_or(0) as u8,
        )
    } else {
        (0, 0, 0)
    };
    Some(OffsetDateTime::new_utc(
        date,
        time::Time::from_hms(h, mi, s).ok()?,
    ))
}

/// Whether an event has an instance in `[start, end)`. A recurring master (an
/// `RRULE` and/or `RDATE`s) is answered by the store's one expansion function —
/// the same one the Agenda range listing uses, so CalDAV never grows a second
/// recurrence implementation; a one-off must overlap directly. The caller
/// still checks per-occurrence overrides, which can move an instance into the
/// window that the unoverridden series would miss.
fn event_overlaps(e: &CalendarEvent, start: OffsetDateTime, end: OffsetDateTime) -> bool {
    if e.recurrence.is_some() || !e.rdates.is_empty() {
        alo_store::calendar::series_occurs_in_range(e, start, end)
    } else {
        e.starts_at < end && e.ends_at > start
    }
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
