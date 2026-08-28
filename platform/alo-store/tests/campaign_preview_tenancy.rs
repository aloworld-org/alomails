//! The preview, and the three people it must refuse to be (C3.6, ADR 0044;
//! Law 1: isolation is tested, not assumed).
//!
//! `campaign_merge.rs`'s unit tests hold the resolution down without a database
//! and `campaign_html_golden.rs` holds the compilation. What needs a real
//! Postgres, and what this suite is for, is the join this wave introduces: a
//! **stored letter** meeting a **stored person**.
//!
//! - **A neighbour's letter is a row that does not exist.** Previewing another
//!   tenant's campaign is a `NotFound`, from an id that is perfectly valid one
//!   tenant over.
//! - **A neighbour's person is nobody.** Two tenants holding the *same address*
//!   is the ordinary case — a shared supplier, a marketplace — and it is exactly
//!   where a preview could quietly render one tenant's letter with the other's
//!   record. So the same address is seeded on both sides with different names,
//!   and each tenant's preview must greet its own.
//! - **Somebody this tenant may not mail cannot be previewed as.** A suppressed
//!   address and one with no consent record are both `NotFound`, because a
//!   preview is the operation that ends with a colleague reading a rendered
//!   letter addressed to a person and deciding to send it. This is the "who may
//!   not" test the campaigns queue requires of anything touching who is mailed:
//!   the suppression is absolute (ADR 0044 §2), including for a rehearsal.
//! - **A preview against nobody is offered rather than faked.** A tenant with an
//!   empty audience still gets a letter, and it says the words in it are the
//!   writer's fallbacks — which is also the copy most of a form-built audience
//!   actually receives.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::campaign_unsubscribe_link::UnsubscribeInvitation;
use alo_store::{
    AccountStore, CampaignContent, CampaignId, CampaignMergeField, CampaignPreview, ConsentSource,
    FallbackReason, NewCampaign, NewCampaignConsent, NewCustomer, NewSuppression, PreviewAgainst,
    PreviewAs, Store, StoreError, SuppressionReason, TenantStore,
};
use serde_json::json;

/// The way out a preview shows because the recipient will see it. Its URL is a
/// placeholder: a preview has no recipient, so there is no token to mint.
fn unsub() -> UnsubscribeInvitation {
    UnsubscribeInvitation {
        one_click_url: "https://alo.test/jmap/campaign-unsubscribe/preview".to_owned(),
        page_url: "https://alo.test/unsubscribe/preview".to_owned(),
        topic: Some("Nieuwsbrief".to_owned()),
        link_text: "Uitschrijven".to_owned(),
    }
}

/// A tenant with one user: the account door for campaigns and customers, the
/// tenant door for suppression (which has no logged-in colleague behind it).
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore) {
    let tenant = store.create_tenant(&format!("cprev-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@cprev.test")).await.unwrap();
    (store.for_account(tenant, user), ts)
}

/// Somebody in the audience: a billing customer with a name and a country, and
/// a consent record so they are somebody the tenant may mail.
async fn reachable(account: &AccountStore, name: &str, email: &str, country: &str) {
    account
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: country.to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    account
        .record_campaign_consent(&NewCampaignConsent {
            address: email,
            source: ConsentSource::Manual,
            source_ref: None,
            statement: "Ticked the newsletter box at the counter",
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// Somebody the tenant holds a record of and has **no** consent for.
async fn unconsented(account: &AccountStore, name: &str, email: &str, country: &str) {
    account
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: country.to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
}

/// A letter that greets by name, writes to the address, and names the country —
/// so a preview against the wrong record is visible in the rendered words
/// rather than only in a field report.
fn a_letter() -> CampaignContent {
    CampaignContent::from_value(json!({
        "schema_version": 1,
        "blocks": [
            { "type": "heading", "id": "h1", "level": 1, "text": "Hi {{first_name|there}}" },
            { "type": "paragraph", "id": "p1",
              "text": "We write to {{email|your address}} about deliveries in {{country|your country}}." },
        ],
    }))
    .expect("a body a campaign may carry")
}

/// A campaign whose subject is personalised too.
async fn campaign(account: &AccountStore, subject: &str) -> CampaignId {
    account
        .create_campaign(&NewCampaign {
            subject,
            preheader: Some("Prices for {{first_name|our customers}}"),
            topic: "Monthly newsletter",
            content: a_letter(),
        })
        .await
        .unwrap()
        .id
}

/// What one field printed in a preview, and whether it was the reader's own.
fn printed(preview: &CampaignPreview, field: CampaignMergeField) -> Vec<(String, bool)> {
    preview
        .fields
        .iter()
        .filter(|used| used.field == field)
        .map(|used| (used.value.clone(), used.fell_back))
        .collect()
}

fn is_not_found(result: Result<CampaignPreview, StoreError>) -> bool {
    matches!(result, Err(StoreError::NotFound))
}

#[tokio::test]
async fn a_preview_resolves_against_a_real_record_and_says_which_words_are_theirs() {
    let store = common::test_store().await;
    let (account, _) = tenant(&store, "real").await;
    reachable(&account, "Jean Dupont", "jean@cprev.test", "FR").await;
    let id = campaign(&account, "Spring prices for {{first_name|you}}").await;

    let preview = account
        .preview_campaign(
            &id,
            &PreviewAs::Recipient("  JEAN@Cprev.TEST ".to_owned()),
            &unsub(),
        )
        .await
        .unwrap();

    // The address is folded on the way in, exactly as everywhere else on this
    // surface — a screen echoing back what it was shown lands where it means.
    assert_eq!(
        preview.against,
        PreviewAgainst::Recipient {
            address: "jean@cprev.test".to_owned(),
            name: Some("Jean Dupont".to_owned()),
            country: Some("FR".to_owned()),
        }
    );

    // The three things the item names: the HTML, the text part, and the fields.
    assert!(preview.html.contains("Hi Jean"), "{}", preview.html);
    assert!(preview.text.contains("Hi Jean"), "{}", preview.text);
    assert!(preview.html.contains("jean@cprev.test"));
    assert!(preview.text.contains("jean@cprev.test"));
    assert_eq!(preview.subject, "Spring prices for Jean");
    assert_eq!(
        preview.preheader.as_deref(),
        Some("Prices for Jean"),
        "the preview text is personalised too — it is the second line of an inbox"
    );
    assert!(
        !preview.html.contains("{{") && !preview.text.contains("{{"),
        "no placeholder may survive into a preview"
    );

    // Nothing fell back for this reader, and the report says so rather than
    // leaving the screen to guess from words that read identically either way.
    assert_eq!(
        printed(&preview, CampaignMergeField::FirstName),
        vec![("Jean".to_owned(), false)]
    );
    assert_eq!(
        printed(&preview, CampaignMergeField::Country),
        vec![("FR".to_owned(), false)]
    );
}

#[tokio::test]
async fn one_address_held_by_two_tenants_previews_as_each_tenants_own_record() {
    // The ordinary case a leak hides in: a shared supplier, a marketplace
    // address, the same person buying from two of our customers. If the
    // recipient lookup were not tenant-scoped, both previews would greet the
    // same name and nobody would notice which.
    let store = common::test_store().await;
    let (ours, _) = tenant(&store, "ours").await;
    let (theirs, _) = tenant(&store, "theirs").await;
    reachable(&ours, "Ann Ours", "shared@cprev.test", "BE").await;
    reachable(&theirs, "Bea Theirs", "shared@cprev.test", "NL").await;

    let ours_id = campaign(&ours, "Ours").await;
    let theirs_id = campaign(&theirs, "Theirs").await;

    let our_preview = ours
        .preview_campaign(
            &ours_id,
            &PreviewAs::Recipient("shared@cprev.test".into()),
            &unsub(),
        )
        .await
        .unwrap();
    let their_preview = theirs
        .preview_campaign(
            &theirs_id,
            &PreviewAs::Recipient("shared@cprev.test".into()),
            &unsub(),
        )
        .await
        .unwrap();

    assert!(our_preview.html.contains("Hi Ann"), "{}", our_preview.html);
    assert!(!our_preview.html.contains("Bea"));
    assert!(our_preview.html.contains("BE") && !our_preview.html.contains("NL"));
    assert!(their_preview.html.contains("Hi Bea"));
    assert!(!their_preview.html.contains("Ann"));

    // And neither tenant can reach the other's letter, from an id that is a
    // perfectly good campaign one tenant over.
    assert!(is_not_found(
        ours.preview_campaign(&theirs_id, &PreviewAs::Fallbacks, &unsub())
            .await
    ));
    assert!(is_not_found(
        theirs
            .preview_campaign(&ours_id, &PreviewAs::AnyRecipient, &unsub())
            .await
    ));
}

#[tokio::test]
async fn a_preview_cannot_be_rendered_as_somebody_this_tenant_may_not_mail() {
    // The rule this queue requires of anything touching who is mailed: the
    // failing case, written down. A suppression is absolute — including for the
    // rehearsal that ends with somebody deciding the letter looks ready.
    let store = common::test_store().await;
    let (account, ts) = tenant(&store, "refuse").await;
    reachable(&account, "Gone Away", "gone@cprev.test", "BE").await;
    unconsented(&account, "Never Asked", "quiet@cprev.test", "BE").await;
    let id = campaign(&account, "Spring prices").await;

    // Before the suppression they are previewable, so the assertion after it is
    // about the suppression and not about a typo in the address.
    assert!(
        account
            .preview_campaign(
                &id,
                &PreviewAs::Recipient("gone@cprev.test".into()),
                &unsub()
            )
            .await
            .is_ok()
    );

    ts.suppress_campaign_address(&NewSuppression {
        address: "gone@cprev.test",
        reason: SuppressionReason::Unsubscribe,
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    assert!(
        is_not_found(
            account
                .preview_campaign(
                    &id,
                    &PreviewAs::Recipient("gone@cprev.test".into()),
                    &unsub()
                )
                .await
        ),
        "a suppressed address was rendered a letter"
    );
    assert!(
        is_not_found(
            account
                .preview_campaign(
                    &id,
                    &PreviewAs::Recipient("quiet@cprev.test".into()),
                    &unsub()
                )
                .await
        ),
        "somebody with no consent record was rendered a letter"
    );
    assert!(
        is_not_found(
            account
                .preview_campaign(
                    &id,
                    &PreviewAs::Recipient("stranger@cprev.test".into()),
                    &unsub()
                )
                .await
        ),
        "an address this tenant has never held answers the same way, so the \
         refusal is not an oracle for which of the three is true"
    );

    // A re-stated consent does not undo it — the same rule C1.3 proves for the
    // audience, holding for the preview because it is the same query.
    account
        .record_campaign_consent(&NewCampaignConsent {
            address: "gone@cprev.test",
            source: ConsentSource::Import,
            source_ref: Some("newsletter-2026.csv"),
            statement: "Subscribed on our old shop in 2024",
            occurred_at: None,
        })
        .await
        .unwrap();
    assert!(
        is_not_found(
            account
                .preview_campaign(
                    &id,
                    &PreviewAs::Recipient("gone@cprev.test".into()),
                    &unsub()
                )
                .await
        ),
        "an import resurrected somebody the preview had refused"
    );

    // `AnyRecipient` picks from the same query, so it cannot land on them
    // either: the only mailable person is gone, and the answer says nobody.
    let any = account
        .preview_campaign(&id, &PreviewAs::AnyRecipient, &unsub())
        .await
        .unwrap();
    assert_eq!(
        any.against,
        PreviewAgainst::Fallbacks(FallbackReason::NobodyToMailYet)
    );
}

#[tokio::test]
async fn a_preview_against_nobody_prints_the_writers_fallbacks_and_says_so() {
    // Not a degraded mode: this is the copy every recipient with nothing
    // recorded receives, which on an audience built from web forms is most of
    // them. A writer who has only read the personalised preview has not read
    // the mail most people get.
    let store = common::test_store().await;
    let (account, _) = tenant(&store, "nobody").await;
    let id = campaign(&account, "Spring prices for {{first_name|you}}").await;

    // An empty audience does not make a preview impossible, and the reason is
    // reported rather than substituted.
    let empty = account
        .preview_campaign(&id, &PreviewAs::AnyRecipient, &unsub())
        .await
        .unwrap();
    assert_eq!(
        empty.against,
        PreviewAgainst::Fallbacks(FallbackReason::NobodyToMailYet)
    );

    // And with a real audience, asking for it anyway is a different answer.
    reachable(&account, "Jean Dupont", "jean@cprev.test", "FR").await;
    let asked = account
        .preview_campaign(&id, &PreviewAs::Fallbacks, &unsub())
        .await
        .unwrap();
    assert_eq!(
        asked.against,
        PreviewAgainst::Fallbacks(FallbackReason::Asked)
    );

    assert_eq!(asked.subject, "Spring prices for you");
    assert!(asked.html.contains("Hi there"), "{}", asked.html);
    assert!(asked.text.contains("your address"), "{}", asked.text);
    assert!(
        asked.fields.iter().all(|used| used.fell_back),
        "every field in a preview against nobody is the writer's own words"
    );
    assert!(
        !asked.html.contains("jean@cprev.test"),
        "asking for the fallback copy must not quietly borrow a real record"
    );
}
