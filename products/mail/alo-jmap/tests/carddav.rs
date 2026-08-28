//! End-to-end CardDAV (RFC 6352): the request sequence a real client
//! runs — OPTIONS, principal/addressbook discovery via PROPFIND, PUT to
//! create, GET, listing, addressbook-multiget + sync-collection REPORTs,
//! preconditions, DELETE — plus HTTP Basic auth and the mandatory
//! tenant-isolation test (one account can never reach another's
//! address objects). Driven through the real router over Postgres.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{Harness, harness};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;

fn basic(email: &str) -> String {
    let raw = format!("{email}:s3cret-pw");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

/// Sends a DAV request (any method) authenticated as `h`'s user.
async fn dav(
    h: &Harness,
    method: &str,
    path: &str,
    depth: Option<&str>,
    body: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", basic(&h.email));
    if let Some(d) = depth {
        b = b.header("depth", d);
    }
    let req = b.body(Body::from(body.to_owned())).unwrap();
    h.app.clone().oneshot_dav(req).await
}

/// A tiny extension so the test can read the response headers too (the
/// shared `send` returns JSON only).
trait OneshotDav {
    async fn oneshot_dav(self, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String);
}
impl OneshotDav for axum::Router {
    async fn oneshot_dav(self, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
        use tower::ServiceExt;
        let resp = self.oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap();
        (
            status,
            headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }
}

const VCARD: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada Lovelace\r\nN:Lovelace;Ada;;;\r\nEMAIL:ada@eng.uk\r\nEND:VCARD\r\n";

#[tokio::test]
async fn full_client_sync_flow() {
    let h = harness("dav-flow").await;
    let uid = &h.account_id;

    // OPTIONS advertises CardDAV.
    let (status, headers, _) = dav(&h, "OPTIONS", "/dav/", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .get("dav")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("addressbook"),
        "DAV: header advertises addressbook"
    );

    // Principal discovery.
    let (status, _h, xml) = dav(
        &h,
        "PROPFIND",
        &format!("/dav/principals/{uid}/"),
        Some("0"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains("addressbook-home-set"), "{xml}");
    assert!(xml.contains(&format!("/dav/addressbooks/{uid}/")), "{xml}");

    // Empty addressbook (Depth:1 lists only the collection).
    let book = format!("/dav/addressbooks/{uid}/default/");
    let (status, _h, xml) = dav(&h, "PROPFIND", &book, Some("1"), "").await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(
        xml.contains("<card:addressbook/>"),
        "the addressbook resourcetype: {xml}"
    );
    assert!(!xml.contains(".vcf"), "no objects yet: {xml}");

    // PUT creates an object at a client-chosen href.
    // Unique object id per run (the `contacts.id` column is global; real
    // clients use UUIDs). Embedding the unique account id avoids colliding
    // with a row left by a prior panicked run on the shared dev DB.
    let obj_id = format!("ada-{uid}");
    let obj = format!("{book}{obj_id}.vcf");
    let (status, headers, _) = dav(&h, "PUT", &obj, None, VCARD).await;
    assert_eq!(status, StatusCode::CREATED, "first PUT creates");
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_owned();
    assert!(!etag.is_empty());

    // GET returns the vCard with the same ETag.
    let (status, headers, body) = dav(&h, "GET", &obj, None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "text/vcard; charset=utf-8"
    );
    assert!(body.contains("FN:Ada Lovelace"), "{body}");
    assert_eq!(headers.get("etag").unwrap().to_str().unwrap(), etag);

    // Depth:1 now lists the object with its etag.
    let (_s, _h, xml) = dav(&h, "PROPFIND", &book, Some("1"), "").await;
    assert!(xml.contains(&format!("{obj_id}.vcf")), "{xml}");
    assert!(xml.contains("getetag"), "{xml}");

    // addressbook-multiget REPORT returns the card data.
    let multiget = format!(
        "<c:addressbook-multiget xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:carddav\">\
         <d:href>{obj}</d:href></c:addressbook-multiget>"
    );
    let (status, _h, xml) = dav(&h, "REPORT", &book, Some("1"), &multiget).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains("address-data"), "{xml}");
    assert!(
        xml.contains("FN:Ada Lovelace"),
        "the vCard is embedded: {xml}"
    );

    // sync-collection REPORT (initial, empty token) returns the object and
    // a fresh sync-token.
    let sync = "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token/></d:sync-collection>";
    let (status, _h, xml) = dav(&h, "REPORT", &book, None, sync).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains(&format!("{obj_id}.vcf")), "{xml}");
    assert!(
        xml.contains("<d:sync-token>urn:alo:contacts:"),
        "carries a sync-token: {xml}"
    );

    // If-None-Match: * on an existing object is refused (create-only).
    let mut b = Request::builder()
        .method("PUT")
        .uri(&obj)
        .header("authorization", basic(&h.email))
        .header("if-none-match", "*");
    b = b.header("content-type", "text/vcard");
    let (status, ..) = h
        .app
        .clone()
        .oneshot_dav(b.body(Body::from(VCARD)).unwrap())
        .await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED);

    // DELETE removes it; GET then 404s.
    let (status, ..) = dav(&h, "DELETE", &obj, None, "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, ..) = dav(&h, "GET", &obj, None, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unauthenticated_gets_a_basic_challenge() {
    let h = harness("dav-auth").await;
    let req = Request::builder()
        .method("PROPFIND")
        .uri("/dav/")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = h.app.clone().oneshot_dav(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        headers
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Basic"),
        "issues a Basic challenge"
    );
}

#[tokio::test]
async fn carddav_is_tenant_isolated() {
    let a = harness("dav-iso-a").await;
    let b = harness("dav-iso-b").await;

    // A creates a contact.
    let obj_id = format!("secret-{}", a.account_id);
    let a_obj = format!("/dav/addressbooks/{}/default/{obj_id}.vcf", a.account_id);
    let (status, ..) = dav(&a, "PUT", &a_obj, None, VCARD).await;
    assert_eq!(status, StatusCode::CREATED);

    // B, authenticated as B, cannot reach A's object via A's path (the
    // path user != B → NotFound), nor via B's own path + A's id (B's
    // store has no such object).
    let via_a_path = dav(&b, "GET", &a_obj, None, "").await.0;
    assert_eq!(
        via_a_path,
        StatusCode::NOT_FOUND,
        "no cross-user path access"
    );
    let via_b_path = format!("/dav/addressbooks/{}/default/{obj_id}.vcf", b.account_id);
    assert_eq!(
        dav(&b, "GET", &via_b_path, None, "").await.0,
        StatusCode::NOT_FOUND,
        "B's own book does not contain A's id"
    );

    // B's addressbook listing is empty; A's still has the object.
    let (_s, _h, b_xml) = dav(
        &b,
        "PROPFIND",
        &format!("/dav/addressbooks/{}/default/", b.account_id),
        Some("1"),
        "",
    )
    .await;
    assert!(!b_xml.contains(".vcf"), "B's book stays empty: {b_xml}");
    let (_s, _h, a_xml) = dav(
        &a,
        "PROPFIND",
        &format!("/dav/addressbooks/{}/default/", a.account_id),
        Some("1"),
        "",
    )
    .await;
    assert!(
        a_xml.contains(&format!("{obj_id}.vcf")),
        "A still has the object: {a_xml}"
    );
}
