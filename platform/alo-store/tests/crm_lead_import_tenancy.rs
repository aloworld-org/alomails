//! The lead import over a real Postgres (alo CRM, B2.09) — the arc, the
//! all-or-nothing rule, the duplicate rules, and the tenant wall (Law 1:
//! isolation is tested, not assumed).
//!
//! The pure reading of a file — the delimiters, the amounts, the mapping guess
//! — is unit-tested in `alo_store::crm_lead_import` and `alo_store::csv_read`.
//! What can only be proven against a database is here: that a preview writes
//! nothing, that a commit writes everything or nothing, that duplicates are
//! decided against **this** tenant's customers and open deals and never
//! another's, and that an outsider tenant reaching for this board gets the
//! clean `NotFound` an id that never existed gets.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::crm_lead_import::{
    DuplicateReason, DuplicateSource, LeadImportRequest, LeadMapping, MAX_IMPORT_ROWS,
};
use alo_store::{
    AccountStore, CrmPipelineId, CrmStageId, DealFilter, DealState, NewCustomer, NewDeal,
    PipelineSeed, StageSeed, Store, StoreError, TenantId,
};

/// The fixture a person would actually upload: a semicolon file from a
/// European spreadsheet, with grouped amounts, an accented name, a blank line
/// in the middle, and two rows that name a company already in the file.
const LEADS: &str = include_str!("fixtures/crm_leads.csv");

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
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
            stage_seed("Won", true, false),
            stage_seed("Lost", false, true),
        ],
    }
}

/// A tenant with one user and its seeded board.
async fn tenant(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, CrmPipelineId, Vec<CrmStageId>) {
    let tenant = store.create_tenant(&format!("crmimp-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crmimp.test"))
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user);
    let boards = acc.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let board = boards[0].id.clone();
    let stages = acc
        .crm_stages(&board, false)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    (acc, tenant, board, stages)
}

/// A request that lets the header speak for itself.
fn guessed(board: &CrmPipelineId) -> LeadImportRequest {
    LeadImportRequest {
        pipeline_id: board.clone(),
        stage_id: None,
        mapping: LeadMapping::default(),
    }
}

/// Every deal of a tenant, however it got there.
async fn deals(acc: &AccountStore) -> Vec<alo_store::Deal> {
    acc.crm_deals(&DealFilter::default()).await.unwrap()
}

#[tokio::test]
async fn a_preview_writes_nothing_and_the_commit_writes_all_of_it() {
    let store = common::test_store().await;
    let (a, _t, board, stages) = tenant(&store, "arc").await;

    // ---- the preview -------------------------------------------------------
    let preview = a
        .preview_crm_lead_import(&guessed(&board), LEADS.as_bytes())
        .await
        .unwrap();
    assert!(!preview.committed, "a preview never writes");
    assert_eq!(preview.delimiter, ';', "a European export, sniffed");
    assert_eq!(preview.encoding, "utf-8");
    assert_eq!(preview.total_rows, 6, "the blank line is not a row");
    assert_eq!(preview.leads.len(), 4);
    assert_eq!(preview.duplicates.len(), 2);
    assert!(preview.errors.is_empty(), "{:?}", preview.errors);
    assert_eq!(
        preview.mapping.company.as_deref(),
        Some("Company"),
        "the header named its own columns"
    );
    assert_eq!(preview.mapping.email.as_deref(), Some("E-mail"));
    assert!(preview.leads.iter().all(|lead| lead.id.is_none()));
    assert!(deals(&a).await.is_empty(), "the preview wrote nothing");

    // The amounts every European spreadsheet writes, as integer cents.
    let acme = &preview.leads[0];
    assert_eq!(acme.line, 2);
    assert_eq!(acme.deal.title, "Acme GmbH");
    assert_eq!(acme.deal.value_cents, 1_250_000);
    assert_eq!(acme.deal.currency, "EUR");
    assert_eq!(acme.deal.source, "trade fair");
    assert!(acme.deal.expected_close.is_some());
    let gamma = &preview.leads[2];
    assert_eq!(gamma.deal.contact_name, "Chloé Martin", "UTF-8 survived");
    assert_eq!(gamma.deal.value_cents, 750_050);
    assert_eq!(gamma.deal.currency, "CHF", "the file's own currency");
    let delta = &preview.leads[3];
    assert_eq!(delta.line, 6, "the blank line still moved the numbering");
    assert_eq!(
        delta.deal.currency, "EUR",
        "the default when none is stated"
    );
    assert_eq!(delta.deal.value_cents, 0);

    // The two skipped rows: the same address, then the same company domain.
    assert_eq!(preview.duplicates[0].line, 7);
    assert_eq!(preview.duplicates[0].reason, DuplicateReason::Email);
    assert_eq!(preview.duplicates[0].source, DuplicateSource::File);
    assert_eq!(preview.duplicates[1].line, 8);
    assert_eq!(preview.duplicates[1].reason, DuplicateReason::Domain);
    assert_eq!(preview.duplicates[1].matched, "acme.example");

    // ---- the commit --------------------------------------------------------
    let report = a
        .import_crm_leads(&guessed(&board), LEADS.as_bytes())
        .await
        .unwrap();
    assert!(report.committed);
    assert_eq!(report.leads.len(), 4);
    assert!(report.leads.iter().all(|lead| lead.id.is_some()));

    let stored = deals(&a).await;
    assert_eq!(stored.len(), 4);
    // Every imported deal is an ordinary deal: open, owned by the importer, in
    // the board's first column, with its first history row written.
    for deal in &stored {
        assert_eq!(deal.state(), DealState::Open);
        assert_eq!(deal.stage_id, stages[0], "the first live column");
        assert_eq!(deal.owner_user_id, a.user().as_str());
        let history = a.crm_deal_history(&deal.id).await.unwrap();
        assert_eq!(history.len(), 1, "raised here, never moved");
        assert!(history[0].from_stage_id.is_none());
    }
    let acme = stored.iter().find(|d| d.title == "Acme GmbH").unwrap();
    assert_eq!(acme.contact_email, "ada@acme.example");
    assert_eq!(acme.value_cents, 1_250_000);

    // ---- and again: the tenant now knows all four ---------------------------
    let again = a
        .import_crm_leads(&guessed(&board), LEADS.as_bytes())
        .await
        .unwrap();
    assert!(again.committed, "an import of nothing is not a failure");
    assert!(again.leads.is_empty());
    assert_eq!(again.duplicates.len(), 6, "every row is now known");
    assert!(
        again
            .duplicates
            .iter()
            .all(|row| row.source == DuplicateSource::Crm)
    );
    assert_eq!(deals(&a).await.len(), 4, "nothing was doubled");
}

#[tokio::test]
async fn one_unimportable_row_refuses_the_whole_file() {
    let store = common::test_store().await;
    let (a, _t, board, _stages) = tenant(&store, "allornothing").await;

    let file = "Company,Email,Amount\n\
                Acme GmbH,ada@acme.example,100\n\
                Beta BV,not-an-address,200\n\
                Gamma SA,chloe@gamma.example,300\n";
    let report = a
        .import_crm_leads(&guessed(&board), file.as_bytes())
        .await
        .unwrap();
    assert!(!report.committed, "one bad row refuses the file");
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].line, 3);
    assert!(
        !report.errors[0].rule.contains("not-an-address"),
        "a refusal never quotes the row"
    );
    assert_eq!(
        report.leads.len(),
        2,
        "the report still says what would have landed"
    );
    assert!(
        deals(&a).await.is_empty(),
        "all-or-nothing: not even the good rows"
    );

    // Fixed, the same file lands whole.
    let fixed = file.replace("not-an-address", "bob@beta.example");
    let report = a
        .import_crm_leads(&guessed(&board), fixed.as_bytes())
        .await
        .unwrap();
    assert!(report.committed);
    assert_eq!(deals(&a).await.len(), 3);
}

#[tokio::test]
async fn duplicates_are_decided_against_customers_and_open_deals_only() {
    let store = common::test_store().await;
    let (a, _t, board, stages) = tenant(&store, "dupes").await;

    // A customer the tenant already invoices.
    a.create_billing_customer(&NewCustomer {
        name: "Acme GmbH".to_owned(),
        country: "DE".to_owned(),
        email: Some("billing@acme.example".to_owned()),
        ..NewCustomer::default()
    })
    .await
    .unwrap();
    // An open deal with a contact of its own.
    a.create_crm_deal(
        &board,
        &stages[0],
        &NewDeal {
            title: "Beta BV".to_owned(),
            contact_email: "bob@beta.example".to_owned(),
            ..NewDeal::default()
        },
    )
    .await
    .unwrap();
    // A deal that is over: history must not make tomorrow's lead a duplicate.
    let lost = a
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: "Gamma SA".to_owned(),
                contact_email: "chloe@gamma.example".to_owned(),
                ..NewDeal::default()
            },
        )
        .await
        .unwrap();
    a.move_crm_deal(
        &lost,
        &alo_store::StageMove {
            stage_id: stages[3].clone(),
            position: None,
            lost_reason: Some("no budget".to_owned()),
        },
    )
    .await
    .unwrap();

    let file = "Company,Email\n\
                Acme GmbH,ada@acme.example\n\
                Beta BV,bob@beta.example\n\
                Gamma SA,chloe@gamma.example\n\
                Ada's bakery,ada@gmail.com\n\
                Bob's bikes,bob@gmail.com\n";
    let report = a
        .preview_crm_lead_import(&guessed(&board), file.as_bytes())
        .await
        .unwrap();
    assert_eq!(report.duplicates.len(), 2, "{:?}", report.duplicates);
    // The customer's domain caught the first, the open deal's address the
    // second.
    assert_eq!(report.duplicates[0].line, 2);
    assert_eq!(report.duplicates[0].reason, DuplicateReason::Domain);
    assert_eq!(report.duplicates[0].source, DuplicateSource::Crm);
    assert_eq!(report.duplicates[1].line, 3);
    assert_eq!(report.duplicates[1].reason, DuplicateReason::Email);
    // The lost deal's contact is a lead again, and two unrelated people at one
    // free-mail provider are two people.
    let taken: Vec<&str> = report
        .leads
        .iter()
        .map(|lead| lead.deal.contact_email.as_str())
        .collect();
    assert_eq!(
        taken,
        ["chloe@gamma.example", "ada@gmail.com", "bob@gmail.com"]
    );
}

#[tokio::test]
async fn a_neighbours_board_is_an_id_that_never_existed() {
    let store = common::test_store().await;
    let (a, _ta, a_board, a_stages) = tenant(&store, "wall-a").await;
    let (b, _tb, b_board, _b_stages) = tenant(&store, "wall-b").await;

    // A knows a company; B must not learn that from an import report.
    a.create_billing_customer(&NewCustomer {
        name: "Acme GmbH".to_owned(),
        country: "DE".to_owned(),
        email: Some("billing@acme.example".to_owned()),
        ..NewCustomer::default()
    })
    .await
    .unwrap();
    a.import_crm_leads(&guessed(&a_board), LEADS.as_bytes())
        .await
        .unwrap();

    let file = "Company,Email\nAcme GmbH,ada@acme.example\n";

    // B reaching for A's board — preview and commit, named column and not.
    assert_not_found(
        b.preview_crm_lead_import(&guessed(&a_board), file.as_bytes())
            .await,
    );
    assert_not_found(
        b.import_crm_leads(&guessed(&a_board), file.as_bytes())
            .await,
    );
    assert_not_found(
        b.import_crm_leads(
            &LeadImportRequest {
                pipeline_id: a_board.clone(),
                stage_id: Some(a_stages[0].clone()),
                mapping: LeadMapping::default(),
            },
            file.as_bytes(),
        )
        .await,
    );
    // A board that never existed answers identically — no existence oracle.
    assert_not_found(
        b.import_crm_leads(&guessed(&CrmPipelineId::new("pip_nope")), file.as_bytes())
            .await,
    );
    // A's own column on B's board is not B's column either.
    assert_not_found(
        b.import_crm_leads(
            &LeadImportRequest {
                pipeline_id: b_board.clone(),
                stage_id: Some(a_stages[0].clone()),
                mapping: LeadMapping::default(),
            },
            file.as_bytes(),
        )
        .await,
    );
    assert!(deals(&b).await.is_empty(), "nothing landed for B");
    // Three, not four: A's own customer already holds the acme.example domain,
    // so A's first row was a duplicate for A — which is exactly the fact B must
    // not be able to observe.
    assert_eq!(deals(&a).await.len(), 3, "and nothing changed for A");

    // The one that matters most: A's customers and deals are invisible to B's
    // duplicate rules, so the same file is entirely new work for B.
    let report = b
        .import_crm_leads(&guessed(&b_board), LEADS.as_bytes())
        .await
        .unwrap();
    assert!(report.committed);
    assert_eq!(
        report.leads.len(),
        4,
        "the row A skipped as a duplicate is new work for B"
    );
    assert!(
        report
            .duplicates
            .iter()
            .all(|row| row.source == DuplicateSource::File),
        "only this file's own repeats: {:?}",
        report.duplicates
    );
}

#[tokio::test]
async fn the_column_the_leads_land_in_is_the_callers_or_the_first_one() {
    let store = common::test_store().await;
    let (a, _t, board, stages) = tenant(&store, "column").await;

    let file = "Company,Email\nAcme GmbH,ada@acme.example\n";
    a.import_crm_leads(
        &LeadImportRequest {
            pipeline_id: board.clone(),
            stage_id: Some(stages[1].clone()),
            mapping: LeadMapping::default(),
        },
        file.as_bytes(),
    )
    .await
    .unwrap();
    assert_eq!(deals(&a).await[0].stage_id, stages[1]);

    // An archived column is not a place to import into — the same rule a card
    // move lives by.
    a.set_crm_stage_archived(&stages[2], true).await.unwrap();
    let refused = a
        .import_crm_leads(
            &LeadImportRequest {
                pipeline_id: board.clone(),
                stage_id: Some(stages[2].clone()),
                mapping: LeadMapping::default(),
            },
            "Company,Email\nBeta BV,bob@beta.example\n".as_bytes(),
        )
        .await;
    match refused {
        Err(StoreError::Validation(rule)) => assert!(rule.contains("archived"), "{rule}"),
        other => panic!("expected the archived-column rule, got {other:?}"),
    }
}

#[tokio::test]
async fn a_file_that_is_not_a_lead_list_is_refused_before_anything_is_read() {
    let store = common::test_store().await;
    let (a, _t, board, _stages) = tenant(&store, "unreadable").await;

    for (file, rule) in [
        ("".to_owned(), "empty"),
        ("\n\n".to_owned(), "no rows"),
        (
            "Company,Email\nAcme,a@b.example,extra\n".to_owned(),
            "more fields than the header",
        ),
        (
            format!("Company\n{}", "Acme\n".repeat(MAX_IMPORT_ROWS + 1)),
            "more than",
        ),
    ] {
        match a
            .preview_crm_lead_import(&guessed(&board), file.as_bytes())
            .await
        {
            Err(StoreError::Validation(got)) => {
                assert!(got.contains(rule), "expected {rule:?}, got {got:?}");
            }
            other => panic!("expected a refusal naming {rule:?}, got {other:?}"),
        }
    }
    // A mapping naming a column the file does not have is a refusal, not a
    // silently blank import.
    let refused = a
        .preview_crm_lead_import(
            &LeadImportRequest {
                pipeline_id: board.clone(),
                stage_id: None,
                mapping: LeadMapping {
                    company: Some("Firma".to_owned()),
                    ..LeadMapping::default()
                },
            },
            "Company,Email\nAcme,a@b.example\n".as_bytes(),
        )
        .await;
    match refused {
        Err(StoreError::Validation(rule)) => {
            assert!(rule.contains("no column mapped to company name"), "{rule}");
        }
        other => panic!("expected the mapping refusal, got {other:?}"),
    }
    assert!(deals(&a).await.is_empty());
}
