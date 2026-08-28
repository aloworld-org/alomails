//! CRM's public lead seam against a real database (ADR 0040 §2 and §4): what
//! one anonymous site conversation may write into a tenant's CRM — one lead,
//! landed where CRM's own defaults put it — and everything it may not.
//!
//! The email-shape rules are unit-tested inside `crm_lead_capture`; this suite
//! proves the seam: the seeded first board, the landing column, the duplicate
//! answers (open deal, colleague's domain, customer, free mail, closed
//! history), and — Law 1 — that no pair of tenants can reach each other
//! through the door.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, CapturedLead, ConversationLead, CrmLeadCapture, DealFilter, NewCustomer,
    PipelineSeed, StageMove, StageSeed, StoreError,
};
use sqlx::postgres::PgPoolOptions;

/// The board a first capture seeds, exactly as the CRM screen would seed it —
/// the strings are the caller's (the edge translates them), never the store's.
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

/// One conversation's worth of visitor: a name, an address, no journey.
fn visitor(email: &str) -> ConversationLead {
    ConversationLead {
        title: "Website conversation — Vera Visitor".to_owned(),
        visitor_name: "Vera Visitor".to_owned(),
        visitor_email: email.to_owned(),
        company_name: String::new(),
        source: "studio.alosites.test".to_owned(),
    }
}

/// A tenant, its one user's account door, and the anonymous lead door a
/// published site would open for that owner.
struct Owned {
    blobs: alo_store::BlobStore,
    account: AccountStore,
    pool: sqlx::PgPool,
}

async fn owned(tag: &str) -> Owned {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store
        .create_tenant(&format!("lead-seam-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&common::database_url())
        .await
        .unwrap();
    Owned {
        blobs,
        account,
        pool,
    }
}

impl Owned {
    fn door(&self) -> CrmLeadCapture {
        CrmLeadCapture::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.account.tenant().clone(),
            self.account.user().clone(),
        )
    }

    async fn open_deal_count(&self) -> usize {
        self.account
            .crm_deals(&DealFilter::default())
            .await
            .unwrap()
            .len()
    }
}

fn created(result: CapturedLead) -> alo_store::CrmDealId {
    match result {
        CapturedLead::Created(id) => id,
        other => panic!("expected Created, got: {other:?}"),
    }
}

#[tokio::test]
async fn a_conversation_raises_the_lead_on_a_seeded_first_board() {
    let owned = owned("first").await;
    let id = created(
        owned
            .door()
            .capture(&seed(), &visitor("vera@newcompany.example"))
            .await
            .unwrap(),
    );

    // The tenant that had never opened CRM now has the seeded board, once.
    let pipelines = owned.account.crm_pipelines(false).await.unwrap();
    assert_eq!(pipelines.len(), 1);
    assert_eq!(pipelines[0].name, "Sales");

    // The card is CRM's own record, in the first live column, carrying the
    // visitor's facts and the caller's words — and no value it invented.
    let deal = owned.account.crm_deal(&id).await.unwrap().unwrap();
    let stages = owned
        .account
        .crm_stages(&pipelines[0].id, false)
        .await
        .unwrap();
    assert_eq!(deal.stage_id.as_str(), stages[0].id.as_str());
    assert_eq!(stages[0].name, "Incoming");
    assert!(!stages[0].is_closed());
    assert_eq!(deal.title, "Website conversation — Vera Visitor");
    assert_eq!(deal.contact_name, "Vera Visitor");
    assert_eq!(deal.contact_email, "vera@newcompany.example");
    assert_eq!(deal.company_name, "");
    assert_eq!(deal.source, "studio.alosites.test");
    assert_eq!(deal.value_cents, 0);
    assert_eq!(deal.owner_user_id, owned.account.user().as_str());
    assert_eq!(deal.state(), alo_store::DealState::Open);
}

#[tokio::test]
async fn a_second_conversation_from_the_same_address_answers_the_open_deal() {
    let owned = owned("twice").await;
    let door = owned.door();
    let first = created(
        door.capture(&seed(), &visitor("vera@corp.example"))
            .await
            .unwrap(),
    );
    // Same address — and case must not mint a twin.
    let again = door
        .capture(&seed(), &visitor("VERA@CORP.example"))
        .await
        .unwrap();
    assert_eq!(again, CapturedLead::AlreadyKnown(first));
    assert_eq!(owned.open_deal_count().await, 1);
}

#[tokio::test]
async fn a_colleague_at_a_known_company_folds_into_the_open_deal() {
    let owned = owned("colleague").await;
    let door = owned.door();
    let first = created(
        door.capture(&seed(), &visitor("vera@acme.example"))
            .await
            .unwrap(),
    );
    let colleague = door
        .capture(&seed(), &visitor("victor@acme.example"))
        .await
        .unwrap();
    assert_eq!(colleague, CapturedLead::AlreadyKnown(first));
    assert_eq!(owned.open_deal_count().await, 1);
}

#[tokio::test]
async fn free_mail_addresses_never_fold_strangers_together() {
    let owned = owned("freemail").await;
    let door = owned.door();
    let first = created(
        door.capture(&seed(), &visitor("one.person@gmail.com"))
            .await
            .unwrap(),
    );
    let second = created(
        door.capture(&seed(), &visitor("another.person@gmail.com"))
            .await
            .unwrap(),
    );
    assert_ne!(first, second);
    assert_eq!(owned.open_deal_count().await, 2);
}

#[tokio::test]
async fn an_existing_customer_is_answered_not_duplicated() {
    let owned = owned("customer").await;
    owned
        .account
        .create_billing_customer(&NewCustomer {
            name: "Corp BV".to_owned(),
            country: "BE".to_owned(),
            email: Some("buyer@corp-client.example".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    let door = owned.door();
    // The exact address, and a colleague at the same (non-free-mail) domain.
    assert_eq!(
        door.capture(&seed(), &visitor("buyer@corp-client.example"))
            .await
            .unwrap(),
        CapturedLead::AlreadyCustomer
    );
    assert_eq!(
        door.capture(&seed(), &visitor("other@corp-client.example"))
            .await
            .unwrap(),
        CapturedLead::AlreadyCustomer
    );
    assert_eq!(owned.open_deal_count().await, 0);
}

#[tokio::test]
async fn a_closed_deal_does_not_block_tomorrows_lead() {
    let owned = owned("closed").await;
    let door = owned.door();
    let first = created(
        door.capture(&seed(), &visitor("vera@returning.example"))
            .await
            .unwrap(),
    );
    let pipelines = owned.account.crm_pipelines(false).await.unwrap();
    let stages = owned
        .account
        .crm_stages(&pipelines[0].id, false)
        .await
        .unwrap();
    let won = stages.iter().find(|s| s.is_won).unwrap();
    owned
        .account
        .move_crm_deal(&first, &StageMove::to(won.id.clone()))
        .await
        .unwrap();
    // History must not make tomorrow's lead a duplicate.
    let second = created(
        door.capture(&seed(), &visitor("vera@returning.example"))
            .await
            .unwrap(),
    );
    assert_ne!(first, second);
}

#[tokio::test]
async fn crm_refuses_a_blank_title_at_the_seam_too() {
    let owned = owned("title").await;
    let mut lead = visitor("vera@titleless.example");
    lead.title = "   ".to_owned();
    match owned.door().capture(&seed(), &lead).await {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains("title"),
                "expected the title rule, got {msg:?}"
            );
        }
        other => panic!("expected Validation, got: {other:?}"),
    }
    // A refused capture leaves nothing behind — not even the seeded board.
    assert!(owned.account.crm_pipelines(false).await.unwrap().is_empty());
}

#[tokio::test]
async fn no_pair_of_tenants_can_reach_each_other_through_the_door() {
    let a = owned("tenant-a").await;
    let b = owned("tenant-b").await;
    let id = created(
        a.door()
            .capture(&seed(), &visitor("vera@isolated.example"))
            .await
            .unwrap(),
    );

    // Tenant B cannot see A's lead, its board, or any echo of the capture.
    assert!(b.account.crm_deal(&id).await.unwrap().is_none());
    assert!(b.account.crm_pipelines(false).await.unwrap().is_empty());

    // The same visitor writing to tenant B raises B's own lead — A's open
    // deal must not make a stranger's enquiry a duplicate across tenants.
    let b_lead = created(
        b.door()
            .capture(&seed(), &visitor("vera@isolated.example"))
            .await
            .unwrap(),
    );
    assert_ne!(b_lead, id);
    assert_eq!(a.open_deal_count().await, 1);
    assert_eq!(b.open_deal_count().await, 1);

    // A door opened with a pair that does not hold — tenant B, A's user —
    // writes nothing anywhere, not even a seeded board.
    let forged = CrmLeadCapture::open(
        b.pool.clone(),
        b.blobs.clone(),
        b.account.tenant().clone(),
        a.account.user().clone(),
    );
    match forged
        .capture(&seed(), &visitor("intruder@elsewhere.example"))
        .await
    {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains("owner"),
                "expected the owner rule, got {msg:?}"
            );
        }
        other => panic!("expected Validation, got: {other:?}"),
    }
    assert_eq!(a.open_deal_count().await, 1);
    assert_eq!(b.open_deal_count().await, 1);
}
