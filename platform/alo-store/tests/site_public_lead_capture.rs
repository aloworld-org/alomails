//! The public capture door (`SitePublicStore::capture_conversation_lead`,
//! item S3.03d): from a **resolved** published site into CRM's lead seam,
//! with the `(tenant, owner)` pair read from the site's own row.
//!
//! The mandatory isolation case: two tenants' sites are two separate doors —
//! the same visitor raises one lead per tenant, and neither tenant can see
//! the other's — plus the seam's duplicate answer surviving the trip, a
//! refused field writing nothing (not even the seeded board), and proof that
//! the type that crossed the boundary carried no journey.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, CapturedLead, ConversationLead, DealFilter, PipelineSeed, PublishedSite,
    SitePublicStore, StageSeed, StoreError,
};

fn seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: vec![
            StageSeed {
                name: "Incoming".to_owned(),
                is_won: false,
                is_lost: false,
            },
            StageSeed {
                name: "Won".to_owned(),
                is_won: true,
                is_lost: false,
            },
            StageSeed {
                name: "Lost".to_owned(),
                is_won: false,
                is_lost: true,
            },
        ],
    }
}

fn visitor(email: &str) -> ConversationLead {
    ConversationLead {
        title: "Website enquiry — Studio".to_owned(),
        visitor_name: "Vera Visitor".to_owned(),
        visitor_email: email.to_owned(),
        company_name: String::new(),
        source: "studio.sites.test".to_owned(),
    }
}

/// A tenant with one live site, its account door for verification, and the
/// public door a serving process would hold.
struct Live {
    account: AccountStore,
    public: SitePublicStore,
    site: PublishedSite,
}

async fn live(tag: &str) -> Live {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store
        .create_tenant(&format!("lead-door-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let subdomain = format!(
        "{tag}-{}x",
        alo_store::SiteId::generate()
            .as_str()
            .to_lowercase()
            .replace('_', "-")
    );
    let site = account.create_site("Studio", &subdomain).await.unwrap();
    account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(6)
        .connect(&common::database_url())
        .await
        .unwrap();
    let public = SitePublicStore::new(pool, blobs);
    let site = public
        .resolve_published(&subdomain)
        .await
        .unwrap()
        .expect("the published site resolves");
    Live {
        account,
        public,
        site,
    }
}

fn created(result: CapturedLead) -> alo_store::CrmDealId {
    match result {
        CapturedLead::Created(id) => id,
        other => panic!("expected Created, got: {other:?}"),
    }
}

/// The capture lands in the serving site's own tenant, owned by the site's
/// creator — and in nobody else's: the same visitor on another tenant's site
/// is a separate lead, and each tenant sees exactly one.
#[tokio::test]
async fn one_visitor_on_two_tenants_sites_is_two_isolated_leads() {
    let a = live("tenant-a").await;
    let b = live("tenant-b").await;

    let deal_a = created(
        a.public
            .capture_conversation_lead(&a.site, &seed(), &visitor("vera@newcompany.example"))
            .await
            .unwrap(),
    );
    let deal_b = created(
        b.public
            .capture_conversation_lead(&b.site, &seed(), &visitor("vera@newcompany.example"))
            .await
            .unwrap(),
    );

    // Each tenant holds exactly its own card, owned by its own site creator.
    for (owned, deal, foreign) in [(&a, &deal_a, &deal_b), (&b, &deal_b, &deal_a)] {
        let deals = owned
            .account
            .crm_deals(&DealFilter::default())
            .await
            .unwrap();
        assert_eq!(deals.len(), 1);
        assert_eq!(deals[0].id.as_str(), deal.as_str());
        assert_eq!(deals[0].owner_user_id, owned.account.user().as_str());
        assert_eq!(deals[0].contact_email, "vera@newcompany.example");
        // The mandatory wrong-tenant proof: the other tenant's card does not
        // exist here, not even as a hidden row.
        assert!(owned.account.crm_deal(foreign).await.unwrap().is_none());
    }
}

/// CRM's duplicate answer crosses the door intact: the second conversation
/// from the same address is `AlreadyKnown`, and no twin card is raised.
#[tokio::test]
async fn a_known_address_answers_without_a_twin() {
    let a = live("known").await;
    let first = created(
        a.public
            .capture_conversation_lead(&a.site, &seed(), &visitor("vera@newcompany.example"))
            .await
            .unwrap(),
    );
    match a
        .public
        .capture_conversation_lead(&a.site, &seed(), &visitor("VERA@newcompany.example"))
        .await
        .unwrap()
    {
        CapturedLead::AlreadyKnown(deal) => assert_eq!(deal.as_str(), first.as_str()),
        other => panic!("expected AlreadyKnown, got: {other:?}"),
    }
    assert_eq!(
        a.account
            .crm_deals(&DealFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );
}

/// A field CRM refuses refuses the whole capture with CRM's own sentence,
/// and nothing is left behind — not a card, not even the seeded board.
#[tokio::test]
async fn a_refused_field_writes_nothing() {
    let a = live("refused").await;
    let out = a
        .public
        .capture_conversation_lead(&a.site, &seed(), &visitor("not-an-address"))
        .await;
    match out {
        Err(StoreError::Validation(detail)) => assert!(detail.contains("valid address")),
        other => panic!("expected Validation, got: {other:?}"),
    }
    assert!(
        a.account
            .crm_deals(&DealFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(a.account.crm_pipelines(false).await.unwrap().is_empty());
}

/// The compile-time privacy proof, stated as a test so it is read: the one
/// type that can cross this boundary has five fields, and none of them can
/// hold a transcript, a question, or a page view. A journey cannot be stored
/// through a type that cannot carry one.
#[test]
fn the_boundary_type_cannot_carry_a_journey() {
    let lead = ConversationLead {
        title: String::new(),
        visitor_name: String::new(),
        visitor_email: String::new(),
        company_name: String::new(),
        source: String::new(),
    };
    // Exhaustive construction: a new field on this type fails this test
    // until its privacy is argued where the field is added.
    let ConversationLead {
        title: _,
        visitor_name: _,
        visitor_email: _,
        company_name: _,
        source: _,
    } = lead;
}
