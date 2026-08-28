//! The `/crm/*` HTTP surface (B2.04) — pipelines, stages and deals driven
//! through the real router over a real Postgres.
//!
//! What this suite is for: `alo-store`'s own suites prove the records work, so
//! what matters here is the **edge**. The auth guard on every route, the status
//! codes `docs/design/crm.md` publishes, the first-use seed that hands a tenant
//! a working board in its own language, the separation the surface exists to
//! enforce (an edit cannot move a card, a drag cannot rename a column), and
//! above all that a board, a column or a deal belonging to another tenant is
//! invisible and untouchable on every verb, answering exactly as an id that
//! never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{harness, send};

// ---- request helpers ---------------------------------------------------------

fn with_json(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("POST", uri, Some(token), body)).await
}

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PATCH", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// The id of a created resource, or a panic naming the status that came back
/// instead — a failed create otherwise shows up as a confusing later failure.
fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} id in {body}"))
        .to_owned()
}

fn field(body: &Value, kind: &str, key: &str) -> Vec<String> {
    body[kind]
        .as_array()
        .unwrap_or_else(|| panic!("no {kind} array in {body}"))
        .iter()
        .map(|v| v[key].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// The seeded board of a tenant, with its columns — the state every arc below
/// starts from, because it is the state a real tenant starts from.
struct Board {
    pipeline: String,
    stages: Vec<Value>,
}

impl Board {
    /// The id of the column with this header.
    fn stage(&self, name: &str) -> String {
        self.stages
            .iter()
            .find(|s| s["name"] == name)
            .unwrap_or_else(|| panic!("no stage {name} in {:?}", self.stages))["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

async fn seeded_board(app: &Router, token: &str, lang: &str) -> Board {
    let (status, body) = get(app, token, &format!("/crm/pipelines?lang={lang}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pipeline = body["pipelines"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no seeded pipeline in {body}"))
        .to_owned();
    let (status, body) = get(app, token, &format!("/crm/pipelines/{pipeline}/stages")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    Board {
        pipeline,
        stages: body["stages"].as_array().cloned().unwrap_or_default(),
    }
}

fn lead(pipeline: &str, stage: &str) -> Value {
    json!({
        "pipelineId": pipeline,
        "stageId": stage,
        "title": "  Renewal — Acme GmbH  ",
        "companyName": "Acme GmbH",
        "contactName": "Ada",
        "contactEmail": "ada@acme.test",
        "valueCents": 250_000,
        "currency": "eur",
        "expectedClose": "2026-09-30",
        "source": "Referral",
    })
}

// ---- the seed ----------------------------------------------------------------

#[tokio::test]
async fn the_first_read_seeds_one_board_in_the_callers_language() {
    let h = harness("crm-seed").await;

    let (status, body) = get(&h.app, &h.token, "/crm/pipelines?lang=nl-BE").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        field(&body, "pipelines", "name"),
        vec!["Verkoop".to_owned()]
    );

    let pipeline = body["pipelines"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{pipeline}/stages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        field(&body, "stages", "name"),
        vec![
            "Nieuw".to_owned(),
            "Gekwalificeerd".to_owned(),
            "Voorstel".to_owned(),
            "Gewonnen".to_owned(),
            "Verloren".to_owned(),
        ],
        "five columns, left to right"
    );
    let stages = body["stages"].as_array().unwrap();
    assert_eq!(stages[3]["isWon"], true);
    assert_eq!(stages[4]["isLost"], true);
    assert!(stages.iter().all(|s| s["archived"] == false));

    // Seeding is a first-use rule, not an every-read one: a second read in
    // another language returns the board the tenant already has.
    let (status, body) = get(&h.app, &h.token, "/crm/pipelines?lang=fr").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        field(&body, "pipelines", "name"),
        vec!["Verkoop".to_owned()]
    );
}

// ---- the pipeline arc --------------------------------------------------------

#[tokio::test]
async fn pipeline_arc_creates_renames_and_archives() {
    let h = harness("crm-pipe-arc").await;
    let board = seeded_board(&h.app, &h.token, "en").await;

    let id = created_id(
        "pipeline",
        post(
            &h.app,
            &h.token,
            "/crm/pipelines",
            json!({ "name": "  Renewals  ", "description": "Contracts up for renewal" }),
        )
        .await,
    );

    let (status, body) = get(&h.app, &h.token, &format!("/crm/pipelines/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["pipeline"]["name"], "Renewals", "trimmed by the store");
    assert_eq!(body["pipeline"]["archived"], false);

    // A board built by hand starts empty — only the seed hands one over ready.
    let (status, body) = get(&h.app, &h.token, &format!("/crm/pipelines/{id}/stages")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stages"].as_array().map(Vec::len), Some(0));

    // A rename leaves the description alone.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{id}"),
        json!({ "name": "Renewals 2027" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["pipeline"]["name"], "Renewals 2027");
    assert_eq!(body["pipeline"]["description"], "Contracts up for renewal");

    // Two active boards may not share a name.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/crm/pipelines",
        json!({ "name": "renewals 2027" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Archive, and it leaves the tabs but stays readable by id.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{id}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["pipeline"]["archived"], true);
    assert!(body["pipeline"]["archivedAt"].is_string());

    let (_, body) = get(&h.app, &h.token, "/crm/pipelines").await;
    assert_eq!(
        field(&body, "pipelines", "id"),
        vec![board.pipeline.clone()]
    );
    let (_, body) = get(&h.app, &h.token, "/crm/pipelines?includeArchived=1").await;
    assert_eq!(body["pipelines"].as_array().map(Vec::len), Some(2));

    // Restoring is the same route with `archived: false`.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{id}/archive"),
        json!({ "archived": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["pipeline"]["archived"], false);
}

#[tokio::test]
async fn a_board_with_open_work_on_it_cannot_be_archived() {
    let h = harness("crm-pipe-open").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let deal = created_id(
        "deal",
        post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await,
    );

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{}/archive", board.pipeline),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Closing the work releases the board.
    let won = board.stage("Won");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": won }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{}/archive", board.pipeline),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

// ---- the stage arc -----------------------------------------------------------

#[tokio::test]
async fn stage_arc_appends_renames_moves_archives_and_deletes() {
    let h = harness("crm-stage-arc").await;
    let board = seeded_board(&h.app, &h.token, "en").await;

    let id = created_id(
        "stage",
        post(
            &h.app,
            &h.token,
            &format!("/crm/pipelines/{}/stages", board.pipeline),
            json!({ "name": "  Negotiation  " }),
        )
        .await,
    );
    let (status, body) = get(&h.app, &h.token, &format!("/crm/stages/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"]["name"], "Negotiation");
    assert_eq!(body["stage"]["pipelineId"], board.pipeline);
    assert_eq!(body["stage"]["closed"], false);
    let appended = body["stage"]["position"].as_f64().unwrap_or_default();
    assert!(appended > 5.0, "appended to the right-hand end: {appended}");

    // A second winning column is refused by name of the rule.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{}/stages", board.pipeline),
        json!({ "name": "Signed", "isWon": true }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap_or_default().contains("won"),
        "{body}"
    );

    // A drag moves the column and nothing else…
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/stages/{id}/move"),
        json!({ "position": 2.5 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"]["position"], 2.5);
    assert_eq!(body["stage"]["name"], "Negotiation");

    // …and a move that does not say where is not a move.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/stages/{id}/move"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // An edit renames and re-flags, and cannot reorder.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/crm/stages/{id}"),
        json!({ "name": "Verhandlung", "position": 99.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"]["name"], "Verhandlung");
    assert_eq!(body["stage"]["position"], 2.5, "PATCH cannot reorder");

    // The column sorts where the drag put it.
    let (_, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{}/stages", board.pipeline),
    )
    .await;
    assert_eq!(field(&body, "stages", "name")[2], "Verhandlung");

    // A column nothing has ever named can be deleted outright.
    let (status, body) = delete(&h.app, &h.token, &format!("/crm/stages/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    let (status, _) = get(&h.app, &h.token, &format!("/crm/stages/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_column_the_past_has_named_is_archived_never_deleted() {
    let h = harness("crm-stage-guard").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let qualified = board.stage("Qualified");
    let deal = created_id(
        "deal",
        post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await,
    );

    // Open work stands in it: neither archiving nor deleting is allowed.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/stages/{new}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = delete(&h.app, &h.token, &format!("/crm/stages/{new}")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Move the card on: the column is now empty, so it may be archived — but
    // its history row still names it, so it may never be deleted.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": qualified }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = delete(&h.app, &h.token, &format!("/crm/stages/{new}")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/stages/{new}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"]["archived"], true);

    // An archived column takes no new cards.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": new }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

// ---- the deal arc ------------------------------------------------------------

#[tokio::test]
async fn deal_arc_raises_edits_moves_closes_and_reopens() {
    let h = harness("crm-deal-arc").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let qualified = board.stage("Qualified");
    let won = board.stage("Won");
    let lost = board.stage("Lost");

    let (status, body) = post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deal = body["deal"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["deal"]["title"], "Renewal — Acme GmbH");
    assert_eq!(body["deal"]["valueCents"], 250_000);
    assert_eq!(body["deal"]["currency"], "EUR", "normalised by the store");
    assert_eq!(body["deal"]["expectedClose"], "2026-09-30");
    assert_eq!(body["deal"]["state"], "open", "a new deal was never won");
    assert_eq!(body["deal"]["ownerUserId"], h.account_id);
    assert!(body["deal"]["closedAt"].is_null());

    // The first history row is the creation, and it comes from nowhere.
    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/history")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["events"].as_array().map(Vec::len), Some(1));
    assert!(body["events"][0]["fromStageId"].is_null());
    assert_eq!(body["events"][0]["toStageId"], new);

    // An edit merges, and cannot move or close the card.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}"),
        json!({ "valueCents": 300_000, "stageId": won, "state": "won" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["valueCents"], 300_000);
    assert_eq!(body["deal"]["stageId"], new, "PATCH cannot move a card");
    assert_eq!(body["deal"]["state"], "open");
    assert_eq!(body["deal"]["title"], "Renewal — Acme GmbH");

    // A move writes exactly one history row.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": qualified }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["stageId"], qualified);
    let (_, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/history")).await;
    assert_eq!(body["events"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["events"][1]["fromStageId"], new);
    assert_eq!(body["events"][1]["movedBy"], h.account_id);

    // A reposition inside one column writes none.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": qualified, "position": 0.5 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["position"], 0.5);
    let (_, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/history")).await;
    assert_eq!(
        body["events"].as_array().map(Vec::len),
        Some(2),
        "a drag up its own column is not a move"
    );

    // Winning it stamps the snapshot.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": won }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["state"], "won");
    assert_eq!(body["deal"]["closed"], true);
    assert!(body["deal"]["closedAt"].is_string());
    assert!(body["deal"]["lostReason"].is_null());

    // A losing column demands a reason; every other column refuses one.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": lost }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": qualified, "lostReason": "Price" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": lost, "lostReason": "  Price  " }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["state"], "lost");
    assert_eq!(body["deal"]["lostReason"], "Price");

    // Reopening clears the snapshot and leaves every event standing.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": qualified }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["state"], "open");
    assert!(body["deal"]["closedAt"].is_null());
    assert!(body["deal"]["lostReason"].is_null());
    let (_, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/history")).await;
    assert_eq!(body["events"].as_array().map(Vec::len), Some(5));

    // A deal is deleted, not archived, and its history goes with it.
    let (status, body) = delete(&h.app, &h.token, &format!("/crm/deals/{deal}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    let (status, _) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/history")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_list_filters_are_exact_and_never_widen_silently() {
    let h = harness("crm-deal-filter").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let won = board.stage("Won");

    let open = created_id(
        "deal",
        post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await,
    );
    let closed = created_id(
        "deal",
        post(
            &h.app,
            &h.token,
            "/crm/deals",
            json!({ "pipelineId": board.pipeline, "stageId": new, "title": "Won one" }),
        )
        .await,
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{closed}/stage"),
        json!({ "stageId": won }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&h.app, &h.token, "/crm/deals").await;
    assert_eq!(body["deals"].as_array().map(Vec::len), Some(2));
    let (_, body) = get(&h.app, &h.token, "/crm/deals?state=open").await;
    assert_eq!(field(&body, "deals", "id"), vec![open.clone()]);
    let (_, body) = get(&h.app, &h.token, "/crm/deals?state=WON").await;
    assert_eq!(field(&body, "deals", "id"), vec![closed.clone()]);
    let (_, body) = get(&h.app, &h.token, &format!("/crm/deals?stageId={new}")).await;
    assert_eq!(field(&body, "deals", "id"), vec![open.clone()]);
    let (_, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/deals?pipelineId={}&state=", board.pipeline),
    )
    .await;
    assert_eq!(
        body["deals"].as_array().map(Vec::len),
        Some(2),
        "blank is no filter"
    );
    let (_, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/deals?ownerUserId={}", h.account_id),
    )
    .await;
    assert_eq!(body["deals"].as_array().map(Vec::len), Some(2));
    let (_, body) = get(&h.app, &h.token, "/crm/deals?ownerUserId=nobody").await;
    assert_eq!(
        body["deals"].as_array().map(Vec::len),
        Some(0),
        "an owner who owns nothing is an empty list, not an error"
    );

    // A filter that is not recognised is a 422, never a wider list.
    for bad in [
        "state=winning".to_owned(),
        "pipelineId=pip_nope".to_owned(),
        "stageId=stg_nope".to_owned(),
    ] {
        let (status, body) = get(&h.app, &h.token, &format!("/crm/deals?{bad}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
}

#[tokio::test]
async fn a_request_that_cannot_be_acted_on_names_the_rule_it_broke() {
    let h = harness("crm-422").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let other = created_id(
        "pipeline",
        post(
            &h.app,
            &h.token,
            "/crm/pipelines",
            json!({ "name": "Renewals" }),
        )
        .await,
    );
    let other_stage = created_id(
        "stage",
        post(
            &h.app,
            &h.token,
            &format!("/crm/pipelines/{other}/stages"),
            json!({ "name": "Open" }),
        )
        .await,
    );

    let cases: Vec<(&str, String, Value, &str)> = vec![
        (
            "POST",
            "/crm/pipelines".to_owned(),
            json!({ "name": "  " }),
            "name",
        ),
        (
            "POST",
            format!("/crm/pipelines/{}/stages", board.pipeline),
            json!({ "name": "Both", "isWon": true, "isLost": true }),
            "both",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "stageId": new, "title": "No board" }),
            "pipelineId",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "title": "No column" }),
            "stageId",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "stageId": new, "title": "  " }),
            "title",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "stageId": new, "title": "Cheap", "valueCents": -1 }),
            "deal value",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "stageId": new, "title": "Odd", "currency": "EURO" }),
            "currency",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "stageId": new, "title": "Someday", "expectedClose": "31/12/2026" }),
            "expectedClose",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "stageId": new, "title": "Theirs", "ownerUserId": "usr_nobody" }),
            "owner",
        ),
        (
            "POST",
            "/crm/deals".to_owned(),
            json!({ "pipelineId": board.pipeline, "stageId": other_stage, "title": "Wrong board" }),
            "pipeline",
        ),
    ];
    for (method, uri, body, rule) in cases {
        let (status, answer) = send(&h.app, with_json(method, &uri, Some(&h.token), body)).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{method} {uri}: {answer}"
        );
        let detail = answer["detail"].as_str().unwrap_or_default().to_owned();
        assert!(
            detail.to_lowercase().contains(&rule.to_lowercase()),
            "{method} {uri}: expected the rule {rule:?}, got {detail:?}"
        );
    }

    // A deal cannot be moved onto another board's column either.
    let deal = created_id(
        "deal",
        post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": other_stage }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn a_malformed_body_is_a_400_that_never_quotes_the_body() {
    let h = harness("crm-400").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let deal = created_id(
        "deal",
        post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await,
    );

    let cases: Vec<(&str, String, &str)> = vec![
        ("POST", "/crm/pipelines".to_owned(), "{not json"),
        (
            "POST",
            "/crm/deals".to_owned(),
            r#"{"pipelineId":"p","stageId":"s","valueCents":1250.5}"#,
        ),
        (
            "POST",
            format!("/crm/pipelines/{}/stages", board.pipeline),
            r#"{"name":"Open","isWon":"yes"}"#,
        ),
        (
            "POST",
            format!("/crm/deals/{deal}/stage"),
            r#"{"stageId":7}"#,
        ),
    ];
    for (method, uri, raw) in cases {
        let req = Request::builder()
            .method(method)
            .uri(&uri)
            .header("authorization", format!("Bearer {}", h.token))
            .header("content-type", "application/json")
            .body(Body::from(raw.to_owned()))
            .unwrap();
        let (status, body) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {uri}: {body}");
        assert_eq!(body["detail"], "malformed request body");
    }
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn every_crm_route_refuses_an_unauthenticated_caller() {
    let h = harness("crm-401").await;
    let board = seeded_board(&h.app, &h.token, "en").await;
    let new = board.stage("New");
    let deal = created_id(
        "deal",
        post(&h.app, &h.token, "/crm/deals", lead(&board.pipeline, &new)).await,
    );
    let pipeline = board.pipeline.clone();

    let mut unauthenticated: Vec<Request<Body>> = vec![
        with_json("POST", "/crm/pipelines", None, json!({ "name": "Theirs" })),
        with_json(
            "PATCH",
            &format!("/crm/pipelines/{pipeline}"),
            None,
            json!({ "name": "Theirs" }),
        ),
        with_json(
            "POST",
            &format!("/crm/pipelines/{pipeline}/archive"),
            None,
            json!({}),
        ),
        with_json(
            "POST",
            &format!("/crm/pipelines/{pipeline}/stages"),
            None,
            json!({ "name": "Theirs" }),
        ),
        with_json(
            "PATCH",
            &format!("/crm/stages/{new}"),
            None,
            json!({ "name": "Theirs" }),
        ),
        with_json(
            "POST",
            &format!("/crm/stages/{new}/move"),
            None,
            json!({ "position": 1.0 }),
        ),
        with_json(
            "POST",
            &format!("/crm/stages/{new}/archive"),
            None,
            json!({}),
        ),
        with_json("POST", "/crm/deals", None, lead(&pipeline, &new)),
        with_json(
            "PATCH",
            &format!("/crm/deals/{deal}"),
            None,
            json!({ "title": "Theirs" }),
        ),
        with_json(
            "POST",
            &format!("/crm/deals/{deal}/stage"),
            None,
            json!({ "stageId": new }),
        ),
    ];
    for uri in [
        "/crm/pipelines".to_owned(),
        format!("/crm/pipelines/{pipeline}"),
        format!("/crm/pipelines/{pipeline}/stages"),
        format!("/crm/stages/{new}"),
        "/crm/deals".to_owned(),
        format!("/crm/deals/{deal}"),
        format!("/crm/deals/{deal}/history"),
    ] {
        unauthenticated.push(Request::builder().uri(uri).body(Body::empty()).unwrap());
    }
    for uri in [format!("/crm/stages/{new}"), format!("/crm/deals/{deal}")] {
        unauthenticated.push(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        );
    }

    for req in unauthenticated {
        let uri = req.uri().to_string();
        let method = req.method().to_string();
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    // Everything it could not see is exactly as it was.
    let (_, body) = get(&h.app, &h.token, "/crm/deals").await;
    assert_eq!(field(&body, "deals", "id"), vec![deal]);
    let (_, body) = get(&h.app, &h.token, "/crm/pipelines").await;
    assert_eq!(field(&body, "pipelines", "id"), vec![pipeline]);
}

#[tokio::test]
async fn another_tenants_board_column_and_deal_are_invisible_on_every_verb() {
    let a = harness("crm-tenant-a").await;
    let b = harness("crm-tenant-b").await;

    let b_board = seeded_board(&b.app, &b.token, "en").await;
    let b_new = b_board.stage("New");
    let b_deal = created_id(
        "deal",
        post(
            &b.app,
            &b.token,
            "/crm/deals",
            lead(&b_board.pipeline, &b_new),
        )
        .await,
    );

    // A's own board is seeded independently, and A's lists never show B's work.
    let a_board = seeded_board(&a.app, &a.token, "en").await;
    assert_ne!(a_board.pipeline, b_board.pipeline);
    let (_, body) = get(&a.app, &a.token, "/crm/pipelines?includeArchived=1").await;
    assert_eq!(
        field(&body, "pipelines", "id"),
        vec![a_board.pipeline.clone()]
    );
    let (_, body) = get(&a.app, &a.token, "/crm/deals").await;
    assert_eq!(body["deals"].as_array().map(Vec::len), Some(0));

    // Every verb on B's ids answers A with the same 404 an invented id gets.
    let invented_pipeline = "pip_does_not_exist";
    let invented_stage = "stg_does_not_exist";
    let invented_deal = "deal_does_not_exist";
    let attempts: Vec<(&str, String, Option<Value>)> = vec![
        ("GET", format!("/crm/pipelines/{}", b_board.pipeline), None),
        (
            "PATCH",
            format!("/crm/pipelines/{}", b_board.pipeline),
            Some(json!({ "name": "Taken Over" })),
        ),
        (
            "POST",
            format!("/crm/pipelines/{}/archive", b_board.pipeline),
            Some(json!({ "archived": true })),
        ),
        (
            "GET",
            format!("/crm/pipelines/{}/stages", b_board.pipeline),
            None,
        ),
        (
            "POST",
            format!("/crm/pipelines/{}/stages", b_board.pipeline),
            Some(json!({ "name": "Ours now" })),
        ),
        ("GET", format!("/crm/stages/{b_new}"), None),
        (
            "PATCH",
            format!("/crm/stages/{b_new}"),
            Some(json!({ "name": "Taken Over" })),
        ),
        (
            "POST",
            format!("/crm/stages/{b_new}/move"),
            Some(json!({ "position": 99.0 })),
        ),
        (
            "POST",
            format!("/crm/stages/{b_new}/archive"),
            Some(json!({ "archived": true })),
        ),
        ("DELETE", format!("/crm/stages/{b_new}"), None),
        ("GET", format!("/crm/deals/{b_deal}"), None),
        (
            "PATCH",
            format!("/crm/deals/{b_deal}"),
            Some(json!({ "title": "Taken Over" })),
        ),
        (
            "POST",
            format!("/crm/deals/{b_deal}/stage"),
            Some(json!({ "stageId": b_new })),
        ),
        ("GET", format!("/crm/deals/{b_deal}/history"), None),
        ("DELETE", format!("/crm/deals/{b_deal}"), None),
        // The same verbs against ids that never existed anywhere: the answers
        // must be indistinguishable, or the surface is an existence oracle.
        ("GET", format!("/crm/pipelines/{invented_pipeline}"), None),
        (
            "GET",
            format!("/crm/pipelines/{invented_pipeline}/stages"),
            None,
        ),
        ("GET", format!("/crm/stages/{invented_stage}"), None),
        ("DELETE", format!("/crm/stages/{invented_stage}"), None),
        ("GET", format!("/crm/deals/{invented_deal}"), None),
        ("GET", format!("/crm/deals/{invented_deal}/history"), None),
        ("DELETE", format!("/crm/deals/{invented_deal}"), None),
    ];
    for (method, uri, body) in attempts {
        let req = match body {
            Some(json) => with_json(method, &uri, Some(&a.token), json),
            None => Request::builder()
                .method(method)
                .uri(&uri)
                .header("authorization", format!("Bearer {}", a.token))
                .body(Body::empty())
                .unwrap(),
        };
        let (status, answer) = send(&a.app, req).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {answer}");
    }

    // Raising a deal on B's board — naming B's column — is refused too.
    let (status, body) = post(
        &a.app,
        &a.token,
        "/crm/deals",
        lead(&b_board.pipeline, &b_new),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // And B's ids are not usable as filters: the same 422 an invented id gets,
    // so the filter cannot be used to ask whether a board exists elsewhere.
    for query in [
        format!("pipelineId={}", b_board.pipeline),
        format!("stageId={b_new}"),
        format!("pipelineId={invented_pipeline}"),
        format!("stageId={invented_stage}"),
    ] {
        let (status, body) = get(&a.app, &a.token, &format!("/crm/deals?{query}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
    }

    // B's records are untouched by every one of those attempts.
    let (_, body) = get(&b.app, &b.token, &format!("/crm/deals/{b_deal}")).await;
    assert_eq!(body["deal"]["title"], "Renewal — Acme GmbH");
    assert_eq!(body["deal"]["stageId"], b_new);
    let (_, body) = get(&b.app, &b.token, &format!("/crm/stages/{b_new}")).await;
    assert_eq!(body["stage"]["name"], "New");
    assert_eq!(body["stage"]["archived"], false);
    let (_, body) = get(&b.app, &b.token, &format!("/crm/deals/{b_deal}/history")).await;
    assert_eq!(body["events"].as_array().map(Vec::len), Some(1));
}
