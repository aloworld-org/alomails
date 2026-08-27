//! M6.1 scripted wire transcripts: the canonical CardDAV sync and CalDAV
//! exchanges, driven as literal HTTP/1.1 over a real TCP socket (the
//! production proxy terminates TLS in front of exactly these bytes) and
//! recorded request/response line by line. Each test asserts the exchange
//! it captures. When `ALO_WIRE_TRANSCRIPTS` names a directory, the trimmed
//! transcript is written there — `scripts/wire-transcripts.sh` runs these
//! tests and splices the output into `docs/interop.md`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::SocketAddr;

use base64::Engine;
use common::{Harness, harness};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Serves the harness router on a real loopback socket.
async fn spawn(h: &Harness) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = h.app.clone();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// A recording raw-HTTP DAV client: one connection per request
/// (`Connection: close`), everything sent logged `C:`, everything received
/// logged `S:`. XML response bodies are broken at tag boundaries for
/// readability; long bodies are trimmed with a count of what was cut.
struct Dav {
    addr: SocketAddr,
    auth: String,
    log: Vec<String>,
}

const BODY_LINE_CAP: usize = 60;

impl Dav {
    fn new(addr: SocketAddr, email: &str) -> Self {
        let auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{email}:s3cret-pw"))
        );
        Self {
            addr,
            auth,
            log: Vec::new(),
        }
    }

    /// An annotation line in the transcript (not wire bytes).
    fn note(&mut self, s: &str) {
        self.log.push(format!("  ({s})"));
    }

    /// Sends one request; returns `(status, response headers, response body)`.
    async fn request(
        &mut self,
        method: &str,
        path: &str,
        extra: &[(&str, &str)],
        body: &str,
    ) -> (u16, String, String) {
        let mut req = format!("{method} {path} HTTP/1.1\r\n");
        self.log.push(format!("C: {method} {path} HTTP/1.1"));
        let header = |req: &mut String, log: &mut Vec<String>, k: &str, v: &str, shown: &str| {
            req.push_str(&format!("{k}: {v}\r\n"));
            log.push(format!("C: {k}: {shown}"));
        };
        header(
            &mut req,
            &mut self.log,
            "Host",
            "alo.example",
            "alo.example",
        );
        let auth = self.auth.clone();
        header(
            &mut req,
            &mut self.log,
            "Authorization",
            &auth,
            "Basic <base64 of \"alice@example.test:<password>\">",
        );
        for (k, v) in extra {
            header(&mut req, &mut self.log, k, v, v);
        }
        if !body.is_empty() {
            let len = body.len().to_string();
            header(&mut req, &mut self.log, "Content-Length", &len, &len);
        }
        header(&mut req, &mut self.log, "Connection", "close", "close");
        req.push_str("\r\n");
        self.log.push("C:".to_owned());
        req.push_str(body);
        for line in body.lines() {
            self.log.push(format!("C: {line}"));
        }

        let mut tcp = TcpStream::connect(self.addr).await.unwrap();
        tcp.write_all(req.as_bytes()).await.unwrap();
        tcp.flush().await.unwrap();
        let mut raw = Vec::new();
        tcp.read_to_end(&mut raw).await.unwrap();
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let (head, resp_body) = raw.split_once("\r\n\r\n").expect("response head");

        for line in head.lines() {
            // The Date header changes every run; keep the transcript stable.
            if line.to_ascii_lowercase().starts_with("date:") {
                self.log.push("S: date: <date>".to_owned());
            } else {
                self.log.push(format!("S: {line}"));
            }
        }
        self.log.push("S:".to_owned());
        let pretty = if resp_body.trim_start().starts_with('<') {
            resp_body.replace("><", ">\n<")
        } else {
            resp_body.to_owned()
        };
        let body_lines: Vec<&str> = pretty.lines().collect();
        for line in body_lines.iter().take(BODY_LINE_CAP) {
            self.log.push(format!("S: {line}"));
        }
        if body_lines.len() > BODY_LINE_CAP {
            self.log.push(format!(
                "  (… {} more lines)",
                body_lines.len() - BODY_LINE_CAP
            ));
        }

        let status: u16 = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .expect("status line");
        (status, head.to_owned(), resp_body.to_owned())
    }
}

/// The text between the first `start_tag` and the next `<`.
fn between<'a>(hay: &'a str, start_tag: &str) -> &'a str {
    let i = hay.find(start_tag).expect("tag present") + start_tag.len();
    let rest = &hay[i..];
    &rest[..rest.find('<').expect("tag closed")]
}

/// Writes the captured transcript when `ALO_WIRE_TRANSCRIPTS` names a
/// directory; first line is the section title, `normalize` stabilises
/// run-specific values.
fn save(name: &str, title: &str, log: &[String], normalize: &[(String, &str)]) {
    let Some(dir) = std::env::var_os("ALO_WIRE_TRANSCRIPTS") else {
        return;
    };
    let mut text = format!("{title}\n");
    for line in log {
        let mut l = line.clone();
        for (from, to) in normalize {
            l = l.replace(from.as_str(), to);
        }
        text.push_str(&l);
        text.push('\n');
    }
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.txt")), text).unwrap();
}

const VCARD: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada Lovelace\r\nN:Lovelace;Ada;;;\r\nEMAIL:ada@eng.uk\r\nEND:VCARD\r\n";

/// The CardDAV sync a real client runs: discovery, PUT, an initial
/// `sync-collection`, changes, and the incremental report that carries the
/// new object and the 404 for the deleted one.
#[tokio::test]
async fn carddav_sync_transcript() {
    let h = harness("wire-carddav").await;
    let uid = h.account_id.clone();
    let addr = spawn(&h).await;
    let mut d = Dav::new(addr, &h.email);
    let book = format!("/dav/addressbooks/{uid}/default/");

    let (status, head, _) = d.request("OPTIONS", "/dav/", &[], "").await;
    assert_eq!(status, 200);
    assert!(head.to_ascii_lowercase().contains("addressbook"), "{head}");

    let (status, _, xml) = d
        .request(
            "PROPFIND",
            &format!("/dav/principals/{uid}/"),
            &[("Depth", "0")],
            "",
        )
        .await;
    assert_eq!(status, 207);
    assert!(xml.contains("addressbook-home-set"), "{xml}");

    // Contact hrefs are globally unique in the store, so the names carry the
    // account id (normalised to ACCOUNT in the saved transcript) — a fixed
    // `ada.vcf` would 409 against the row an earlier run left behind.
    let ada = format!("{book}ada-{uid}.vcf");
    let (status, ..) = d
        .request("PUT", &ada, &[("Content-Type", "text/vcard")], VCARD)
        .await;
    assert_eq!(status, 201);

    let sync_init =
        "<d:sync-collection xmlns:d=\"DAV:\">\r\n<d:sync-token/>\r\n</d:sync-collection>";
    let (status, _, xml) = d.request("REPORT", &book, &[], sync_init).await;
    assert_eq!(status, 207);
    assert!(xml.contains(&format!("ada-{uid}.vcf")), "{xml}");
    let token = between(&xml, "sync-token>").to_owned();
    assert!(!token.is_empty());

    d.note("another client adds bob.vcf and deletes ada.vcf");
    let bob_card = VCARD
        .replace("Ada Lovelace", "Bob Sander")
        .replace("N:Lovelace;Ada;;;", "N:Sander;Bob;;;");
    let (status, ..) = d
        .request(
            "PUT",
            &format!("{book}bob-{uid}.vcf"),
            &[("Content-Type", "text/vcard")],
            &bob_card,
        )
        .await;
    assert_eq!(status, 201);
    let (status, ..) = d.request("DELETE", &ada, &[], "").await;
    assert_eq!(status, 204);

    let sync_next = format!(
        "<d:sync-collection xmlns:d=\"DAV:\">\r\n<d:sync-token>{token}</d:sync-token>\r\n</d:sync-collection>"
    );
    let (status, _, xml) = d.request("REPORT", &book, &[], &sync_next).await;
    assert_eq!(status, 207);
    assert!(xml.contains(&format!("bob-{uid}.vcf")), "{xml}");
    assert!(xml.contains("404"), "the deleted member is reported: {xml}");

    save(
        "carddav-sync",
        "CardDAV: discovery, PUT, initial and incremental sync-collection",
        &d.log,
        &[(h.email.clone(), "alice@example.test"), (uid, "ACCOUNT")],
    );
}

/// CalDAV: PUT of a Brussels-zoned weekly series that crosses the
/// 2026-10-25 DST switch, a time-range `calendar-query`, the `VFREEBUSY`
/// answer (busy/free only — the DST-correct instants, never the title),
/// and `sync-collection`.
#[tokio::test]
async fn caldav_transcript() {
    let h = harness("wire-caldav").await;
    let uid = h.account_id.clone();
    let addr = spawn(&h).await;
    let mut d = Dav::new(addr, &h.email);
    let cal = format!("/dav/calendars/{uid}/default/");

    let (status, _, xml) = d
        .request(
            "PROPFIND",
            &format!("/dav/calendars/{uid}/"),
            &[("Depth", "1")],
            "",
        )
        .await;
    assert_eq!(status, 207);
    assert!(xml.contains("default/"), "{xml}");

    let series = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//transcript//EN\r\n\
         BEGIN:VEVENT\r\nUID:standup-wire\r\nDTSTAMP:20260101T000000Z\r\n\
         DTSTART;TZID=Europe/Brussels:20261019T090000\r\n\
         DTEND;TZID=Europe/Brussels:20261019T093000\r\n\
         RRULE:FREQ=WEEKLY;COUNT=3\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let (status, ..) = d
        .request(
            "PUT",
            &format!("{cal}standup-wire.ics"),
            &[("Content-Type", "text/calendar")],
            series,
        )
        .await;
    assert_eq!(status, 201);

    let query = "<c:calendar-query xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\r\n\
         <d:prop><d:getetag/><c:calendar-data/></d:prop>\r\n\
         <c:filter><c:comp-filter name=\"VCALENDAR\"><c:comp-filter name=\"VEVENT\">\r\n\
         <c:time-range start=\"20261001T000000Z\" end=\"20261130T000000Z\"/>\r\n\
         </c:comp-filter></c:comp-filter></c:filter>\r\n</c:calendar-query>";
    let (status, _, xml) = d.request("REPORT", &cal, &[("Depth", "1")], query).await;
    assert_eq!(status, 207);
    assert!(xml.contains("Standup"), "{xml}");

    d.note("free/busy for the same window: the 09:00 Brussels series stays 09:00 local across the 2026-10-25 switch (07:00Z, then 08:00Z)");
    let fb = "<c:free-busy-query xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\r\n\
         <c:time-range start=\"20261001T000000Z\" end=\"20261130T000000Z\"/>\r\n\
         </c:free-busy-query>";
    let (status, _, body) = d.request("REPORT", &cal, &[], fb).await;
    assert_eq!(status, 200);
    assert!(body.contains("BEGIN:VFREEBUSY"), "{body}");
    assert!(
        body.contains("FREEBUSY;FBTYPE=BUSY:20261019T070000Z/20261019T073000Z"),
        "before the switch: {body}"
    );
    assert!(
        body.contains("FREEBUSY;FBTYPE=BUSY:20261102T080000Z/20261102T083000Z"),
        "after the switch: {body}"
    );
    assert!(!body.contains("Standup"), "busy/free only: {body}");

    let sync_init =
        "<d:sync-collection xmlns:d=\"DAV:\">\r\n<d:sync-token/>\r\n</d:sync-collection>";
    let (status, _, xml) = d.request("REPORT", &cal, &[], sync_init).await;
    assert_eq!(status, 207);
    assert!(xml.contains("standup-wire.ics"), "{xml}");

    save(
        "caldav",
        "CalDAV: PUT of a zoned recurring series, time-range query, VFREEBUSY, sync-collection",
        &d.log,
        &[(h.email.clone(), "alice@example.test"), (uid, "ACCOUNT")],
    );
}
