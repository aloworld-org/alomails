//! The Sites → CRM/Billing seam (S2.10b): a website enquiry becomes an
//! opportunity, the funnel joins the two, and neither ever crosses a tenant.
//!
//! Four things this suite holds the seam to, against the real database:
//!
//! - **Nothing crosses a tenant, in either direction.** A neighbour's
//!   submission cannot be handed off, a neighbour's deal cannot be linked to
//!   an owned enquiry, a neighbour's site reports nothing, and a neighbour's
//!   link cannot be removed.
//! - **One enquiry becomes at most one lead.** The same deal twice is the same
//!   decision; a different deal is refused by name.
//! - **The join is the stated rule.** An invoice raised for the customer the
//!   lead became counts only if it was raised after the enquiry and actually
//!   issued — a draft, a void, and a document that predates the link do not.
//! - **Money is per currency and never mixed**, and the counters survive the
//!   erasure of the personal data behind them.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{
    AccountStore, BillingCustomerId, BlobStore, ConversionStage, CrmDealId, CrmPipelineId,
    CrmStageId, NewCustomer, NewDeal, NewInvoice, NewLine, PipelineSeed, SiteFormId,
    SiteFormSubmissionId, SiteId, SiteLeadDraft, SitePublicStore, StageMove, StageSeed, Store,
    StoreError,
};
use sqlx::postgres::PgPoolOptions;
use time::{Date, Duration, OffsetDateTime};

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("attribution-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}-{tenant}@example.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    // Issuing books the document (B7.01), so the counted-revenue setups below
    // need a chart the booking can resolve its roles against.
    let seed = alo_store::ChartSeed {
        names: alo_store::CHART
            .iter()
            .map(|entry| alo_store::ChartName {
                code: entry.code.to_owned(),
                name: format!("Account {}", entry.code),
            })
            .collect(),
    };
    account.fin_accounts_or_seed(&seed, false).await.unwrap();
    account
}

/// A published site with one contact form — the conversion point everything
/// below is counted and attributed under.
async fn published(acc: &AccountStore, tag: &str) -> (SiteId, String, SiteFormId) {
    let suffix = SiteId::generate()
        .as_str()
        .to_ascii_lowercase()
        .replace('_', "-");
    let subdomain = format!("{tag}-{suffix}");
    let site = acc.create_site(tag, &subdomain).await.unwrap();
    acc.create_site_page(&site, "Home", "", true).await.unwrap();
    let form = acc.create_site_form(&site, "Contact").await.unwrap();
    acc.publish_site(&site).await.unwrap();
    (site, subdomain, form)
}

async fn enquiry(
    acc: &AccountStore,
    site: &SiteId,
    form: &SiteFormId,
    who: &str,
) -> SiteFormSubmissionId {
    acc.add_site_form_submission(
        site,
        form,
        who,
        &format!("{who}@visitor.example"),
        "We would like a quote for twelve desks.",
    )
    .await
    .unwrap()
}

fn sales_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: vec![
            StageSeed {
                name: "New".to_owned(),
                is_won: false,
                is_lost: false,
            },
            StageSeed {
                name: "Won".to_owned(),
                is_won: true,
                is_lost: false,
            },
        ],
    }
}

async fn seeded_board(acc: &AccountStore) -> (CrmPipelineId, Vec<CrmStageId>) {
    let boards = acc.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let board = boards[0].id.clone();
    let stages = acc
        .crm_stages(&board, false)
        .await
        .unwrap()
        .into_iter()
        .map(|stage| stage.id)
        .collect();
    (board, stages)
}

fn draft(title: &str, value_cents: i64, currency: &str) -> SiteLeadDraft {
    SiteLeadDraft {
        title: title.to_owned(),
        value_cents,
        currency: currency.to_owned(),
        ..Default::default()
    }
}

async fn customer(acc: &AccountStore, name: &str) -> BillingCustomerId {
    acc.create_billing_customer(&NewCustomer {
        name: name.to_owned(),
        country: "DE".to_owned(),
        ..Default::default()
    })
    .await
    .unwrap()
}

/// An issued invoice for `customer`, worth `cents` net at 21 % VAT.
async fn issued_invoice(acc: &AccountStore, customer: &BillingCustomerId, cents: i64) {
    let invoice = acc
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    acc.set_billing_invoice_lines(
        &invoice,
        &[NewLine {
            description: "Desks".to_owned(),
            unit: "piece".to_owned(),
            qty_milli: 1_000,
            unit_price_cents: cents,
            vat_rate_bp: 2_100,
        }],
    )
    .await
    .unwrap();
    acc.issue_billing_invoice(&invoice).await.unwrap();
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn a_website_enquiry_becomes_an_opportunity_and_never_leaves_its_tenant() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let store = Store::new(pool.clone(), blobs.clone());
    store.migrate().await.unwrap();
    let public = SitePublicStore::new(pool.clone(), blobs);

    let owner = account(&store, "owner").await;
    let rival = account(&store, "rival").await;
    let (site, subdomain, form) = published(&owner, "alpha").await;
    let (rival_site, _, rival_form) = published(&rival, "bravo").await;

    // The period the report is asked for: today and yesterday. A handoff is
    // stamped now, so a window that ends today contains it.
    let today: Date = OffsetDateTime::now_utc().date();
    let yesterday = today - Duration::days(1);

    // Traffic on the conversion point, counted the anonymous way.
    let resolved = public.resolve_published(&subdomain).await.unwrap().unwrap();
    for stage in [
        ConversionStage::View,
        ConversionStage::View,
        ConversionStage::Start,
    ] {
        public
            .record_public_site_conversion(&resolved, today, form.as_str(), stage)
            .await
            .unwrap();
    }
    public
        .record_public_form_conversion(form.as_str(), today, ConversionStage::Submit)
        .await
        .unwrap();

    // Two enquiries arrive on the owner's form, one on the rival's.
    let first = enquiry(&owner, &site, &form, "ada").await;
    let second = enquiry(&owner, &site, &form, "bea").await;
    let rival_enquiry = enquiry(&rival, &rival_site, &rival_form, "cid").await;

    let (board, stages) = seeded_board(&owner).await;
    let (rival_board, rival_stages) = seeded_board(&rival).await;

    // ---- the wrong-tenant matrix -------------------------------------------

    // The rival's enquiry is not the owner's to hand off — on the owner's own
    // site or on the rival's.
    assert_not_found(
        owner
            .create_site_lead(
                &site,
                &rival_enquiry,
                &board,
                &stages[0],
                &draft("x", 0, ""),
            )
            .await,
    );
    assert_not_found(
        owner
            .create_site_lead(
                &rival_site,
                &rival_enquiry,
                &board,
                &stages[0],
                &draft("x", 0, ""),
            )
            .await,
    );
    // …and the rival cannot reach the owner's.
    assert_not_found(
        rival
            .create_site_lead(
                &rival_site,
                &first,
                &rival_board,
                &rival_stages[0],
                &draft("x", 0, ""),
            )
            .await,
    );

    // The handoff proper: the enquiry becomes an opportunity carrying the
    // enquirer's own name and address, never re-typed.
    let link = owner
        .create_site_lead(
            &site,
            &first,
            &board,
            &stages[0],
            &draft("Twelve desks — Ada", 250_000, "EUR"),
        )
        .await
        .unwrap();
    assert_eq!(link.source_kind, "form");
    assert_eq!(link.source_id, form.as_str());
    assert_eq!(link.submission_id.as_str(), first.as_str());
    assert_eq!(link.deal.value_cents, 250_000);
    assert_eq!(link.deal.currency, "EUR");
    let deal = owner.crm_deal(&link.deal.id).await.unwrap().unwrap();
    assert_eq!(deal.contact_name, "ada");
    assert_eq!(deal.contact_email, "ada@visitor.example");
    // No source was stated, so the deal says where it demonstrably came from.
    assert_eq!(deal.source, subdomain);

    // A neighbour's deal cannot be linked to an owned enquiry, and a
    // neighbour's link cannot be listed or removed.
    let rival_deal = rival
        .create_crm_deal(
            &rival_board,
            &rival_stages[0],
            &NewDeal {
                title: "Theirs".to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_not_found(owner.link_site_lead(&site, &second, &rival_deal).await);
    assert_not_found(
        owner
            .link_site_lead(&site, &second, &CrmDealId::new("nonesuch".to_owned()))
            .await,
    );
    assert!(rival.site_lead_links(&site).await.unwrap().is_empty());
    assert_not_found(rival.unlink_site_lead(&site, &link.id).await);
    assert!(
        rival
            .site_attribution(&site, yesterday, today)
            .await
            .unwrap()
            .is_none()
    );
    // The link survived every one of those attempts.
    assert_eq!(owner.site_lead_links(&site).await.unwrap().len(), 1);

    // ---- one enquiry, one lead ---------------------------------------------

    // The same decision made twice answers the same link…
    let again = owner
        .link_site_lead(&site, &first, &link.deal.id)
        .await
        .unwrap();
    assert_eq!(again.id.as_str(), link.id.as_str());
    // …and a second, different opportunity on the same enquiry is refused.
    let other = owner
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: "A twin".to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    match owner.link_site_lead(&site, &first, &other).await {
        Err(StoreError::Conflict(message)) => {
            assert!(message.contains("already"), "unexpected: {message}");
        }
        other => panic!("expected Conflict, got: {other:?}"),
    }
    // A refused handoff raised nothing: the second enquiry is still free.
    assert_eq!(owner.site_lead_links(&site).await.unwrap().len(), 1);

    // ---- the funnel --------------------------------------------------------

    let report = owner
        .site_attribution(&site, yesterday, today)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((report.views, report.starts, report.submits), (2, 1, 1));
    assert_eq!(report.leads, 1);
    assert_eq!((report.deals_open, report.deals_won), (1, 0));
    assert_eq!(report.invoices, 0);
    assert_eq!(report.sources.len(), 1);
    let source = &report.sources[0];
    assert_eq!(source.id, form.as_str());
    assert_eq!(source.name.as_deref(), Some("Contact"));
    assert_eq!(source.leads, 1);
    assert_eq!(source.money.len(), 1);
    assert_eq!(source.money[0].currency, "EUR");
    assert_eq!(source.money[0].open_cents, 250_000);
    assert_eq!(source.money[0].won_cents, 0);

    // The rival's own site is untouched by any of it.
    let rival_report = rival
        .site_attribution(&rival_site, yesterday, today)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rival_report.leads, 0);
    assert_eq!(rival_report.submits, 0);
}

#[tokio::test]
async fn the_invoice_join_is_the_rule_it_states() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let store = Store::new(pool.clone(), blobs.clone());
    store.migrate().await.unwrap();

    let owner = account(&store, "money").await;
    let (site, _, form) = published(&owner, "charlie").await;
    let today: Date = OffsetDateTime::now_utc().date();
    let yesterday = today - Duration::days(1);
    let (board, stages) = seeded_board(&owner).await;

    // A company the tenant was ALREADY invoicing before they ever wrote in.
    let acme = customer(&owner, "Acme GmbH").await;
    issued_invoice(&owner, &acme, 100_000).await;

    let submission = enquiry(&owner, &site, &form, "ada").await;
    let deal = owner
        .create_crm_deal(
            &board,
            &stages[0],
            &NewDeal {
                title: "Renewal — Acme".to_owned(),
                customer_id: Some(acme.clone()),
                value_cents: 400_000,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    owner
        .link_site_lead(&site, &submission, &deal)
        .await
        .unwrap();

    // Everything raised for that customer AFTER the enquiry: one issued, one
    // left a draft.
    issued_invoice(&owner, &acme, 200_000).await;
    let unfinished = owner
        .create_billing_invoice(&NewInvoice::for_customer(acme.clone()))
        .await
        .unwrap();
    owner
        .set_billing_invoice_lines(
            &unfinished,
            &[NewLine {
                description: "Thinking about it".to_owned(),
                unit: String::new(),
                qty_milli: 1_000,
                unit_price_cents: 900_000,
                vat_rate_bp: 2_100,
            }],
        )
        .await
        .unwrap();

    let report = owner
        .site_attribution(&site, yesterday, today)
        .await
        .unwrap()
        .unwrap();
    // The back catalogue is not credited to the form, and neither is a draft:
    // exactly one document, at 200 000 cents net plus 21 % VAT.
    assert_eq!(report.invoices, 1);
    assert_eq!(report.money.len(), 1);
    assert_eq!(report.money[0].currency, "EUR");
    assert_eq!(report.money[0].invoiced_cents, 242_000);
    assert_eq!(report.money[0].open_cents, 400_000);
    assert_eq!(report.sources[0].invoices, 1);

    // Won money moves from the open line to the won line, in the deal's own
    // currency and without touching the invoices.
    owner
        .move_crm_deal(&deal, &StageMove::to(stages[1].clone()))
        .await
        .unwrap();
    let won = owner
        .site_attribution(&site, yesterday, today)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((won.deals_open, won.deals_won), (0, 1));
    assert_eq!(won.money[0].open_cents, 0);
    assert_eq!(won.money[0].won_cents, 400_000);
    assert_eq!(won.money[0].invoiced_cents, 242_000);

    // A second enquiry priced in another currency is its own line, never
    // summed into the first.
    let second = enquiry(&owner, &site, &form, "bea").await;
    owner
        .create_site_lead(
            &site,
            &second,
            &board,
            &stages[0],
            &draft("Export order", 150_000, "USD"),
        )
        .await
        .unwrap();
    let mixed = owner
        .site_attribution(&site, yesterday, today)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mixed.leads, 2);
    assert_eq!(mixed.money.len(), 2);
    assert_eq!(mixed.money[0].currency, "EUR");
    assert_eq!(mixed.money[1].currency, "USD");
    assert_eq!(mixed.money[1].open_cents, 150_000);
    assert_eq!(mixed.money[1].invoiced_cents, 0);

    // Erasing the visitor's message takes the claim that they wrote in with
    // it — and leaves the aggregate counters, which never held an identity,
    // exactly where they were.
    owner
        .delete_form_submission(&site, &form, &second)
        .await
        .unwrap();
    let after = owner
        .site_attribution(&site, yesterday, today)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.leads, 1);
    assert_eq!(after.money.len(), 1);
    assert_eq!(after.submits, mixed.submits);
    // The opportunity itself is CRM's and is untouched by the erasure.
    assert_eq!(owner.crm_deals(&Default::default()).await.unwrap().len(), 2);
}
