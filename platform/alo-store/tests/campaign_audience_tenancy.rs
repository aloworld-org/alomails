//! Tenancy and privacy proof for the campaign audience (C1.1, ADR 0044; Law 1:
//! isolation is tested, not assumed).
//!
//! This suite exists for one reason above the others. A campaign module's
//! failures do not look like crashes — they look like mail arriving at somebody
//! who never agreed to it, and that lands in a person's inbox rather than in a
//! log. So the assertions here are mostly about **who is absent**:
//!
//! - a neighbouring tenant's customers, deals and form submissions are
//!   unreachable, not merely filtered out;
//! - the per-user `contacts` address book is **never a source**, proved twice —
//!   once against the module's own SQL (a unit test inside
//!   `campaign_audience.rs`) and once here, at runtime, with a contact that
//!   really exists and really is readable by its owner;
//! - a string somebody typed into a free-text field is not a recipient just
//!   because a column called it an email.
//!
//! And one thing about who is present: a person held by three sources is **one**
//! row naming three, because ADR 0044's claim is that there is no list to be on
//! twice.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::campaign_audience::ADDRESS_SHAPE;
use alo_store::{
    AUDIENCE_PAGE_MAX, AccountStore, AudiencePage, AudienceSource, Contact, ContactField,
    ContactId, NewCustomer, NewDeal, PipelineSeed, SiteFormId, SiteId, StageSeed, Store,
    StoreError, normalise_address,
};

/// A tenant with one user, and the account door for them.
async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store.create_tenant(&format!("caud-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@caud.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

/// A customer with an invoice address and a country.
async fn customer(store: &AccountStore, name: &str, email: &str, country: &str) {
    store
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: country.to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
}

/// A deal carrying a contact name and whatever somebody typed as their address.
async fn deal(store: &AccountStore, title: &str, contact_name: &str, contact_email: &str) {
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
                title: title.to_owned(),
                contact_name: contact_name.to_owned(),
                contact_email: contact_email.to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

/// A published-site form, created once per tenant and reused by the submissions.
///
/// The subdomain carries a random tail because that namespace is **global** —
/// a fixed one would be taken by the previous run of this suite, which is a
/// failure of the test rather than of the audience.
async fn form(store: &AccountStore, tag: &str) -> (SiteId, SiteFormId) {
    let subdomain = format!(
        "caud-{tag}-{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase()
    );
    let site = store.create_site("Shop", &subdomain).await.unwrap();
    let form = store.create_site_form(&site, "Contact").await.unwrap();
    (site, form)
}

/// Somebody who filled in that form.
async fn submission(store: &AccountStore, site: &SiteId, form: &SiteFormId, name: &str, at: &str) {
    store
        .add_site_form_submission(site, form, name, at, "Do you deliver to Ghent?")
        .await
        .unwrap();
}

/// A contact in the acting user's **private** address book — the table no
/// campaign may read.
async fn private_contact(store: &AccountStore, display_name: &str, email: &str) -> ContactId {
    store
        .create_contact(&Contact {
            id: ContactId::new("placeholder"),
            display_name: display_name.to_owned(),
            first_name: None,
            last_name: None,
            emails: vec![ContactField {
                kind: Some("home".to_owned()),
                value: email.to_owned(),
            }],
            phones: Vec::new(),
            organization: None,
            job_title: None,
            notes: None,
        })
        .await
        .unwrap()
}

/// Every address in the audience, in order, read a page at a time so the paging
/// path is the one under test everywhere.
async fn addresses(store: &AccountStore) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    loop {
        let page = store
            .campaign_audience(&AudiencePage {
                after: out.last().cloned(),
                limit: 2,
            })
            .await
            .unwrap();
        if page.is_empty() {
            return out;
        }
        out.extend(page.into_iter().map(|m| m.address));
    }
}

#[tokio::test]
async fn the_audience_is_this_tenants_three_sources_and_a_neighbours_people_are_unreachable() {
    let store = common::test_store().await;
    let a = account(&store, "mine").await;
    let b = account(&store, "theirs").await;

    customer(&a, "Acme BV", "orders@acme.test", "BE").await;
    deal(&a, "Spring order", "Ann Dupont", "ann@lead.test").await;
    let (site_a, form_a) = form(&a, "mine").await;
    submission(&a, &site_a, &form_a, "Bo Visitor", "bo@visitor.test").await;

    // The neighbour seeds the same shapes, and one identical address — the
    // sharpest case, because a leak would look like a plausible extra row.
    customer(&b, "Rival NV", "orders@acme.test", "NL").await;
    deal(&b, "Their deal", "Their Lead", "ann@lead.test").await;
    let (site_b, form_b) = form(&b, "theirs").await;
    submission(&b, &site_b, &form_b, "Their Visitor", "cy@visitor.test").await;

    assert_eq!(
        addresses(&a).await,
        ["ann@lead.test", "bo@visitor.test", "orders@acme.test"]
    );
    assert_eq!(a.campaign_audience_size().await.unwrap(), 3);
    // The neighbour's own audience holds the neighbour's own people, and
    // `cy@visitor.test` never appeared above — which is the isolation, stated
    // from both sides.
    assert_eq!(
        addresses(&b).await,
        ["ann@lead.test", "cy@visitor.test", "orders@acme.test"]
    );
    assert_eq!(b.campaign_audience_size().await.unwrap(), 3);
}

#[tokio::test]
async fn the_per_user_address_book_is_never_a_source() {
    let store = common::test_store().await;
    let a = account(&store, "book").await;

    // One person the company knows, and one the *employee* knows.
    customer(&a, "Acme BV", "orders@acme.test", "BE").await;
    let private = private_contact(&a, "Dr Reynders", "surgery@doctor.test").await;

    // The contact really exists and really is readable by its owner: this
    // assertion is what stops the one below from passing for the wrong reason.
    let saved = a.contact(&private).await.unwrap().unwrap();
    assert_eq!(saved.emails[0].value, "surgery@doctor.test");

    // …and it is not somebody a campaign can reach. Not filtered out — never
    // asked for.
    assert_eq!(addresses(&a).await, ["orders@acme.test"]);
    assert_eq!(a.campaign_audience_size().await.unwrap(), 1);
}

#[tokio::test]
async fn a_person_three_sources_hold_is_one_row_naming_three() {
    let store = common::test_store().await;
    let a = account(&store, "one").await;

    // The same human, entered three times by three colleagues, in three
    // casings, with the whitespace a paste leaves behind.
    customer(&a, "Ann Dupont", "Ann.Dupont@Example.test", "BE").await;
    deal(&a, "Repeat order", "A. Dupont", " ANN.DUPONT@EXAMPLE.TEST ").await;
    let (site, form_id) = form(&a, "one").await;
    submission(&a, &site, &form_id, "ann", "ann.dupont@example.test").await;

    let people = a.campaign_audience(&AudiencePage::default()).await.unwrap();
    assert_eq!(
        people.len(),
        1,
        "one person, however many records: {people:?}"
    );
    assert_eq!(a.campaign_audience_size().await.unwrap(), 1);
    let ann = &people[0];
    assert_eq!(ann.address, "ann.dupont@example.test");
    assert_eq!(
        ann.sources,
        [
            AudienceSource::BillingCustomer,
            AudienceSource::CrmDeal,
            AudienceSource::SiteForm
        ]
    );
    // Billing's name and country win: the invoiced name is the one the tenant
    // is surest of, and it is the only source that has a country at all.
    assert_eq!(ann.name.as_deref(), Some("Ann Dupont"));
    assert_eq!(ann.country.as_deref(), Some("BE"));
    assert!(ann.first_seen_at <= ann.last_seen_at);
}

#[tokio::test]
async fn a_string_somebody_typed_is_not_a_recipient_just_because_a_column_called_it_an_email() {
    let store = common::test_store().await;
    let a = account(&store, "junk").await;

    // `crm_deals.contact_email` is free text with an empty default, so this is
    // what a real board looks like rather than a contrived case.
    for typed in [
        "",
        "   ",
        "n/a",
        "ask reception",
        "ann at example.test",
        "ann@localhost",
    ] {
        deal(&a, "Unqualified", "Somebody", typed).await;
    }
    assert_eq!(addresses(&a).await, Vec::<String>::new());
    assert_eq!(a.campaign_audience_size().await.unwrap(), 0);

    // One real address among them, and it is the only one that counts.
    deal(&a, "Qualified", "Ann", "ann@example.test").await;
    assert_eq!(addresses(&a).await, ["ann@example.test"]);
}

#[tokio::test]
async fn postgres_and_rust_agree_on_what_an_address_is() {
    // The shape is applied in SQL (so the count is right) and in Rust (so a
    // cursor can be judged). Two implementations of one rule drift unless
    // something holds them together; this is that thing.
    let store = common::test_store().await;
    let a = account(&store, "shape").await;
    let pool = sqlx::postgres::PgPool::connect(&common::database_url())
        .await
        .unwrap();

    for candidate in [
        "ann@example.test",
        "  Ann@Example.TEST  ",
        "ann.dupont+news@mail.example.test",
        "a@b.c",
        "",
        "   ",
        "n/a",
        "ask reception",
        "ann@localhost",
        "@example.test",
        "ann@",
        "ann@.test",
        "ann@example.",
        "ann@example..test",
        "ann example@x.test",
        "ann@ex ample.test",
        "ann@@example.test",
        "ann@example.test extra",
    ] {
        let in_sql: bool = sqlx::query_scalar("SELECT lower(btrim($1::text)) ~ $2::text")
            .bind(candidate)
            .bind(ADDRESS_SHAPE)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            in_sql,
            normalise_address(candidate).is_some(),
            "Postgres and Rust disagree about {candidate:?}"
        );
    }
    // …and the agreement is not vacuous: the account door reads the same rule.
    customer(&a, "Acme BV", "orders@acme.test", "BE").await;
    assert_eq!(a.campaign_audience_size().await.unwrap(), 1);
}

#[tokio::test]
async fn a_page_is_a_window_that_never_repeats_or_skips_a_person() {
    let store = common::test_store().await;
    let a = account(&store, "page").await;

    for tag in ["e", "a", "d", "b", "c"] {
        customer(&a, "Acme BV", &format!("{tag}@acme.test"), "BE").await;
    }

    let first = a
        .campaign_audience(&AudiencePage {
            after: None,
            limit: 2,
        })
        .await
        .unwrap();
    assert_eq!(
        first.iter().map(|m| m.address.as_str()).collect::<Vec<_>>(),
        ["a@acme.test", "b@acme.test"]
    );
    // A cursor echoed back from a screen in another casing lands where the
    // caller means, because Postgres folds it against the same collation that
    // produced the column.
    let next = a
        .campaign_audience(&AudiencePage {
            after: Some("  B@Acme.TEST ".to_owned()),
            limit: 2,
        })
        .await
        .unwrap();
    assert_eq!(
        next.iter().map(|m| m.address.as_str()).collect::<Vec<_>>(),
        ["c@acme.test", "d@acme.test"]
    );
    // Walked end to end: five people, each once, in order.
    assert_eq!(
        addresses(&a).await,
        [
            "a@acme.test",
            "b@acme.test",
            "c@acme.test",
            "d@acme.test",
            "e@acme.test"
        ]
    );
    assert_eq!(a.campaign_audience_size().await.unwrap(), 5);
}

#[tokio::test]
async fn a_page_size_and_a_cursor_that_are_not_answerable_are_refused_rather_than_guessed() {
    let store = common::test_store().await;
    let a = account(&store, "bad").await;

    for limit in [0, -1, AUDIENCE_PAGE_MAX + 1] {
        let err = a
            .campaign_audience(&AudiencePage { after: None, limit })
            .await
            .err()
            .unwrap_or_else(|| panic!("accepted a page of {limit}"));
        assert!(matches!(err, StoreError::Validation(ref m) if m.contains("between 1 and")));
    }
    // A cursor that is not an address would silently answer page one, which
    // reads as an audience that restarted rather than as the mistake it is.
    let err = a
        .campaign_audience(&AudiencePage {
            after: Some("not an address".to_owned()),
            limit: 10,
        })
        .await
        .err()
        .unwrap_or_else(|| panic!("accepted a cursor that is not an address"));
    assert!(matches!(err, StoreError::Validation(ref m) if m.contains("cursor")));
    // The edges of the allowed range are allowed.
    for limit in [1, AUDIENCE_PAGE_MAX] {
        assert!(
            a.campaign_audience(&AudiencePage { after: None, limit })
                .await
                .is_ok()
        );
    }
}

#[tokio::test]
async fn an_archived_customer_is_still_a_person_the_tenant_knows() {
    let store = common::test_store().await;
    let a = account(&store, "arch").await;

    let id = a
        .create_billing_customer(&NewCustomer {
            name: "Former BV".to_owned(),
            country: "BE".to_owned(),
            email: Some("orders@former.test".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    a.set_billing_customer_archived(&id, true).await.unwrap();

    // Archiving hides a row from billing's pickers. It does not say the person
    // asked us to stop — that is an unsubscribe, and C1.3 is where it belongs.
    // Answering a consent question with a bookkeeping one would be the bug.
    assert_eq!(addresses(&a).await, ["orders@former.test"]);
    assert_eq!(a.campaign_audience_size().await.unwrap(), 1);
}
