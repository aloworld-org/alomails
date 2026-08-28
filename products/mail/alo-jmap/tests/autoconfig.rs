//! Mail-client autoconfiguration endpoints, driven through the real router:
//! they must serve correct XML **without authentication** (the client has no
//! credentials yet) and must not echo caller markup into the document.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use crate::common::harness;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

/// Sends a request through the router and returns (status, content-type, body).
async fn text(h: &common::Harness, req: Request<Body>) -> (StatusCode, String, String) {
    let resp = h.app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    (status, ctype, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn mozilla_config_is_public_and_well_formed() {
    let h = harness("autoconf-moz").await;
    // No Authorization header: discovery precedes credentials.
    let req = Request::builder()
        .method("GET")
        .uri("/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress=someone@acme.eu")
        .body(Body::empty())
        .unwrap();
    let (status, ctype, body) = text(&h, req).await;

    assert_eq!(status, StatusCode::OK, "served without auth");
    assert!(ctype.contains("xml"), "content-type is XML: {ctype}");
    assert!(body.contains("<clientConfig version=\"1.1\">"));
    // The queried domain names the provider; the server host is base_url's host.
    assert!(
        body.contains("<domain>acme.eu</domain>"),
        "queried domain used"
    );
    assert!(
        body.contains("<hostname>test</hostname>"),
        "server host advertised"
    );
    assert!(body.contains("<port>993</port>"), "IMAPS port");
    assert!(body.contains("<port>465</port>"), "SMTPS port");
    assert!(
        body.contains("<username>%EMAILADDRESS%</username>"),
        "placeholder username"
    );
}

#[tokio::test]
async fn mozilla_config_rejects_markup_in_query() {
    let h = harness("autoconf-inj").await;
    // A hostile emailaddress must not break out of the XML: the value is not a
    // sane hostname, so it is dropped in favour of the configured fallback and
    // no angle brackets from the query appear in the body.
    let req = Request::builder()
        .method("GET")
        .uri("/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress=x@%3Cscript%3E")
        .body(Body::empty())
        .unwrap();
    let (status, _ctype, body) = text(&h, req).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<script>"),
        "no injected markup in the config"
    );
    assert!(
        body.contains("<domain>test</domain>"),
        "fell back to configured domain"
    );
}

#[tokio::test]
async fn outlook_autodiscover_echoes_escaped_login() {
    let h = harness("autoconf-out").await;
    let request_body = "<Autodiscover><Request>\
        <EMailAddress>someone@acme.eu</EMailAddress>\
        <AcceptableResponseSchema>x</AcceptableResponseSchema></Request></Autodiscover>";
    let req = Request::builder()
        .method("POST")
        .uri("/autodiscover/autodiscover.xml")
        .header("content-type", "application/xml")
        .body(Body::from(request_body))
        .unwrap();
    let (status, ctype, body) = text(&h, req).await;

    assert_eq!(status, StatusCode::OK, "served without auth");
    assert!(ctype.contains("xml"));
    assert!(body.contains("<Type>IMAP</Type>"));
    assert!(body.contains("<Port>993</Port>"));
    assert!(body.contains("<Type>SMTP</Type>"));
    assert!(body.contains("<Port>465</Port>"));
    assert!(
        body.contains("<LoginName>someone@acme.eu</LoginName>"),
        "login echoed"
    );
}

/// Through the real router: Autodiscover says nothing about MAPI/HTTP, even to
/// an Outlook that announces it can speak it.
///
/// This used to be a safety property with the adapter switched off. Since
/// [ADR 0056] it is permanent — the adapter is gone and alo's own client over
/// 443 is the product — and the test is kept because the failure it guards is
/// silent and expensive. An Outlook handed a `mapiHttp` block does **not** fall
/// back to the IMAP settings beside it in the same document: it goes to the URL
/// it was given, finds the single-page app there, and reports a broken server.
/// Advertising an endpoint we do not serve breaks the mail that works today.
///
/// [ADR 0056]: ../../../../docs/decisions/0056-our-own-client-on-443-is-the-product.md
#[tokio::test]
async fn outlook_autodiscover_never_mentions_mapi_http() {
    let h = harness("autoconf-mapi-off").await;
    let req = Request::builder()
        .method("POST")
        .uri("/autodiscover/autodiscover.xml")
        // Exactly what a MAPI-capable Outlook sends.
        .header("X-MapiHttpCapability", "1")
        .body(Body::from(
            "<Request><EMailAddress>someone@acme.eu</EMailAddress></Request>",
        ))
        .unwrap();
    let (status, _ctype, body) = text(&h, req).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("mapiHttp"),
        "advertised MAPI/HTTP, which we do not serve: {body}"
    );
    assert!(!body.contains("/mapi/"), "leaked a MAPI endpoint: {body}");
    // ...and the settings that do work are still there, unharmed.
    assert!(body.contains("<Type>IMAP</Type>"), "{body}");
    assert!(body.contains("<Type>SMTP</Type>"), "{body}");
}

#[tokio::test]
async fn outlook_autodiscover_omits_login_for_junk_body() {
    let h = harness("autoconf-out2").await;
    // A body with a markup-bearing address is refused as a login (would need
    // escaping to be safe) — the element is simply omitted, Outlook falls back
    // to the address the user typed.
    let request_body = "<Request><EMailAddress><x>@acme.eu</EMailAddress></Request>";
    let req = Request::builder()
        .method("POST")
        .uri("/autodiscover/autodiscover.xml")
        .body(Body::from(request_body))
        .unwrap();
    let (status, _ctype, body) = text(&h, req).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<LoginName>"),
        "no login element for a junk address"
    );
    assert!(!body.contains("<x>"), "no injected markup");
}
