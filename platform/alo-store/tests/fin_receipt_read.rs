//! Reading an uploaded receipt through the account door (alo Finance, B4.06b) —
//! the arc from a file in Drive to candidate fields, and the isolation that
//! makes it safe to offer.
//!
//! Two questions this suite exists to answer, and they have the same answer:
//!
//! - **Another tenant's receipt** (Law 1, every wave): tenant A cannot read a
//!   node of tenant B's, and the denial is the clean `NotFound` — never bytes,
//!   never a `Db` error.
//! - **A colleague's receipt** (the finance module's own rule): a receipt names a
//!   restaurant, a pharmacy, a city on a date, and it lives in the claimant's
//!   *personal* Drive. A co-tenant is as blind to it as an outsider, because the
//!   reading goes through `drive_node` — the same door the Drive UI uses — and
//!   not through a finance table with a tenant id in it.
//!
//! The third thing proved here is the one the design note cares about most: this
//! path **writes nothing**. Reading a receipt twice leaves the same empty
//! expense list behind, because the claim is created afterwards by a person who
//! confirmed the numbers.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::fin_receipt::{Confidence, Evidence};
use alo_store::{
    AccountStore, DriveLocation, DriveNodeId, MAX_RECEIPT_BYTES, NewDriveFile, StoreError,
};
use bytes::Bytes;
use time::{Date, Month};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a typed validation failure whose message names the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(message)) => assert!(
            message.contains(expect),
            "validation {message:?} should name {expect:?}"
        ),
        other => panic!("expected Validation naming {expect:?}, got: {other:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real day")
}

/// The day every reading in this suite happens on.
fn today() -> Date {
    day(2026, Month::March, 20)
}

/// Uploads bytes as a file in the caller's own Drive and returns its node —
/// exactly the two calls the web client makes (`POST /jmap/upload`, then
/// `POST /drive/files`) before it asks finance to read one.
async fn upload(
    acc: &AccountStore,
    name: &str,
    content_type: &str,
    bytes: &'static str,
) -> DriveNodeId {
    let blob = acc
        .put_blob(Bytes::from_static(bytes.as_bytes()), Some(content_type))
        .await
        .expect("a blob of this tenant's own");
    acc.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: name.to_owned(),
            blob_id: blob.as_str().to_owned(),
            size: bytes.len() as i64,
            content_type: Some(content_type.to_owned()),
            ..NewDriveFile::default()
        },
    )
    .await
    .expect("a file in the caller's own Drive")
}

/// A German till roll as a text file — the shape a PDF's text layer arrives in
/// once `extract` has had it.
const TILL_ROLL: &str = "REWE Markt GmbH\nHauptstr. 12\n80331 München\nDatum 14.03.2026\n\
     Milch 1,19\nBrot 2,49\nSUMME EUR 11,90\nMwSt 19% 1,90\n";

#[tokio::test]
async fn a_receipt_in_drive_reads_into_candidates_and_writes_nothing() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "receipt-read").await;
    let node = upload(&acc, "REWE_2026-03-14.txt", "text/plain", TILL_ROLL).await;

    let reading = acc
        .read_receipt(&node, today())
        .await
        .expect("a receipt of the caller's own");

    assert_eq!(
        reading.node_id.as_str(),
        node.as_str(),
        "echoed back to attach"
    );
    assert_eq!(reading.filename, "REWE_2026-03-14.txt");
    assert_eq!(reading.content_type.as_deref(), Some("text/plain"));
    assert_eq!(reading.size, TILL_ROLL.len() as i64);
    assert!(reading.had_text, "a text file has a text layer");

    let parsed = &reading.parsed;
    assert!(parsed.found_anything());
    let merchant = parsed.merchant.as_ref().expect("who was paid");
    assert_eq!(merchant.value, "REWE Markt GmbH");
    assert_eq!(
        merchant.confidence,
        Confidence::High,
        "a legal form is certain"
    );
    assert_eq!(
        parsed.spent_on.as_ref().expect("the day").value,
        day(2026, Month::March, 14)
    );
    assert_eq!(parsed.gross_cents.as_ref().expect("the total").value, 1190);
    assert_eq!(parsed.vat_cents.as_ref().expect("the tax").value, 190);
    assert_eq!(parsed.vat_rate_bp.as_ref().expect("the rate").value, 1900);
    assert_eq!(parsed.currency.as_ref().expect("the currency").value, "EUR");

    // The evidence indexes the lines the reading itself carries, so a form can
    // highlight what it read without re-deriving anything.
    let Evidence::Text { line, start, end } = parsed.gross_cents.as_ref().unwrap().evidence else {
        panic!("the total came from the text");
    };
    let quoted: String = parsed.lines[line]
        .chars()
        .skip(start)
        .take(end - start)
        .collect();
    assert_eq!(quoted, "11,90");

    // Nothing was written. Reading it again is the same answer, and the
    // claimant's expense list is still empty: the claim is what a person creates
    // after confirming these numbers.
    let again = acc.read_receipt(&node, today()).await.expect("read twice");
    assert_eq!(
        again.parsed.gross_cents.as_ref().map(|found| found.value),
        Some(1190)
    );
    assert!(
        acc.expenses(day(2026, Month::January, 1), today(), None)
            .await
            .expect("a claims list")
            .is_empty(),
        "reading a receipt creates no claim, not even a draft"
    );
}

#[tokio::test]
async fn a_photograph_reads_as_no_text_and_whatever_its_name_says() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "receipt-photo").await;
    // A phone camera writes pixels. The bytes here are not a real JPEG on
    // purpose: nothing in this path parses an image, and pretending otherwise
    // would test a decoder we do not have.
    let node = upload(&acc, "REWE_2026-03-14.jpg", "image/jpeg", "\u{fffd}pixels").await;

    let reading = acc
        .read_receipt(&node, today())
        .await
        .expect("still a read");
    assert!(!reading.had_text, "there was nothing here to read");
    // The name still said two things, both at the lowest confidence.
    let merchant = reading.parsed.merchant.as_ref().expect("from the name");
    assert_eq!(merchant.value, "REWE");
    assert_eq!(merchant.confidence, Confidence::Low);
    assert_eq!(merchant.evidence, Evidence::Filename);
    assert_eq!(
        reading
            .parsed
            .spent_on
            .as_ref()
            .expect("from the name")
            .value,
        day(2026, Month::March, 14)
    );
    assert!(
        reading.parsed.gross_cents.is_none(),
        "a file name never states an amount"
    );
}

#[tokio::test]
async fn an_unreadable_receipt_is_an_empty_answer_and_not_a_failure() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "receipt-blank").await;
    let node = upload(&acc, "scan.jpg", "image/jpeg", "\u{fffd}\u{fffd}").await;

    let reading = acc
        .read_receipt(&node, today())
        .await
        .expect("not an error");
    assert!(!reading.had_text);
    assert!(
        !reading.parsed.found_anything(),
        "the person types the claim — the pre-B4.06 experience, unchanged"
    );
}

#[tokio::test]
async fn a_folder_and_an_oversized_file_are_refused_by_name() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "receipt-refuse").await;

    let folder = acc
        .drive_create_folder(&DriveLocation::Personal, None, "Receipts")
        .await
        .expect("a folder of the caller's own");
    assert_invalid(
        acc.read_receipt(&folder, today()).await,
        "a receipt is a file",
    );

    // `size` is what the upload declared, so the cheap refusal happens before a
    // single byte is fetched. (The real byte length is checked too, for the
    // upload that lies the other way.)
    let blob = acc
        .put_blob(Bytes::from_static(b"small"), Some("application/pdf"))
        .await
        .expect("a blob");
    let huge = acc
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "poster.pdf".to_owned(),
                blob_id: blob.as_str().to_owned(),
                size: MAX_RECEIPT_BYTES + 1,
                content_type: Some("application/pdf".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .expect("a file");
    assert_invalid(acc.read_receipt(&huge, today()).await, "12 MB");

    // An id that never existed is the same answer as one belonging to somebody
    // else: absent.
    assert_not_found(
        acc.read_receipt(&DriveNodeId::new("nope".to_owned()), today())
            .await,
    );
}

#[tokio::test]
async fn another_tenants_receipt_and_a_colleagues_are_both_simply_absent() {
    let store = common::test_store().await;
    let (mine, _user, _inbox) = common::fresh_account(&store, "receipt-mine").await;
    let (outsider, _u2, _i2) = common::fresh_account(&store, "receipt-outsider").await;

    let node = upload(&mine, "Apotheke_2026-03-14.txt", "text/plain", TILL_ROLL).await;
    // Law 1: another tenant reaching this node gets a clean denial, not bytes.
    assert_not_found(outsider.read_receipt(&node, today()).await);

    // And a colleague inside the same tenant is just as blind: a receipt lives
    // in the claimant's personal Drive, and this path reads it through the door
    // that enforces that.
    let tenant = store.for_tenant(mine.tenant().clone());
    let colleague = tenant
        .create_user("colleague@receipt.test")
        .await
        .expect("a co-tenant");
    let theirs = store.for_account(mine.tenant().clone(), colleague);
    assert_not_found(theirs.read_receipt(&node, today()).await);

    // The claimant themself still reads it — the denials above are about who is
    // asking, not about the file being unreachable.
    assert!(
        mine.read_receipt(&node, today())
            .await
            .expect("the claimant's own")
            .parsed
            .found_anything()
    );
}
