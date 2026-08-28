//! Tenancy and arithmetic proof for the CRM pipeline report (B2.08, Law 1:
//! isolation is tested, not assumed).
//!
//! Two things it has to get right, and both are proved against the real
//! database rather than argued about:
//!
//! - **A neighbour's board is a clean `NotFound`**, never an empty report —
//!   and a neighbour's deals never appear in, or shift a cent of, our own
//!   figures even when both tenants seed a board with the same column names.
//! - **The figures are the ones a person would compute by hand**: value by
//!   stage over the open board, won and lost over the stated period judged on
//!   `closed_at`, one group per currency and no conversion between them.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::crm_report::PipelineTally;
use alo_store::{
    AccountStore, CrmPipelineId, CrmStageId, NewDeal, PipelineSeed, StageMove, StageSeed, Store,
    StoreError, TenantId,
};
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

async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("crmr-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crmr.test"))
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
            stage_seed("Qualified", false, false),
            stage_seed("Proposal", false, false),
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

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// A priced deal in one currency.
fn priced(title: &str, value_cents: i64, currency: &str) -> NewDeal {
    NewDeal {
        title: title.to_owned(),
        value_cents,
        currency: currency.to_owned(),
        ..Default::default()
    }
}

/// The period every assertion below uses: a year wide enough to hold "now",
/// which is when a deal closed in this test closes.
fn this_year() -> (Date, Date) {
    let today = time::OffsetDateTime::now_utc().date();
    (
        day(today.year(), Month::January, 1),
        day(today.year(), Month::December, 31),
    )
}

#[tokio::test]
async fn a_pipeline_report_counts_one_board_and_never_a_neighbours() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "a").await;
    let (b, _) = tenant_with_user(&store, "b").await;

    let (board, stages) = seeded_board(&a).await;
    let (new, qualified, won_col, lost_col) = (
        stages[0].clone(),
        stages[1].clone(),
        stages[3].clone(),
        stages[4].clone(),
    );
    let (b_board, b_stages) = seeded_board(&b).await;
    let (from, to) = this_year();

    // ---- an empty board: a report, not a refusal, and no invented currency
    let empty = a.crm_pipeline_report(&board, from, to).await.unwrap();
    assert_eq!(empty.pipeline_id, board);
    assert_eq!(empty.pipeline_name, "Sales");
    assert_eq!(empty.from, from);
    assert_eq!(empty.to, to);
    assert!(
        empty.currencies.is_empty(),
        "nothing billed is no groups, not one group of zeros"
    );

    // ---- the open board: two in New, one in Qualified --------------------
    a.create_crm_deal(&board, &new, &priced("Renewal — Acme", 250_000, "EUR"))
        .await
        .unwrap();
    a.create_crm_deal(&board, &new, &priced("Pilot — Beta", 50_000, "EUR"))
        .await
        .unwrap();
    a.create_crm_deal(&board, &qualified, &priced("Expansion", 1_000_000, "EUR"))
        .await
        .unwrap();

    // ---- the neighbour works an identically-named board, in the same
    //      currency, with much larger numbers: none of it may leak in.
    for title in ["Theirs one", "Theirs two"] {
        let theirs = b
            .create_crm_deal(&b_board, &b_stages[0], &priced(title, 99_000_000, "EUR"))
            .await
            .unwrap();
        b.move_crm_deal(&theirs, &StageMove::to(b_stages[3].clone()))
            .await
            .unwrap();
    }

    let open = a.crm_pipeline_report(&board, from, to).await.unwrap();
    assert_eq!(open.currencies.len(), 1);
    let eur = &open.currencies[0];
    assert_eq!(eur.currency, "EUR");
    assert_eq!(
        eur.stages
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["New", "Qualified", "Proposal", "Won", "Lost"],
        "every column of the board, in board order"
    );
    assert_eq!(
        eur.stages[0].open,
        PipelineTally {
            deal_count: 2,
            value_cents: 300_000
        }
    );
    assert_eq!(
        eur.stages[1].open,
        PipelineTally {
            deal_count: 1,
            value_cents: 1_000_000
        }
    );
    assert_eq!(eur.stages[2].open, PipelineTally::default());
    assert_eq!(
        eur.open,
        PipelineTally {
            deal_count: 3,
            value_cents: 1_300_000
        },
        "hand-computed: 2 500 + 500 + 10 000"
    );
    assert_eq!(eur.won, PipelineTally::default(), "nothing has closed yet");
    assert_eq!(
        eur.win_rate_bp(),
        None,
        "a win rate over nothing is unasked"
    );

    // ---- closing moves value out of the open board and into an outcome ---
    let deals = a.crm_deals(&Default::default()).await.unwrap();
    let acme = deals
        .iter()
        .find(|d| d.title == "Renewal — Acme")
        .unwrap()
        .id
        .clone();
    let beta = deals
        .iter()
        .find(|d| d.title == "Pilot — Beta")
        .unwrap()
        .id
        .clone();
    a.move_crm_deal(&acme, &StageMove::to(won_col.clone()))
        .await
        .unwrap();
    a.move_crm_deal(
        &beta,
        &StageMove {
            stage_id: lost_col.clone(),
            position: None,
            lost_reason: Some("Price".to_owned()),
        },
    )
    .await
    .unwrap();

    let closed = a.crm_pipeline_report(&board, from, to).await.unwrap();
    let eur = &closed.currencies[0];
    assert_eq!(
        eur.open,
        PipelineTally {
            deal_count: 1,
            value_cents: 1_000_000
        },
        "only the deal still being worked"
    );
    assert_eq!(
        eur.stages[0].open,
        PipelineTally::default(),
        "New emptied out"
    );
    assert_eq!(
        eur.won,
        PipelineTally {
            deal_count: 1,
            value_cents: 250_000
        }
    );
    assert_eq!(
        eur.lost,
        PipelineTally {
            deal_count: 1,
            value_cents: 50_000
        }
    );
    assert_eq!(eur.win_rate_bp(), Some(5_000), "one of two");
    // The columns a deal closed in still carry no OPEN work.
    assert_eq!(eur.stages[3].open, PipelineTally::default());
    assert!(eur.stages[3].is_won && eur.stages[4].is_lost);

    // ---- a period that excludes today excludes the outcomes, not the board
    let last_year = (day(2019, Month::January, 1), day(2019, Month::December, 31));
    let elsewhere = a
        .crm_pipeline_report(&board, last_year.0, last_year.1)
        .await
        .unwrap();
    let eur = &elsewhere.currencies[0];
    assert_eq!(
        eur.open.value_cents, 1_000_000,
        "the open board is a snapshot, not a period"
    );
    assert_eq!(eur.won, PipelineTally::default());
    assert_eq!(eur.lost, PipelineTally::default());

    // ---- a second currency is a second group, never a converted total ----
    a.create_crm_deal(&board, &new, &priced("US pilot", 700_000, "USD"))
        .await
        .unwrap();
    let mixed = a.crm_pipeline_report(&board, from, to).await.unwrap();
    assert_eq!(
        mixed
            .currencies
            .iter()
            .map(|c| c.currency.as_str())
            .collect::<Vec<_>>(),
        ["EUR", "USD"]
    );
    assert_eq!(mixed.currencies[0].open.value_cents, 1_000_000);
    assert_eq!(mixed.currencies[1].open.value_cents, 700_000);
    assert_eq!(mixed.currencies[1].won, PipelineTally::default());

    // ---- the neighbour's own board is unmoved by all of ours -------------
    let theirs = b.crm_pipeline_report(&b_board, from, to).await.unwrap();
    assert_eq!(theirs.currencies.len(), 1);
    assert_eq!(
        theirs.currencies[0].won,
        PipelineTally {
            deal_count: 2,
            value_cents: 198_000_000
        }
    );
    assert_eq!(theirs.currencies[0].open, PipelineTally::default());

    // ---- and neither tenant can ask about the other's board at all -------
    assert_not_found(b.crm_pipeline_report(&board, from, to).await);
    assert_not_found(a.crm_pipeline_report(&b_board, from, to).await);
    assert_not_found(
        a.crm_pipeline_report(&CrmPipelineId::new("pip_nope"), from, to)
            .await,
    );
}

#[tokio::test]
async fn a_report_period_is_stated_forwards() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "period").await;
    let (board, _) = seeded_board(&a).await;
    let refusal = a
        .crm_pipeline_report(
            &board,
            day(2026, Month::March, 3),
            day(2026, Month::March, 2),
        )
        .await;
    match refusal {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains("ends before"), "{msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
    // A backwards period is refused before the board is resolved, so it stays
    // a `422` about the question rather than a `404` about the board — but an
    // unknown board with a good period is still the clean denial.
    assert_not_found(
        a.crm_pipeline_report(
            &CrmPipelineId::new("pip_nope"),
            day(2026, Month::March, 1),
            day(2026, Month::March, 31),
        )
        .await,
    );
}
