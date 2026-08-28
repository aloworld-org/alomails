//! Tenancy and behaviour proof for the won-deal handoff (B2.08, Law 1:
//! isolation is tested, not assumed).
//!
//! The handoff is the one place CRM writes into billing, so this suite holds it
//! to four things against the real database:
//!
//! - **A neighbour's deal is a clean `NotFound`** — no document is raised, and
//!   nothing of theirs is read on the way to finding that out.
//! - **A lead becomes exactly one customer**, created from the deal's own
//!   fields and linked back onto it, so the second document bills the same
//!   company rather than a twin of it.
//! - **The draft mirrors the deal**: the deal's currency, one line at its
//!   value, and totals the store computed — never the client, never a float.
//! - **A lost deal raises nothing**, and neither does a deal whose company
//!   nobody has named.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_invoices::InvoiceStatus;
use alo_store::billing_quotes::QuoteStatus;
use alo_store::{
    AccountStore, CrmDealId, CrmPipelineId, CrmStageId, DealHandoff, NewCustomer, NewDeal,
    PipelineSeed, StageMove, StageSeed, Store, StoreError, TenantId,
};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Validation({rule:?}), got: {other:?}"),
    }
}

async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("crmh-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crmh.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

fn stage_seed(name: &str, is_won: bool, is_lost: bool) -> StageSeed {
    StageSeed {
        name: name.to_owned(),
        is_won,
        is_lost,
    }
}

fn sales_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: vec![
            stage_seed("New", false, false),
            stage_seed("Won", true, false),
            stage_seed("Lost", false, true),
        ],
    }
}

async fn seeded_board(store: &AccountStore) -> (CrmPipelineId, Vec<CrmStageId>) {
    let boards = store.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let board = boards[0].id.clone();
    let stages = store
        .crm_stages(&board, false)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    (board, stages)
}

/// A won lead worth 2 500,00 EUR with a company and a contact address.
async fn won_lead(store: &AccountStore, title: &str, value_cents: i64) -> CrmDealId {
    let (board, stages) = seeded_board(store).await;
    let id = store
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: title.to_owned(),
                company_name: "Acme GmbH".to_owned(),
                contact_name: "Ada".to_owned(),
                contact_email: "ada@acme.example".to_owned(),
                value_cents,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    store
        .move_crm_deal(&id, &StageMove::to(stages[1].clone()))
        .await
        .unwrap();
    id
}

fn german(vat_rate_bp: i32) -> DealHandoff {
    DealHandoff {
        vat_rate_bp: Some(vat_rate_bp),
        country: "DE".to_owned(),
    }
}

#[tokio::test]
async fn a_won_lead_becomes_one_customer_and_a_draft_that_mirrors_the_deal() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "lead").await;
    let deal = won_lead(&a, "Renewal — Acme GmbH", 250_000).await;

    // ---- the invoice: a draft, in the deal's currency, one line ----------
    let invoice = a.crm_deal_invoice(&deal, &german(1900)).await.unwrap();
    let doc = a.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(doc.invoice.status, InvoiceStatus::Draft);
    assert!(
        doc.invoice.number.is_none(),
        "a draft consumes no number from the gapless sequence"
    );
    assert_eq!(doc.invoice.currency, "EUR");
    assert_eq!(doc.lines.len(), 1);
    assert_eq!(doc.lines[0].description, "Renewal — Acme GmbH");
    assert_eq!(doc.lines[0].qty_milli, 1_000);
    assert_eq!(doc.lines[0].unit_price_cents, 250_000);
    assert_eq!(doc.lines[0].vat_rate_bp, 1900);
    // The totals are the store's, computed from the line: 2 500 at 19 %.
    assert_eq!(doc.totals.net_cents, 250_000);
    assert_eq!(doc.totals.vat_cents, 47_500);
    assert_eq!(doc.totals.gross_cents, 297_500);

    // ---- the customer: created from the lead, and linked back to it ------
    let linked = a.crm_deal(&deal).await.unwrap().unwrap();
    let customer_id = linked
        .customer_id
        .clone()
        .expect("the lead is now a customer of the tenant");
    assert_eq!(customer_id, doc.invoice.customer_id);
    let customer = a.billing_customer(&customer_id).await.unwrap().unwrap();
    assert_eq!(
        customer.name, "Acme GmbH",
        "the company, not the deal title"
    );
    assert_eq!(customer.country, "DE");
    assert_eq!(customer.email.as_deref(), Some("ada@acme.example"));
    assert_eq!(customer.currency, "EUR");
    assert!(customer.vat_id.is_none(), "nothing is invented");

    // ---- a second document bills the SAME customer, not a twin ----------
    let quote = a.crm_deal_quote(&deal, &german(1900)).await.unwrap();
    let offer = a.billing_quote(&quote).await.unwrap().unwrap();
    assert_eq!(offer.quote.status, QuoteStatus::Draft);
    assert_eq!(offer.quote.customer_id, customer_id);
    assert_eq!(offer.totals.gross_cents, 297_500);
    assert_eq!(
        a.billing_customers(true).await.unwrap().len(),
        1,
        "one company, one customer row"
    );
}

#[tokio::test]
async fn a_deal_that_already_names_a_customer_bills_that_one() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "known").await;
    let customer = a
        .create_billing_customer(&NewCustomer {
            name: "Beta BV".to_owned(),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (board, stages) = seeded_board(&a).await;
    let deal = a
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: "Support renewal".to_owned(),
                customer_id: Some(customer.clone()),
                company_name: "Beta BV".to_owned(),
                value_cents: 120_000,
                currency: "USD".to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // No country is needed: nothing is being created.
    let invoice = a
        .crm_deal_invoice(
            &deal,
            &DealHandoff {
                vat_rate_bp: Some(2100),
                country: String::new(),
            },
        )
        .await
        .unwrap();
    let doc = a.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(doc.invoice.customer_id, customer);
    assert_eq!(
        doc.invoice.currency, "USD",
        "the deal's currency, not the customer's default"
    );
    assert_eq!(
        a.billing_customers(true).await.unwrap().len(),
        1,
        "no second customer was invented"
    );

    // An archived customer is a refusal that names the rule rather than a
    // document raised for a company the tenant has retired.
    a.set_billing_customer_archived(&customer, true)
        .await
        .unwrap();
    assert_invalid(
        a.crm_deal_invoice(
            &deal,
            &DealHandoff {
                vat_rate_bp: Some(2100),
                country: String::new(),
            },
        )
        .await,
        "archived",
    );
}

#[tokio::test]
async fn a_deal_worth_nothing_raises_an_empty_draft() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "unpriced").await;
    let deal = won_lead(&a, "Scoping — Acme GmbH", 0).await;
    // No VAT rate is stated, and none is needed: there is no line to rate.
    let quote = a
        .crm_deal_quote(
            &deal,
            &DealHandoff {
                vat_rate_bp: None,
                country: "DE".to_owned(),
            },
        )
        .await
        .unwrap();
    let offer = a.billing_quote(&quote).await.unwrap().unwrap();
    assert!(offer.lines.is_empty());
    assert_eq!(offer.totals.gross_cents, 0);
    assert!(
        a.crm_deal(&deal)
            .await
            .unwrap()
            .unwrap()
            .customer_id
            .is_some()
    );
}

#[tokio::test]
async fn what_the_handoff_refuses() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "refuse").await;
    let (board, stages) = seeded_board(&a).await;

    // ---- a priced deal without a VAT rate --------------------------------
    let priced = won_lead(&a, "Renewal — Acme GmbH", 250_000).await;
    assert_invalid(
        a.crm_deal_invoice(
            &priced,
            &DealHandoff {
                vat_rate_bp: None,
                country: "DE".to_owned(),
            },
        )
        .await,
        "VAT rate",
    );
    assert!(
        a.crm_deal(&priced)
            .await
            .unwrap()
            .unwrap()
            .customer_id
            .is_none(),
        "a refusal creates no customer either — the rate is checked first"
    );

    // ---- a lead nobody has named a company for ---------------------------
    let nameless = a
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: "Someone at the conference".to_owned(),
                value_cents: 10_000,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_invalid(
        a.crm_deal_quote(&nameless, &german(2100)).await,
        "names no company",
    );

    // ---- a lead whose country nobody stated ------------------------------
    assert_invalid(
        a.crm_deal_quote(
            &priced,
            &DealHandoff {
                vat_rate_bp: Some(2100),
                country: String::new(),
            },
        )
        .await,
        "two-letter",
    );

    // ---- a lost deal ------------------------------------------------------
    a.move_crm_deal(
        &priced,
        &StageMove {
            stage_id: stages[2].clone(),
            position: None,
            lost_reason: Some("Price".to_owned()),
        },
    )
    .await
    .unwrap();
    assert_invalid(a.crm_deal_invoice(&priced, &german(2100)).await, "was lost");
    assert_invalid(a.crm_deal_quote(&priced, &german(2100)).await, "was lost");
    // Reopening it makes it billable again — a deal is our own private record.
    a.move_crm_deal(&priced, &StageMove::to(stages[0].clone()))
        .await
        .unwrap();
    assert!(a.crm_deal_quote(&priced, &german(2100)).await.is_ok());

    // ---- a deal worth more than one line may carry ------------------------
    let huge = a
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: "Enterprise programme".to_owned(),
                company_name: "Gamma SA".to_owned(),
                value_cents: 1_000_000_001,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_invalid(
        a.crm_deal_invoice(&huge, &german(2100)).await,
        "split it across lines",
    );
}

#[tokio::test]
async fn a_neighbours_deal_raises_nothing_at_all() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "mine").await;
    let (b, _) = tenant_with_user(&store, "theirs").await;
    let theirs = won_lead(&b, "Renewal — Acme GmbH", 250_000).await;

    assert_not_found(a.crm_deal_invoice(&theirs, &german(2100)).await);
    assert_not_found(a.crm_deal_quote(&theirs, &german(2100)).await);
    assert_not_found(
        a.crm_deal_quote(&CrmDealId::new("dea_nope"), &german(2100))
            .await,
    );

    // Nothing was written on either side of the wall.
    assert!(a.billing_customers(true).await.unwrap().is_empty());
    assert!(a.billing_invoices(None).await.unwrap().is_empty());
    assert!(a.billing_quotes(None).await.unwrap().is_empty());
    assert!(
        b.crm_deal(&theirs)
            .await
            .unwrap()
            .unwrap()
            .customer_id
            .is_none(),
        "their deal is untouched"
    );
    assert!(b.billing_customers(true).await.unwrap().is_empty());
}
