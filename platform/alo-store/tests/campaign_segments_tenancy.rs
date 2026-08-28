//! Segments — the saved question, its count, and the people the count leaves
//! out (C1.4, ADR 0044; Law 1: isolation is tested, not assumed).
//!
//! The queue's definition of done for this module: *an item that touches who
//! may be mailed is not done without a test that proves who may not be.* A
//! segment is the first thing in alo Campaigns that a colleague points at a
//! group of people and presses send on, so the assertions here are about the
//! three ways that goes wrong:
//!
//! - **the number lies.** "1 240 recipients" that turns into 900 sends is not a
//!   rounding error, it is a consent bug nobody was shown. So the tally is
//!   asserted whole — mailable, and every excluded person accounted for by name
//!   and reason, summing exactly to the people the conditions selected.
//! - **the segment widens who may be mailed.** A segment is a `WHERE` over the
//!   audience, never a source of its own, so somebody who unsubscribed cannot
//!   reappear because a condition happened to select them.
//! - **a condition quietly means something else.** "Has not bought in ninety
//!   days" must include the person who never bought, exclude the person who
//!   bought last week, and include the person who bought four months ago —
//!   which needs an invoice older than this test run, so one is backdated
//!   directly in the database. It must also ignore a draft and a voided
//!   invoice: a document that was never issued, or was cancelled, is not a
//!   purchase.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, AudiencePage, BillingCustomerId, CampaignSegment, CampaignSegmentId, Contact,
    ContactField, ContactId, ExclusionReason, NewCampaignConsent, NewCampaignSegment, NewCustomer,
    NewDeal, NewInvoice, NewLine, NewSuppression, PipelineSeed, PurchaseCondition, PurchaseWindow,
    SegmentConditions, SegmentExclusion, SegmentTally, StageSeed, Store, StoreError,
    SuppressionReason, TenantId, TenantStore,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};

/// A tenant with one user: the account door for segments and the audience, the
/// tenant door for suppression (which has no logged-in colleague behind it).
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore) {
    let tenant: TenantId = store.create_tenant(&format!("cseg-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@cseg.test")).await.unwrap();
    let account = store.for_account(tenant, user);
    common::seed_default_chart(&account).await;
    (account, ts)
}

/// A direct pool, for the one piece of setup no API offers: an invoice issued
/// months ago. Issuing dates a document today, and a period condition that can
/// only be tested against today is a period condition that is not tested.
async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// A customer the tenant invoices, in a country.
async fn customer(
    store: &AccountStore,
    name: &str,
    email: &str,
    country: &str,
) -> BillingCustomerId {
    store
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: country.to_owned(),
            currency: "EUR".to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
}

/// A CRM deal contact — a person the tenant knows and has no country for.
async fn deal(store: &AccountStore, contact_name: &str, contact_email: &str) {
    let boards = store
        .crm_pipelines_or_seed(&PipelineSeed {
            name: "Sales".to_owned(),
            stages: vec![StageSeed {
                name: "New".to_owned(),
                is_won: false,
                is_lost: false,
            }],
        })
        .await
        .unwrap();
    let board = boards[0].id.clone();
    let stage = store.crm_stages(&board, false).await.unwrap()[0].id.clone();
    store
        .create_crm_deal(
            &board,
            &stage,
            &NewDeal {
                title: format!("Deal with {contact_name}"),
                contact_name: contact_name.to_owned(),
                contact_email: contact_email.to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

/// Somebody said yes, and here is what they said yes to.
async fn agreed(store: &AccountStore, address: &str) {
    store
        .record_campaign_consent(&NewCampaignConsent {
            address,
            source: alo_store::ConsentSource::Manual,
            source_ref: None,
            statement: "Ticked the newsletter box at the counter",
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// Somebody may never be mailed again.
async fn suppressed(store: &TenantStore, address: &str, reason: SuppressionReason) {
    store
        .suppress_campaign_address(&NewSuppression {
            address,
            reason,
            source_ref: None,
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// An issued invoice for a customer, dated today by the store.
async fn issued(store: &AccountStore, customer: &BillingCustomerId) -> alo_store::BillingInvoiceId {
    let id = store
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    store
        .set_billing_invoice_lines(
            &id,
            &[NewLine {
                description: "Coffee beans".to_owned(),
                unit: "kg".to_owned(),
                qty_milli: 2_000,
                unit_price_cents: 1_800,
                vat_rate_bp: 2_100,
            }],
        )
        .await
        .unwrap();
    store.issue_billing_invoice(&id).await.unwrap();
    id
}

/// Moves an already-issued invoice back in time. The only way to have bought
/// four months ago in a test that started four seconds ago.
async fn backdate(
    pool: &PgPool,
    account: &AccountStore,
    invoice: &alo_store::BillingInvoiceId,
    days_ago: i64,
) {
    let when = (OffsetDateTime::now_utc() - Duration::days(days_ago)).date();
    let updated =
        sqlx::query("UPDATE billing_invoices SET issue_date = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(account.tenant().as_str())
            .bind(invoice.as_str())
            .bind(when)
            .execute(pool)
            .await
            .unwrap();
    assert_eq!(updated.rows_affected(), 1, "the invoice was not backdated");
}

/// Everybody a segment selects, mailable or not, read a page at a time so the
/// first page — where a mis-bracketed `WHERE` would let somebody through — is
/// always exercised.
async fn members(store: &AccountStore, conditions: &SegmentConditions) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    loop {
        let page = store
            .campaign_segment_members(
                conditions,
                &AudiencePage {
                    after: out.last().cloned(),
                    limit: 2,
                },
            )
            .await
            .unwrap();
        if page.is_empty() {
            return out;
        }
        out.extend(page.into_iter().map(|m| m.address));
    }
}

/// Everybody a segment may actually mail, paged the same way.
async fn recipients(store: &AccountStore, conditions: &SegmentConditions) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    loop {
        let page = store
            .campaign_segment_recipients(
                conditions,
                &AudiencePage {
                    after: out.last().cloned(),
                    limit: 2,
                },
            )
            .await
            .unwrap();
        if page.is_empty() {
            return out;
        }
        out.extend(page.into_iter().map(|r| r.address));
    }
}

/// The conditions for "bought / has not bought, within a period".
fn purchased(condition: PurchaseCondition, within_days: Option<i32>) -> SegmentConditions {
    SegmentConditions {
        countries: Vec::new(),
        purchase: Some(PurchaseWindow {
            condition,
            within_days,
        }),
    }
}

/// Saves a segment under a name, returning it.
async fn save(store: &AccountStore, name: &str, conditions: SegmentConditions) -> CampaignSegment {
    store
        .create_campaign_segment(&NewCampaignSegment { name, conditions })
        .await
        .unwrap()
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::Validation(_))),
        "expected a validation error, got {result:?}"
    );
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::NotFound)),
        "expected NotFound, got {result:?}"
    );
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_count_and_every_person_it_leaves_out_are_both_readable() {
    // The item, stated as a test: "the count **and its exclusions** are both
    // readable; a number without them is not auditable." Five people the tenant
    // knows, one of whom may be mailed.
    let store = common::test_store().await;
    let (account, tenant) = tenant(&store, "tally").await;

    customer(&account, "Mailable", "yes@cseg.test", "BE").await;
    agreed(&account, "yes@cseg.test").await;

    customer(&account, "Never asked", "silent@cseg.test", "BE").await;

    customer(&account, "Left", "left@cseg.test", "BE").await;
    agreed(&account, "left@cseg.test").await;
    suppressed(&tenant, "left@cseg.test", SuppressionReason::Unsubscribe).await;

    customer(&account, "Gone", "dead@cseg.test", "BE").await;
    agreed(&account, "dead@cseg.test").await;
    suppressed(&tenant, "dead@cseg.test", SuppressionReason::HardBounce).await;

    // The sharpest one: no consent record AND a complaint. Suppression is the
    // stronger fact, so the tally must say "complained" rather than "never
    // agreed" — the first is a person a colleague cannot fix by asking nicely,
    // and reporting it as the second invites them to try.
    customer(&account, "Angry", "angry@cseg.test", "BE").await;
    suppressed(&tenant, "angry@cseg.test", SuppressionReason::Complaint).await;

    let everyone = SegmentConditions::default();
    let tally = account.campaign_segment_tally(&everyone).await.unwrap();

    assert_eq!(tally.mailable, 1, "exactly one person may be mailed");
    assert_eq!(
        tally.excluded,
        [
            SegmentExclusion {
                reason: ExclusionReason::NoConsent,
                people: 1
            },
            SegmentExclusion {
                reason: ExclusionReason::Suppressed(SuppressionReason::Unsubscribe),
                people: 1
            },
            SegmentExclusion {
                reason: ExclusionReason::Suppressed(SuppressionReason::HardBounce),
                people: 1
            },
            SegmentExclusion {
                reason: ExclusionReason::Suppressed(SuppressionReason::Complaint),
                people: 1
            },
        ],
        "every excluded person must be accounted for, by reason"
    );
    // The arithmetic that makes the number auditable: nobody is counted twice,
    // and nobody vanishes between the parts and the whole.
    assert_eq!(tally.matched(), 5);
    assert_eq!(
        tally.matched() - tally.mailable,
        tally.excluded.iter().map(|e| e.people).sum::<i64>()
    );

    // And the list agrees with the number, person by person: the same
    // precedence, read off a member rather than counted in SQL.
    assert_eq!(
        members(&account, &everyone).await,
        [
            "angry@cseg.test",
            "dead@cseg.test",
            "left@cseg.test",
            "silent@cseg.test",
            "yes@cseg.test"
        ]
    );
    let page = account
        .campaign_segment_members(&everyone, &AudiencePage::default())
        .await
        .unwrap();
    let named: Vec<(String, Option<ExclusionReason>)> = page
        .iter()
        .map(|m| (m.address.clone(), ExclusionReason::for_member(m)))
        .collect();
    assert_eq!(
        named,
        [
            (
                "angry@cseg.test".to_owned(),
                Some(ExclusionReason::Suppressed(SuppressionReason::Complaint))
            ),
            (
                "dead@cseg.test".to_owned(),
                Some(ExclusionReason::Suppressed(SuppressionReason::HardBounce))
            ),
            (
                "left@cseg.test".to_owned(),
                Some(ExclusionReason::Suppressed(SuppressionReason::Unsubscribe))
            ),
            (
                "silent@cseg.test".to_owned(),
                Some(ExclusionReason::NoConsent)
            ),
            ("yes@cseg.test".to_owned(), None),
        ]
    );

    // The whole point of the exclusions being visible: the send itself reaches
    // exactly the one person the tally promised.
    assert_eq!(recipients(&account, &everyone).await, ["yes@cseg.test"]);
}

#[tokio::test]
async fn a_segment_cannot_reach_somebody_the_audience_would_not() {
    // A segment is a WHERE over the audience, never a source of its own — so a
    // condition that happens to select a suppressed person still cannot mail
    // them, and an import that re-states their consent changes nothing (C1.3).
    let store = common::test_store().await;
    let (account, tenant) = tenant(&store, "narrow").await;

    let bought = customer(&account, "Regular", "regular@cseg.test", "BE").await;
    agreed(&account, "regular@cseg.test").await;
    issued(&account, &bought).await;

    let quit = customer(&account, "Quit", "quit@cseg.test", "BE").await;
    agreed(&account, "quit@cseg.test").await;
    issued(&account, &quit).await;
    suppressed(&tenant, "quit@cseg.test", SuppressionReason::Unsubscribe).await;

    let recent_buyers = SegmentConditions {
        countries: vec!["BE".to_owned()],
        purchase: Some(PurchaseWindow {
            condition: PurchaseCondition::Bought,
            within_days: Some(30),
        }),
    };

    // Both bought; only one may be mailed, and the other is named with why.
    assert_eq!(
        members(&account, &recent_buyers).await,
        ["quit@cseg.test", "regular@cseg.test"]
    );
    assert_eq!(
        recipients(&account, &recent_buyers).await,
        ["regular@cseg.test"]
    );
    let tally = account
        .campaign_segment_tally(&recent_buyers)
        .await
        .unwrap();
    assert_eq!(tally.mailable, 1);
    assert_eq!(
        tally.excluded,
        [SegmentExclusion {
            reason: ExclusionReason::Suppressed(SuppressionReason::Unsubscribe),
            people: 1
        }]
    );

    // An import that swears they agreed this morning. It is kept as evidence
    // that it tried, and it grants nothing.
    account
        .record_campaign_consent(&NewCampaignConsent {
            address: " QUIT@Cseg.TEST ",
            source: alo_store::ConsentSource::Import,
            source_ref: Some("newsletter-2026.csv"),
            statement: "Subscribed on our old shop in 2024",
            occurred_at: None,
        })
        .await
        .unwrap();
    assert_eq!(
        recipients(&account, &recent_buyers).await,
        ["regular@cseg.test"],
        "an import resurrected somebody a segment had excluded"
    );
    let after = account
        .campaign_segment_tally(&recent_buyers)
        .await
        .unwrap();
    assert_eq!(after, tally, "the tally moved on a re-import");
}

#[tokio::test]
async fn a_period_means_what_a_colleague_reading_it_would_think() {
    // "Has not bought in ninety days" has to include the person who never
    // bought, the person whose invoice is four months old, the person whose
    // invoice was never issued and the person whose invoice was cancelled —
    // and exclude only the person who actually bought last week.
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "period").await;
    let pool = pool().await;

    let recent = customer(&account, "Recent", "recent@cseg.test", "BE").await;
    agreed(&account, "recent@cseg.test").await;
    issued(&account, &recent).await;

    let lapsed = customer(&account, "Lapsed", "lapsed@cseg.test", "BE").await;
    agreed(&account, "lapsed@cseg.test").await;
    let old = issued(&account, &lapsed).await;
    backdate(&pool, &account, &old, 120).await;

    let never = customer(&account, "Never", "never@cseg.test", "BE").await;
    agreed(&account, "never@cseg.test").await;
    // A draft is an intention somebody may still delete, not a purchase.
    account
        .create_billing_invoice(&NewInvoice::for_customer(never.clone()))
        .await
        .unwrap();

    let cancelled = customer(&account, "Cancelled", "void@cseg.test", "BE").await;
    agreed(&account, "void@cseg.test").await;
    let voided = issued(&account, &cancelled).await;
    account.void_billing_invoice(&voided).await.unwrap();

    assert_eq!(
        recipients(&account, &purchased(PurchaseCondition::Bought, Some(90))).await,
        ["recent@cseg.test"],
        "a draft, a void invoice or a four-month-old one is not a recent purchase"
    );
    assert_eq!(
        recipients(&account, &purchased(PurchaseCondition::NotBought, Some(90))).await,
        ["lapsed@cseg.test", "never@cseg.test", "void@cseg.test"],
        "somebody who never bought must be in 'has not bought in ninety days'"
    );
    // Widen the period and the lapsed customer comes back — the boundary is the
    // date, not the existence of an invoice.
    assert_eq!(
        recipients(&account, &purchased(PurchaseCondition::Bought, Some(180))).await,
        ["lapsed@cseg.test", "recent@cseg.test"]
    );
    // "Ever", which is `None` rather than a period of 36 500 days.
    assert_eq!(
        recipients(&account, &purchased(PurchaseCondition::Bought, None)).await,
        ["lapsed@cseg.test", "recent@cseg.test"]
    );
    assert_eq!(
        recipients(&account, &purchased(PurchaseCondition::NotBought, None)).await,
        ["never@cseg.test", "void@cseg.test"],
        "'has never bought' must not be emptied by a NULL in the purchases subquery"
    );
}

#[tokio::test]
async fn a_country_segment_excludes_the_people_it_cannot_place() {
    // Only a billing customer carries a country. A deal contact and a form
    // submitter have none, and "is in Belgium" must not quietly mean "is in
    // Belgium, or we have no idea" — that is how a Dutch-language offer reaches
    // the wrong country.
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "country").await;

    customer(&account, "Belgian", "be@cseg.test", "BE").await;
    agreed(&account, "be@cseg.test").await;
    customer(&account, "Dutch", "nl@cseg.test", "NL").await;
    agreed(&account, "nl@cseg.test").await;
    deal(&account, "Unplaced", "somewhere@cseg.test").await;
    agreed(&account, "somewhere@cseg.test").await;

    let belgians = SegmentConditions {
        countries: vec!["be".to_owned()],
        purchase: None,
    };
    assert_eq!(
        recipients(&account, &belgians).await,
        ["be@cseg.test"],
        "a country nobody stated is not a match, and lowercase 'be' is still Belgium"
    );
    let benelux = SegmentConditions {
        countries: vec!["NL".to_owned(), "BE".to_owned(), "be".to_owned()],
        purchase: None,
    };
    assert_eq!(
        recipients(&account, &benelux).await,
        ["be@cseg.test", "nl@cseg.test"]
    );
    // An empty list is the absence of the condition, not a filter that matches
    // nobody — the difference between "everyone" and "no one" on a screen.
    assert_eq!(
        recipients(&account, &SegmentConditions::default()).await,
        ["be@cseg.test", "nl@cseg.test", "somewhere@cseg.test"]
    );
}

#[tokio::test]
async fn a_segment_stores_the_question_rather_than_the_people() {
    // ADR 0044: "there is nothing to sync, because there is no list." A saved
    // segment holds no membership, so somebody who consents after it was saved
    // is in it, and somebody who unsubscribes after it was saved is out —
    // without anybody refreshing anything.
    let store = common::test_store().await;
    let (account, tenant) = tenant(&store, "live").await;

    customer(&account, "First", "first@cseg.test", "BE").await;
    agreed(&account, "first@cseg.test").await;

    let saved = save(
        &account,
        "  Belgian customers  ",
        SegmentConditions {
            countries: vec!["BE".to_owned()],
            purchase: None,
        },
    )
    .await;
    assert_eq!(saved.name, "Belgian customers", "the name is trimmed");
    assert_eq!(saved.created_by, *account.user());
    assert_eq!(
        account
            .campaign_segment_tally(&saved.conditions)
            .await
            .unwrap(),
        SegmentTally {
            mailable: 1,
            excluded: Vec::new()
        }
    );

    customer(&account, "Second", "second@cseg.test", "BE").await;
    agreed(&account, "second@cseg.test").await;
    suppressed(&tenant, "first@cseg.test", SuppressionReason::Unsubscribe).await;

    // The stored row has not been touched, and the answer has changed anyway.
    let reread = account
        .campaign_segment(&saved.id)
        .await
        .unwrap()
        .expect("the segment is still there");
    assert_eq!(reread, saved);
    assert_eq!(
        account
            .campaign_segment_tally(&reread.conditions)
            .await
            .unwrap(),
        SegmentTally {
            mailable: 1,
            excluded: vec![SegmentExclusion {
                reason: ExclusionReason::Suppressed(SuppressionReason::Unsubscribe),
                people: 1,
            }],
        }
    );
    assert_eq!(
        recipients(&account, &reread.conditions).await,
        ["second@cseg.test"]
    );
}

#[tokio::test]
async fn a_saved_segment_can_be_renamed_rewritten_and_forgotten() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "crud").await;

    customer(&account, "Belgian", "be@cseg.test", "BE").await;
    agreed(&account, "be@cseg.test").await;
    customer(&account, "Dutch", "nl@cseg.test", "NL").await;
    agreed(&account, "nl@cseg.test").await;

    let belgians = save(
        &account,
        "Belgians",
        SegmentConditions {
            countries: vec!["BE".to_owned()],
            purchase: None,
        },
    )
    .await;
    let lapsed = save(
        &account,
        "Lapsed customers",
        purchased(PurchaseCondition::NotBought, Some(90)),
    )
    .await;
    // Listed by name, folded — so the order is the one a screen shows.
    assert_eq!(
        account
            .campaign_segments(50)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        ["Belgians", "Lapsed customers"]
    );

    // A whole-record rewrite: the conditions change together with the name, so
    // "Belgians" cannot half-become "Benelux".
    let widened = account
        .update_campaign_segment(
            &belgians.id,
            &NewCampaignSegment {
                name: "Benelux",
                conditions: SegmentConditions {
                    countries: vec!["NL".to_owned(), "BE".to_owned()],
                    purchase: None,
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(widened.id, belgians.id);
    assert_eq!(widened.conditions.countries, ["BE", "NL"]);
    assert_eq!(widened.created_at, belgians.created_at);
    assert_eq!(
        recipients(&account, &widened.conditions).await,
        ["be@cseg.test", "nl@cseg.test"]
    );

    // Forgetting a question does not forget the evidence: the consent records
    // it read are untouched, and so is the audience.
    account.delete_campaign_segment(&lapsed.id).await.unwrap();
    assert!(
        account
            .campaign_segment(&lapsed.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(account.delete_campaign_segment(&lapsed.id).await);
    assert_eq!(
        account
            .campaign_segment_tally(&SegmentConditions::default())
            .await
            .unwrap()
            .mailable,
        2
    );
}

#[tokio::test]
async fn a_segment_that_would_mean_nothing_is_refused_rather_than_saved() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "refuse").await;

    let good = SegmentConditions {
        countries: vec!["BE".to_owned()],
        purchase: None,
    };
    assert_validation(
        account
            .create_campaign_segment(&NewCampaignSegment {
                name: "   ",
                conditions: good.clone(),
            })
            .await,
    );
    assert_validation(
        account
            .create_campaign_segment(&NewCampaignSegment {
                name: "Belgium?",
                conditions: SegmentConditions {
                    countries: vec!["belgium".to_owned()],
                    purchase: None,
                },
            })
            .await,
    );
    assert_validation(
        account
            .create_campaign_segment(&NewCampaignSegment {
                name: "Yesterday",
                conditions: purchased(PurchaseCondition::Bought, Some(0)),
            })
            .await,
    );
    // A condition that cannot be validated cannot be counted either — the
    // screen must refuse it while it is being typed, not on save.
    assert_validation(
        account
            .campaign_segment_tally(&SegmentConditions {
                countries: vec!["belgium".to_owned()],
                purchase: None,
            })
            .await,
    );

    let saved = save(&account, "Belgians", good.clone()).await;
    // Two questions with one name is a colleague about to send the wrong one.
    let clash = account
        .create_campaign_segment(&NewCampaignSegment {
            name: "  belgians ",
            conditions: good.clone(),
        })
        .await;
    assert!(
        matches!(clash, Err(StoreError::Conflict(_))),
        "a duplicate name was accepted: {clash:?}"
    );
    assert_eq!(
        account.campaign_segments(50).await.unwrap(),
        [saved],
        "a refusal must leave nothing half-written behind"
    );
    assert_validation(account.campaign_segments(0).await);
    assert_validation(
        account
            .campaign_segment_members(
                &good,
                &AudiencePage {
                    after: Some("not an address".to_owned()),
                    limit: 10,
                },
            )
            .await,
    );
}

#[tokio::test]
async fn a_neighbours_customers_invoices_and_segments_are_all_unreachable() {
    // The mandatory wrong-tenant test, and sharpened the way C1.1–C1.3 were:
    // both tenants hold the SAME address, so a leak has to show up as a named
    // extra person rather than as a count that looks plausible.
    let store = common::test_store().await;
    let (ours, _our_tenant) = tenant(&store, "ours").await;
    let (theirs, _their_tenant) = tenant(&store, "theirs").await;

    for account in [&ours, &theirs] {
        customer(account, "Acme", "orders@acme.test", "BE").await;
        agreed(account, "orders@acme.test").await;
    }
    // Only the neighbour has actually sold anything to them.
    let their_customer = theirs
        .billing_customers(false)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("the neighbour's customer")
        .id;
    issued(&theirs, &their_customer).await;

    let buyers = purchased(PurchaseCondition::Bought, None);
    assert_eq!(
        recipients(&ours, &buyers).await,
        Vec::<String>::new(),
        "a neighbour's invoice made our address look like a customer"
    );
    assert_eq!(recipients(&theirs, &buyers).await, ["orders@acme.test"]);
    // And from both sides, so a leak in either direction is named.
    assert_eq!(
        ours.campaign_segment_tally(&buyers)
            .await
            .unwrap()
            .matched(),
        0
    );
    assert_eq!(
        theirs
            .campaign_segment_tally(&buyers)
            .await
            .unwrap()
            .mailable,
        1
    );

    // A saved segment is theirs alone: not readable, not writable, not
    // deletable from here, and absent from our list.
    let their_segment = save(&theirs, "Their buyers", buyers.clone()).await;
    assert!(
        ours.campaign_segment(&their_segment.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        ours.update_campaign_segment(
            &their_segment.id,
            &NewCampaignSegment {
                name: "Stolen",
                conditions: SegmentConditions::default(),
            },
        )
        .await,
    );
    assert_not_found(ours.delete_campaign_segment(&their_segment.id).await);
    assert!(ours.campaign_segments(50).await.unwrap().is_empty());
    // The neighbour's segment survived every one of those attempts unchanged.
    assert_eq!(
        theirs.campaign_segment(&their_segment.id).await.unwrap(),
        Some(their_segment)
    );

    // A segment id invented from nothing is a NotFound, never an oracle.
    assert!(
        ours.campaign_segment(&CampaignSegmentId::new("no-such-segment"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn the_per_user_address_book_is_never_a_source_of_a_segment() {
    // The promise C1.1 made, carried into the place it is most likely to be
    // broken: a segment is exactly where somebody eventually asks for "and my
    // own contacts too". Proved at runtime, with a contact that really exists
    // and really is readable by its owner — without the read-back this test
    // would pass just as happily if `create_contact` had silently failed.
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "private").await;

    customer(&account, "Acme", "orders@acme.test", "BE").await;
    agreed(&account, "orders@acme.test").await;

    let contact = account
        .create_contact(&Contact {
            id: ContactId::new("placeholder"),
            display_name: "Dr Reynders".to_owned(),
            first_name: None,
            last_name: None,
            emails: vec![ContactField {
                kind: Some("home".to_owned()),
                value: "surgery@doctor.test".to_owned(),
            }],
            phones: Vec::new(),
            organization: None,
            job_title: None,
            notes: None,
        })
        .await
        .unwrap();
    let stored = account
        .contact(&contact)
        .await
        .unwrap()
        .expect("the private contact really is there");
    assert_eq!(stored.emails[0].value, "surgery@doctor.test");

    for conditions in [
        SegmentConditions::default(),
        SegmentConditions {
            countries: vec!["BE".to_owned()],
            purchase: None,
        },
        purchased(PurchaseCondition::NotBought, None),
    ] {
        assert_eq!(
            members(&account, &conditions).await,
            ["orders@acme.test"],
            "a segment reached the acting user's private address book"
        );
        assert_eq!(
            account
                .campaign_segment_tally(&conditions)
                .await
                .unwrap()
                .matched(),
            1
        );
    }
}
