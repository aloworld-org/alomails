//! Address-book import (POST /contacts/import, a .vcf upload) and export
//! (GET /contacts/export, the whole book as .vcf), through the real
//! router — including that an import lands only in the importing user's
//! account (tenant isolation) and that export→import round-trips.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{Harness, api, get_text, harness, post_raw};
use serde_json::{Value, json};

const CONTACTS_USING: [&str; 2] = ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:contacts"];

async fn contact_count(h: &Harness) -> usize {
    let body = json!({
        "using": CONTACTS_USING,
        "methodCalls": [["Contact/get", { "accountId": h.account_id, "ids": Value::Null }, "0"]],
    });
    let (_s, resp) = api(&h.app, &h.token, body).await;
    resp["methodResponses"][0][1]["list"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0)
}

const SAMPLE_VCF: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada Lovelace\r\n\
N:Lovelace;Ada;;;\r\nEMAIL;TYPE=work:ada@analytical.eng\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Grace Hopper\r\nEMAIL:grace@navy.mil\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:4.0\r\nEND:VCARD\r\n";

#[tokio::test]
async fn import_creates_contacts_and_reports_skips() {
    let h = harness("import-basic").await;
    let (status, resp) = post_raw(&h.app, &h.token, "/contacts/import", SAMPLE_VCF).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(resp["imported"], 2, "two named cards imported");
    assert_eq!(resp["skipped"], 1, "the nameless card is reported skipped");
    assert_eq!(contact_count(&h).await, 2);
}

#[tokio::test]
async fn export_returns_a_vcard_document_that_reimports() {
    let h = harness("export-roundtrip").await;
    post_raw(&h.app, &h.token, "/contacts/import", SAMPLE_VCF).await;

    let (status, vcf) = get_text(&h.app, &h.token, "/contacts/export").await;
    assert_eq!(status, 200);
    assert!(vcf.contains("FN:Ada Lovelace"), "{vcf}");
    assert!(vcf.contains("FN:Grace Hopper"), "{vcf}");
    assert_eq!(vcf.matches("BEGIN:VCARD").count(), 2);

    // The exported document imports cleanly into a second account.
    let other = harness("export-target").await;
    let (_s, resp) = post_raw(&other.app, &other.token, "/contacts/import", &vcf).await;
    assert_eq!(resp["imported"], 2, "{resp}");
}

#[tokio::test]
async fn imported_contacts_are_tenant_isolated() {
    // A imports; B (another tenant) must not see any of A's contacts.
    let a = harness("import-iso-a").await;
    let b = harness("import-iso-b").await;
    let (_s, resp) = post_raw(&a.app, &a.token, "/contacts/import", SAMPLE_VCF).await;
    assert_eq!(resp["imported"], 2);
    assert_eq!(contact_count(&a).await, 2, "A has its imported contacts");
    assert_eq!(contact_count(&b).await, 0, "B's address book is untouched");
    // And B's export is empty (no cross-tenant leak into the .vcf).
    let (_s, vcf) = get_text(&b.app, &b.token, "/contacts/export").await;
    assert!(
        !vcf.contains("ada@analytical.eng"),
        "no cross-tenant leak: {vcf}"
    );
}
