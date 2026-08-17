//! The `/campaigns/*` HTTP surface (C1.5, ADR 0044) — the audience, the saved
//! question, and the two records that decide who may be mailed, driven through
//! the real router over a real Postgres.
//!
//! `alo-store`'s own suites prove the queries; what matters here is the
//! **edge**, and on this surface the edge is a person's inbox. So beyond the
//! usual auth guard and status codes, three things are asserted that no other
//! module's HTTP suite has to assert:
//!
//! - **The exclusions travel with the count.** A route that answered "412
//!   recipients" without saying who the other 88 are would make the number
//!   unauditable, and a colleague would find out by sending.
//! - **Nothing on this surface can lift a suppression or edit consent.** The
//!   methods are absent rather than guarded — `405`, not `403` — because a
//!   route that exists is a route somebody points a bulk importer at.
//! - **Every route is wrong-tenant tested**, including from both sides where a
//!   leak could be silent: both tenants hold the same address, and each one's
//!   answer is asserted whole so a leak has to show up as a named extra row.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{
    AccountStore, ConsentSource, NewCampaignConsent, NewCustomer, NewDeal, NewSuppression,
    PipelineSeed, StageSeed, SuppressionReason, TenantStore,
};

use common::{Harness, harness, harness_on, send};

// ---- request helpers ---------------------------------------------------------

fn request(method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request("GET", uri, Some(token), None)).await
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, request("POST", uri, Some(token), Some(body))).await
}

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, request("PATCH", uri, Some(token), Some(body))).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request("DELETE", uri, Some(token), None)).await
}

/// The addresses in an audience answer, in the order the route returned them.
fn addresses(body: &Value) -> Vec<String> {
    body["people"]
        .as_array()
        .unwrap_or_else(|| panic!("no people array in {body}"))
        .iter()
        .map(|person| person["address"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// One person out of an audience answer, by address.
fn person<'a>(body: &'a Value, address: &str) -> &'a Value {
    body["people"]
        .as_array()
        .unwrap_or_else(|| panic!("no people array in {body}"))
        .iter()
        .find(|person| person["address"] == address)
        .unwrap_or_else(|| panic!("no {address} in {body}"))
}

/// A tally's exclusions as `(reason, people)` pairs, in the order returned.
fn exclusions(body: &Value) -> Vec<(String, i64)> {
    body["tally"]["excluded"]
        .as_array()
        .unwrap_or_else(|| panic!("no exclusions in {body}"))
        .iter()
        .map(|bucket| {
            (
                bucket["reason"].as_str().unwrap_or_default().to_owned(),
                bucket["people"].as_i64().unwrap_or_default(),
            )
        })
        .collect()
}

// ---- the fixture -------------------------------------------------------------

/// A customer the tenant invoices, in a country.
async fn customer(store: &AccountStore, name: &str, email: &str, country: &str) {
    store
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: country.to_owned(),
            currency: "EUR".to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
}

/// A CRM deal contact — somebody the tenant knows and has no country for.
async fn deal(store: &AccountStore, name: &str, email: &str) {
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
                title: format!("Deal with {name}"),
                contact_name: name.to_owned(),
                contact_email: email.to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

/// Somebody agreed, and here is what they agreed to.
async fn agreed(store: &AccountStore, address: &str) {
    store
        .record_campaign_consent(&NewCampaignConsent {
            address,
            source: ConsentSource::Manual,
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

/// Four people this tenant knows, in the four states the screen has to be able
/// to explain: mailable and a customer, mailable and a lead, known but never
/// asked, and gone for good.
async fn four_people(h: &Harness) {
    customer(&h.acc, "Acme GmbH", "orders@acme.test", "BE").await;
    customer(&h.acc, "Bravo BV", "hello@bravo.test", "NL").await;
    customer(&h.acc, "Cirrus SA", "quit@cirrus.test", "BE").await;
    deal(&h.acc, "Ann Dupont", "ann@lead.test").await;

    agreed(&h.acc, "orders@acme.test").await;
    agreed(&h.acc, "ann@lead.test").await;
    agreed(&h.acc, "quit@cirrus.test").await;
    suppressed(&h.ts, "quit@cirrus.test", SuppressionReason::Unsubscribe).await;
}

/// Every route this module registers, as `(method, path, body)` — the list the
/// auth guard and the wrong-tenant sweep both walk, so a route added later
/// without a guard fails a test rather than shipping.
fn every_route(segment: &str, address: &str) -> Vec<(&'static str, String, Option<Value>)> {
    vec![
        ("GET", "/campaigns/audience".to_owned(), None),
        ("GET", "/campaigns/audience/tally".to_owned(), None),
        (
            "POST",
            "/campaigns/consent".to_owned(),
            Some(json!({
                "address": "someone@elsewhere.test",
                "source": "manual",
                "statement": "Said yes on the telephone",
            })),
        ),
        ("GET", format!("/campaigns/consent/{address}"), None),
        ("GET", "/campaigns/suppressions".to_owned(), None),
        (
            "POST",
            "/campaigns/suppressions".to_owned(),
            Some(json!({ "address": "someone@elsewhere.test", "reason": "manual" })),
        ),
        ("GET", format!("/campaigns/suppressions/{address}"), None),
        ("GET", "/campaigns/segments".to_owned(), None),
        (
            "POST",
            "/campaigns/segments".to_owned(),
            Some(json!({ "name": "Everybody" })),
        ),
        ("GET", format!("/campaigns/segments/{segment}"), None),
        (
            "PATCH",
            format!("/campaigns/segments/{segment}"),
            Some(json!({ "name": "Renamed by a stranger" })),
        ),
        ("DELETE", format!("/campaigns/segments/{segment}"), None),
    ]
}

// ---- the guard ---------------------------------------------------------------

#[tokio::test]
async fn every_campaigns_route_refuses_a_caller_with_no_token() {
    let h = harness("camp-auth").await;

    for (method, path, body) in every_route("any-segment", "ann@lead.test") {
        let (status, _) = send(&h.app, request(method, &path, None, body)).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} answered an anonymous caller"
        );
    }
}

// ---- the audience ------------------------------------------------------------

#[tokio::test]
async fn the_audience_names_everybody_and_says_which_of_them_will_not_be_mailed() {
    let h = harness("camp-audience").await;
    four_people(&h).await;

    let (status, body) = get(&h.app, &h.token, "/campaigns/audience").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        addresses(&body),
        vec![
            "ann@lead.test".to_owned(),
            "hello@bravo.test".to_owned(),
            "orders@acme.test".to_owned(),
            "quit@cirrus.test".to_owned(),
        ],
        "everybody the tenant knows, including the people it may not mail"
    );

    // The person who agreed: mailable, carrying the evidence id to read the
    // statement from.
    let acme = person(&body, "orders@acme.test");
    assert_eq!(acme["mailable"], true);
    assert_eq!(acme["exclusionReason"], Value::Null);
    assert_eq!(acme["country"], "BE");
    assert_eq!(acme["sources"], json!(["billing_customer"]));
    assert!(
        acme["consent"]["recordId"].as_str().is_some(),
        "a recipient carries the reason they are one: {acme}"
    );

    // Known, never asked. Not a gap to be filled in by whoever sends.
    let bravo = person(&body, "hello@bravo.test");
    assert_eq!(bravo["mailable"], false);
    assert_eq!(bravo["exclusionReason"], "no_consent");
    assert_eq!(bravo["consent"], Value::Null);

    // Agreed once and then asked to stop. Both facts are readable, because the
    // exclusion is not a claim that they never agreed — and they are still a
    // customer this tenant invoices.
    let cirrus = person(&body, "quit@cirrus.test");
    assert_eq!(cirrus["mailable"], false);
    assert_eq!(cirrus["exclusionReason"], "suppressed:unsubscribe");
    assert_eq!(cirrus["suppression"]["reason"], "unsubscribe");
    assert!(
        cirrus["consent"]["recordId"].as_str().is_some(),
        "the suppression outranks the consent record; it does not erase it"
    );

    // The count, and every person it leaves out, in the same answer.
    let (status, body) = get(&h.app, &h.token, "/campaigns/audience/tally").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tally"]["mailable"], 2);
    assert_eq!(body["tally"]["matched"], 4);
    assert_eq!(
        exclusions(&body),
        vec![
            ("no_consent".to_owned(), 1),
            ("suppressed:unsubscribe".to_owned(), 1),
        ],
        "the two people the tenant expected to reach and will not"
    );
    // The arithmetic that makes the number auditable: nobody vanishes between
    // the count and its explanation.
    let matched = body["tally"]["matched"].as_i64().unwrap();
    let mailable = body["tally"]["mailable"].as_i64().unwrap();
    assert_eq!(
        matched - mailable,
        exclusions(&body)
            .iter()
            .map(|(_, people)| people)
            .sum::<i64>()
    );
}

#[tokio::test]
async fn a_condition_narrows_the_same_question_on_both_reads() {
    let h = harness("camp-conditions").await;
    four_people(&h).await;

    // Belgium: the two customers placed there. The lead nobody placed is not
    // evidence of being in Belgium, and the Dutch customer is not either.
    let (status, body) = get(&h.app, &h.token, "/campaigns/audience?countries=be").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        addresses(&body),
        vec!["orders@acme.test".to_owned(), "quit@cirrus.test".to_owned()],
        "lowercase still means Belgium, and an unplaced person is not in it"
    );

    let (status, body) = get(&h.app, &h.token, "/campaigns/audience/tally?countries=be").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tally"]["mailable"], 1);
    assert_eq!(body["tally"]["matched"], 2);
    assert_eq!(
        exclusions(&body),
        vec![("suppressed:unsubscribe".to_owned(), 1)],
        "only reasons that excluded somebody are reported"
    );

    // Two countries on one URL. A repeated key would keep the last and quietly
    // narrow the question, which is why they are comma-separated.
    let (status, body) = get(&h.app, &h.token, "/campaigns/audience?countries=BE,NL").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(addresses(&body).len(), 3, "{body}");

    // Nobody in this tenant has bought anything, so "has not bought" is
    // everybody — including the people who will not be mailed.
    let (status, body) = get(
        &h.app,
        &h.token,
        "/campaigns/audience/tally?purchase=not_bought&withinDays=90",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tally"]["matched"], 4);
    assert_eq!(body["tally"]["mailable"], 2);
}

#[tokio::test]
async fn a_question_this_build_cannot_read_is_refused_rather_than_answered_wider() {
    let h = harness("camp-refuse").await;
    four_people(&h).await;

    for uri in [
        // A period with no condition would answer about everybody, and on this
        // surface a wider answer is a bigger send.
        "/campaigns/audience?withinDays=90",
        "/campaigns/audience/tally?withinDays=90",
        "/campaigns/audience?purchase=maybe",
        "/campaigns/audience?purchase=bought&withinDays=ninety",
        // Not a country. Dropping it would widen the segment silently.
        "/campaigns/audience?countries=belgium",
        // A cursor that is not an address would restart the walk for ever.
        "/campaigns/audience?after=page-two",
        "/campaigns/audience?limit=0",
        "/campaigns/audience?limit=501",
        "/campaigns/audience?limit=banana",
        "/campaigns/suppressions?limit=0",
        "/campaigns/consent/not-an-address",
        "/campaigns/suppressions/not-an-address",
    ] {
        let (status, body) = get(&h.app, &h.token, uri).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{uri} was answered instead of refused: {body}"
        );
        assert!(
            body["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "{uri} refused without saying what to fix: {body}"
        );
    }
}

// ---- consent -----------------------------------------------------------------

#[tokio::test]
async fn consent_is_recorded_with_its_provenance_and_can_never_be_edited() {
    let h = harness("camp-consent").await;
    customer(&h.acc, "Acme GmbH", "orders@acme.test", "BE").await;

    // Not a recipient until there is evidence.
    let (_, body) = get(&h.app, &h.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 0);

    let (status, body) = post(
        &h.app,
        &h.token,
        "/campaigns/consent",
        json!({
            "address": " Orders@Acme.TEST ",
            "source": "import",
            "sourceRef": "newsletter-2026.csv",
            "statement": "Opted in on the 2019 shop checkout, migrated 2026",
            "occurredAt": "2026-03-04T10:00:00Z",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["consent"]["address"], "orders@acme.test",
        "consent reaches the person it was given for, however it is spelled"
    );
    assert_eq!(body["consent"]["source"], "import");
    assert_eq!(body["consent"]["sourceRef"], "newsletter-2026.csv");
    assert_eq!(body["consent"]["occurredAt"], "2026-03-04T10:00:00Z");
    assert_eq!(
        body["consent"]["recordedBy"], h.account_id,
        "a consent record says whose workspace made the claim"
    );

    let (status, body) = get(&h.app, &h.token, "/campaigns/consent/ORDERS@acme.test").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["consent"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["consent"][0]["statement"], "Opted in on the 2019 shop checkout, migrated 2026",
        "the history is the answer to 'how do we know', quoted"
    );

    // A second record joins the first rather than replacing it.
    let (status, _) = post(
        &h.app,
        &h.token,
        "/campaigns/consent",
        json!({
            "address": "orders@acme.test",
            "source": "site_form",
            "sourceRef": "form-newsletter",
            "statement": "Re-confirmed through the website form",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, "/campaigns/consent/orders@acme.test").await;
    assert_eq!(
        body["consent"].as_array().map(Vec::len),
        Some(2),
        "evidence is appended, never overwritten: {body}"
    );

    // And the person is now reachable — the gate is the record, not a flag a
    // caller sets.
    let (_, body) = get(&h.app, &h.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 1);
}

#[tokio::test]
async fn a_record_that_is_not_evidence_is_refused_rather_than_stored() {
    let h = harness("camp-consent-bad").await;
    customer(&h.acc, "Acme GmbH", "orders@acme.test", "BE").await;

    for body in [
        json!({ "address": "orders@acme.test", "source": "manual" }),
        json!({ "address": "orders@acme.test", "source": "manual", "statement": "   " }),
        // An import that cannot say where it came from is not provenance.
        json!({ "address": "orders@acme.test", "source": "import", "statement": "They agreed" }),
        json!({ "address": "orders@acme.test", "source": "telepathy", "statement": "They agreed" }),
        json!({ "address": "not-an-address", "source": "manual", "statement": "They agreed" }),
        json!({
            "address": "orders@acme.test",
            "source": "manual",
            "statement": "They agreed",
            "occurredAt": "2026-03-04",
        }),
    ] {
        let (status, answer) = post(&h.app, &h.token, "/campaigns/consent", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body} was accepted: {answer}"
        );
    }

    // A refusal that half-wrote would leave a mailable customer behind it.
    let (_, body) = get(&h.app, &h.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 0, "{body}");
    let (_, body) = get(&h.app, &h.token, "/campaigns/consent/orders@acme.test").await;
    assert_eq!(body["consent"].as_array().map(Vec::len), Some(0));
}

// ---- suppression -------------------------------------------------------------

#[tokio::test]
async fn nothing_on_this_surface_can_lift_a_suppression_or_rewrite_its_reason() {
    let h = harness("camp-suppress").await;
    customer(&h.acc, "Cirrus SA", "quit@cirrus.test", "BE").await;
    agreed(&h.acc, "quit@cirrus.test").await;

    let (status, body) = post(
        &h.app,
        &h.token,
        "/campaigns/suppressions",
        json!({
            "address": " QUIT@Cirrus.test ",
            "reason": "unsubscribe",
            "sourceRef": "one-click",
            "occurredAt": "2026-05-01T09:00:00Z",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["suppression"]["address"], "quit@cirrus.test");
    assert_eq!(body["suppression"]["reason"], "unsubscribe");
    assert_eq!(
        body["suppression"]["personsDecision"], true,
        "an unsubscribe is somebody deciding, which a bounce is not"
    );
    let first = body["suppression"]["id"].as_str().unwrap().to_owned();

    // A hard bounce months later must not rewrite "they asked to stop" into
    // "their mailbox was full". Same record, same reason, same id.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/campaigns/suppressions",
        json!({ "address": "quit@cirrus.test", "reason": "hard_bounce" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "posting a state that already holds is not a conflict: {body}"
    );
    assert_eq!(body["suppression"]["id"], first);
    assert_eq!(body["suppression"]["reason"], "unsubscribe");
    assert_eq!(body["suppression"]["sourceRef"], "one-click");

    let (status, body) = get(&h.app, &h.token, "/campaigns/suppressions").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["suppressions"].as_array().map(Vec::len), Some(1));

    let (status, body) = get(&h.app, &h.token, "/campaigns/suppressions/quit@cirrus.test").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["suppression"]["id"], first);

    // The methods that would undo it are absent, not guarded: a route that
    // exists is a route a bulk importer is eventually pointed at.
    for (method, uri) in [
        ("DELETE", "/campaigns/suppressions/quit@cirrus.test"),
        ("PATCH", "/campaigns/suppressions/quit@cirrus.test"),
        ("DELETE", "/campaigns/consent/quit@cirrus.test"),
        ("PATCH", "/campaigns/consent/quit@cirrus.test"),
    ] {
        let (status, _) = send(
            &h.app,
            request(method, uri, Some(&h.token), Some(json!({}))),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri} exists, and it must not"
        );
    }

    // An import dated today does not resurrect them.
    let (status, _) = post(
        &h.app,
        &h.token,
        "/campaigns/consent",
        json!({
            "address": "quit@cirrus.test",
            "source": "import",
            "sourceRef": "newsletter-2026.csv",
            "statement": "Opted in, per the list we bought",
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the record is kept; it grants nothing"
    );
    let (_, body) = get(&h.app, &h.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 0, "{body}");
    assert_eq!(
        exclusions(&body),
        vec![("suppressed:unsubscribe".to_owned(), 1)]
    );

    // A suppression that could never be applied is refused at the door.
    for body in [
        json!({ "address": "not-an-address", "reason": "unsubscribe" }),
        json!({ "address": "someone@else.test", "reason": "bored" }),
        json!({ "address": "someone@else.test" }),
        json!({
            "address": "someone@else.test",
            "reason": "manual",
            "occurredAt": "2026-05-01",
        }),
    ] {
        let (status, answer) =
            post(&h.app, &h.token, "/campaigns/suppressions", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body} was accepted: {answer}"
        );
    }
    let (_, body) = get(&h.app, &h.token, "/campaigns/suppressions").await;
    assert_eq!(
        body["suppressions"].as_array().map(Vec::len),
        Some(1),
        "a half-written suppression would show up here: {body}"
    );
}

// ---- segments ----------------------------------------------------------------

#[tokio::test]
async fn a_segment_is_saved_read_back_and_counted_through_the_same_conditions() {
    let h = harness("camp-segments").await;
    four_people(&h).await;

    let (status, body) = post(
        &h.app,
        &h.token,
        "/campaigns/segments",
        json!({
            "name": "  Belgian customers  ",
            "conditions": { "countries": ["be"], "purchase": { "condition": "not_bought", "withinDays": 90 } },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["segment"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["segment"]["name"], "Belgian customers");
    assert_eq!(body["segment"]["conditions"]["countries"], json!(["BE"]));
    assert_eq!(
        body["segment"]["conditions"]["purchase"],
        json!({ "condition": "not_bought", "withinDays": 90 })
    );

    // The saved question, read back and counted by the SAME route an unsaved
    // one is counted by — one counting path, so a segment cannot mean one thing
    // while it is being typed and another once it is stored.
    let (status, saved) = get(&h.app, &h.token, &format!("/campaigns/segments/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(
        saved["segment"]["conditions"],
        body["segment"]["conditions"]
    );

    let (status, counted) = get(
        &h.app,
        &h.token,
        "/campaigns/audience/tally?countries=BE&purchase=not_bought&withinDays=90",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["tally"]["matched"], 2);
    assert_eq!(counted["tally"]["mailable"], 1);

    // A duplicate name is a conflict: "send it to the Belgian customers" must
    // name one thing.
    let (status, _) = post(
        &h.app,
        &h.token,
        "/campaigns/segments",
        json!({ "name": "belgian customers" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A rename leaves the question alone; rewriting the conditions replaces
    // them whole.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/campaigns/segments/{id}"),
        json!({ "name": "Belgians we have not sold to" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["segment"]["name"], "Belgians we have not sold to");
    assert_eq!(
        body["segment"]["conditions"], saved["segment"]["conditions"],
        "a rename is not a rewrite"
    );

    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/campaigns/segments/{id}"),
        json!({ "conditions": { "countries": ["NL"] } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["segment"]["conditions"]["countries"], json!(["NL"]));
    assert_eq!(
        body["segment"]["conditions"]["purchase"],
        Value::Null,
        "conditions are one sentence, replaced whole"
    );

    let (status, body) = get(&h.app, &h.token, "/campaigns/segments").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["segments"].as_array().map(Vec::len), Some(1));

    // Forgetting a question never forgets the evidence.
    let (status, _) = delete(&h.app, &h.token, &format!("/campaigns/segments/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&h.app, &h.token, &format!("/campaigns/segments/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = get(&h.app, &h.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 2, "{body}");
}

#[tokio::test]
async fn a_segment_that_would_mean_nothing_is_refused_rather_than_saved() {
    let h = harness("camp-segments-bad").await;

    for body in [
        json!({}),
        json!({ "name": "   " }),
        json!({ "name": "Nowhere", "conditions": { "countries": ["belgium"] } }),
        json!({ "name": "Nowhere", "conditions": { "purchase": { "condition": "maybe" } } }),
        json!({ "name": "Nowhere", "conditions": { "purchase": { "withinDays": 90 } } }),
        json!({
            "name": "Nowhere",
            "conditions": { "purchase": { "condition": "bought", "withinDays": -10 } },
        }),
    ] {
        let (status, answer) = post(&h.app, &h.token, "/campaigns/segments", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body} was saved: {answer}"
        );
    }

    let (_, body) = get(&h.app, &h.token, "/campaigns/segments").await;
    assert_eq!(body["segments"].as_array().map(Vec::len), Some(0));
}

// ---- the boundary ------------------------------------------------------------

#[tokio::test]
async fn a_neighbours_people_evidence_and_segments_are_unreachable_on_every_route() {
    let ours = harness("camp-tenancy-a").await;
    let theirs = harness_on(std::sync::Arc::clone(&ours.store), "camp-tenancy-b").await;

    // Both tenants hold the SAME address, so a leak has to show up as a named
    // extra row rather than as an off-by-one.
    customer(&ours.acc, "Acme GmbH", "orders@acme.test", "BE").await;
    agreed(&ours.acc, "orders@acme.test").await;

    customer(&theirs.acc, "Acme GmbH", "orders@acme.test", "BE").await;
    customer(&theirs.acc, "Their Own BV", "only-theirs@bravo.test", "NL").await;
    agreed(&theirs.acc, "orders@acme.test").await;
    agreed(&theirs.acc, "only-theirs@bravo.test").await;
    // They lose the shared address; we do not. Unsubscribing from one company
    // is not unsubscribing from every company on the platform.
    suppressed(&theirs.ts, "orders@acme.test", SuppressionReason::Complaint).await;

    let (status, body) = post(
        &theirs.app,
        &theirs.token,
        "/campaigns/segments",
        json!({ "name": "Their question", "conditions": { "countries": ["NL"] } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let theirs_segment = body["segment"]["id"].as_str().unwrap().to_owned();

    // Our side, asserted whole.
    let (_, body) = get(&ours.app, &ours.token, "/campaigns/audience").await;
    assert_eq!(addresses(&body), vec!["orders@acme.test".to_owned()]);
    assert_eq!(person(&body, "orders@acme.test")["mailable"], true);
    assert_eq!(
        person(&body, "orders@acme.test")["suppression"],
        Value::Null,
        "their complaint is not our suppression"
    );
    let (_, body) = get(&ours.app, &ours.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 1);
    assert_eq!(body["tally"]["matched"], 1);

    // Their side, asserted whole from their own token.
    let (_, body) = get(&theirs.app, &theirs.token, "/campaigns/audience").await;
    assert_eq!(
        addresses(&body),
        vec![
            "only-theirs@bravo.test".to_owned(),
            "orders@acme.test".to_owned(),
        ]
    );
    let (_, body) = get(&theirs.app, &theirs.token, "/campaigns/audience/tally").await;
    assert_eq!(body["tally"]["mailable"], 1);
    assert_eq!(
        exclusions(&body),
        vec![("suppressed:complaint".to_owned(), 1)]
    );

    // Their person is not in our audience under any question we can ask.
    let (_, body) = get(&ours.app, &ours.token, "/campaigns/audience?countries=NL").await;
    assert_eq!(addresses(&body), Vec::<String>::new());

    // Their evidence is not readable from here — and the answer is the same
    // shape an address nobody has ever heard of gets, so it is no oracle.
    let (status, body) = get(
        &ours.app,
        &ours.token,
        "/campaigns/consent/only-theirs@bravo.test",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["consent"].as_array().map(Vec::len), Some(0));

    let (status, _) = get(
        &ours.app,
        &ours.token,
        "/campaigns/suppressions/orders@acme.test",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "their complaint about a shared address is not ours to read"
    );
    let (_, body) = get(&ours.app, &ours.token, "/campaigns/suppressions").await;
    assert_eq!(body["suppressions"].as_array().map(Vec::len), Some(0));

    // Their segment is invisible, unwritable and undeletable from our handle,
    // and absent from our list — on every verb the route offers.
    let path = format!("/campaigns/segments/{theirs_segment}");
    let (status, _) = get(&ours.app, &ours.token, &path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = patch(&ours.app, &ours.token, &path, json!({ "name": "Mine now" })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = delete(&ours.app, &ours.token, &path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = get(&ours.app, &ours.token, "/campaigns/segments").await;
    assert_eq!(body["segments"].as_array().map(Vec::len), Some(0));

    // And it is still there for its owner — a 404 that had deleted it would be
    // worse than a leak.
    let (status, body) = get(&theirs.app, &theirs.token, &path).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["segment"]["name"], "Their question");
}
