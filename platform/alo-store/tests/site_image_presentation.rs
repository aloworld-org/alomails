//! How an image is framed (ADR 0036, S2.07a): the crop rectangle, the focal
//! point and the decorative flag, proved against a real Postgres rather than
//! against serde alone.
//!
//! Three properties are load-bearing. The presentation props **survive the
//! round trip through storage** unchanged, so a re-framed photo stays
//! re-framed. A rectangle that leaves the image, or a focal point that
//! contradicts its own crop, is **refused at the write gate** with the rule
//! named — the store is the second door the editor's first door agrees with.
//! And framing is **tenant-scoped like everything else**: another tenant can
//! neither read how an image is framed nor re-frame it, and the page it lives
//! on is indistinguishable from a page that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{SiteId, StoreError};
use serde_json::{Value, json};

fn subdomain(tag: &str) -> String {
    format!(
        "{tag}-{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

/// A gallery of one image, framed however the caller says.
fn gallery(image: Value) -> Value {
    json!({
        "schema_version": 1,
        "sections": [{"type": "gallery", "heading": "Inside", "images": [image]}]
    })
}

fn framed_image() -> Value {
    json!({
        "blob_id": "9hK3vQ2mR8pT1xWz4bC5dg",
        "alt": "The restored Probat roasting drum",
        "crop": {"x_bp": 1250, "y_bp": 0, "width_bp": 7500, "height_bp": 10000},
        "focal": {"x_bp": 4000, "y_bp": 3500}
    })
}

fn assert_conflict_mentioning<T: std::fmt::Debug>(result: Result<T, StoreError>, needle: &str) {
    match result {
        Err(StoreError::Conflict(detail)) => assert!(
            detail.contains(needle),
            "refusal {detail:?} does not name the broken rule ({needle:?})"
        ),
        other => panic!("expected a Conflict naming {needle:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn framing_survives_storage_and_a_broken_frame_is_refused_by_name() {
    let store = common::test_store().await;
    let tenant = store
        .create_tenant("site-image-presentation")
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-image-presentation.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Nordwind", &subdomain("framing"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();

    // ---- the frame round-trips through the JSONB column ---------------------
    account
        .set_page_sections(&site, &home, gallery(framed_image()))
        .await
        .unwrap();
    let stored = account.site_page(&site, &home).await.unwrap().unwrap();
    let image = &stored.sections["sections"][0]["images"][0];
    assert_eq!(image["crop"]["x_bp"], json!(1250));
    assert_eq!(image["crop"]["width_bp"], json!(7500));
    assert_eq!(image["focal"], json!({"x_bp": 4000, "y_bp": 3500}));
    assert!(
        image.get("decorative").is_none(),
        "a flag nobody set stays absent: {image}"
    );

    // ---- an image saved without the props keeps none of them ----------------
    let plain = json!({"blob_id": "2fN8wE5rT9yU3iO7pA1sDg", "alt": "Cupping table"});
    account
        .set_page_sections(&site, &home, gallery(plain.clone()))
        .await
        .unwrap();
    let stored = account.site_page(&site, &home).await.unwrap().unwrap();
    assert_eq!(
        stored.sections["sections"][0]["images"][0], plain,
        "storing an unframed image must not invent a frame for it"
    );

    // ---- the write gate refuses a frame that cannot mean anything -----------
    let outside = json!({
        "blob_id": "9hK3vQ2mR8pT1xWz4bC5dg",
        "alt": "",
        "crop": {"x_bp": 6000, "y_bp": 0, "width_bp": 5000, "height_bp": 10000}
    });
    assert_conflict_mentioning(
        account
            .set_page_sections(&site, &home, gallery(outside))
            .await,
        "must stay inside the image",
    );

    let contradiction = json!({
        "blob_id": "9hK3vQ2mR8pT1xWz4bC5dg",
        "alt": "",
        "crop": {"x_bp": 0, "y_bp": 0, "width_bp": 5000, "height_bp": 5000},
        "focal": {"x_bp": 9000, "y_bp": 1000}
    });
    assert_conflict_mentioning(
        account
            .set_page_sections(&site, &home, gallery(contradiction))
            .await,
        "must lie inside the crop",
    );

    let alt_on_a_decorative_image = json!({
        "blob_id": "9hK3vQ2mR8pT1xWz4bC5dg",
        "alt": "The restored Probat roasting drum",
        "decorative": true
    });
    assert_conflict_mentioning(
        account
            .set_page_sections(&site, &home, gallery(alt_on_a_decorative_image))
            .await,
        "decorative image must have empty alt text",
    );

    // A refused write leaves the stored framing exactly as it was.
    let after = account.site_page(&site, &home).await.unwrap().unwrap();
    assert_eq!(after.sections["sections"][0]["images"][0], plain);
}

/// The wrong-tenant proof: tenant B holds tenant A's site and page ids and can
/// do nothing with them — not read how an image is framed, not re-frame it.
#[tokio::test]
async fn another_tenant_can_neither_read_nor_change_how_an_image_is_framed() {
    let store = common::test_store().await;

    let tenant_a = store.create_tenant("site-framing-a").await.unwrap();
    let user_a = store
        .for_tenant(tenant_a.clone())
        .create_user("owner@site-framing-a.test")
        .await
        .unwrap();
    let alice = store.for_account(tenant_a, user_a);
    let site = alice
        .create_site("Nordwind", &subdomain("framing-a"))
        .await
        .unwrap();
    let home = alice
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    alice
        .set_page_sections(&site, &home, gallery(framed_image()))
        .await
        .unwrap();

    let tenant_b = store.create_tenant("site-framing-b").await.unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("owner@site-framing-b.test")
        .await
        .unwrap();
    let mallory = store.for_account(tenant_b, user_b);

    assert!(
        mallory.site_page(&site, &home).await.unwrap().is_none(),
        "another tenant's page must be indistinguishable from one that never existed"
    );
    match mallory
        .set_page_sections(&site, &home, gallery(framed_image()))
        .await
    {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound re-framing another tenant's image, got {other:?}"),
    }

    // The owner's framing is untouched by the attempt.
    let stored = alice.site_page(&site, &home).await.unwrap().unwrap();
    assert_eq!(
        stored.sections["sections"][0]["images"][0]["focal"],
        json!({"x_bp": 4000, "y_bp": 3500})
    );
}
