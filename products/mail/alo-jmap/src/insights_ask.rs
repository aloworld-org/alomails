//! Ask-to-chart at the HTTP edge (ADR 0037, wave BI1.07) — `POST
//! /insights/ask`: a sentence in, a **proposed** chart and its preview figures
//! out.
//!
//! Three modules meet here and each keeps its own job:
//!
//! - [`alo_store::insight_prompt`] renders the closed catalog the model may
//!   choose from, generated from the enums the validator enforces.
//! - [`alo_ai::insights`] owns the conversation: the envelope, the one repair
//!   turn, and the strict read of what came back.
//! - **This file decides nothing about charts.** It runs the two turns, hands
//!   whatever the model produced to the same write gate a hand-built spec meets
//!   ([`ChartSpec::from_value`]), and evaluates the survivor through the same
//!   function `POST /insights/eval` uses. There is no path here that stores
//!   anything: a proposal becomes a tile only when a person pins it
//!   (`POST /insights/dashboards/{id}/tiles`), which is what makes this
//!   propose-then-approve (ADR 0034) rather than a model writing to a board.
//!
//! **A spec is not a capability.** The tenant comes from the account door and a
//! ChartSpec has no field that could name one, so the worst a model can produce
//! is a chart of the caller's own rows — or a `422`. It is also told not to
//! filter by record ids, which it could not know; an invented one is refused at
//! evaluation rather than answered with a quietly empty chart.
//!
//! Nothing here logs the question, the model's reply, or a figure. The one log
//! line carries catalog ids and whether a repair was needed — our own code is
//! held to the promise we sell (law #1).

use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use alo_ai::ChatMessage;
use alo_ai::insights::{
    ChartReply, chart_messages, chart_turn, parse_chart_reply, repair_messages,
};
use alo_store::insight_catalog::Viz;
use alo_store::insight_spec::ChartSpec;

use crate::ai::{MAX_ASK_BYTES, ai_problem, tenant_ai_config};
use crate::billing::parse_body;
use crate::error::Problem;
use crate::insights_eval::evaluate;
use crate::state::{AppState, authenticate};

/// The body of `POST /insights/ask`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskBody {
    /// The question, in the user's own language.
    #[serde(default)]
    q: Option<String>,
}

/// The longest question we will carry to a model. A chart question is a
/// sentence; anything longer is a document, and this is not the route for one.
const MAX_QUESTION_CHARS: usize = 500;

/// How wide a proposed chart wants to sit when it is pinned: a single figure is
/// a small card, everything with a breakdown wants room to be read. The same
/// two widths the gallery's own entries use (`insight_overview`).
///
/// Shared with the Insights agent's report tool (`crate::agent_insights`, A2.4)
/// rather than copied into it: a proposal from a room and a proposal from the
/// ask box lay out the same way because they ask the same function.
pub(crate) fn span_for(viz: Viz) -> i16 {
    match viz {
        Viz::Number => 1,
        Viz::Bar | Viz::Line | Viz::Pie | Viz::Table => 2,
    }
}

/// What one model reply came to, once the write gate has had it.
#[derive(Debug)]
enum Attempt {
    /// A chart this server will actually answer.
    Chart(Box<ChartSpec>),
    /// The model said it cannot chart the question, in its own words. Not an
    /// error to correct: correcting a refusal is how a confident wrong chart
    /// gets made.
    CannotChart(String),
    /// The reply was refused, and this is the sentence the repair turn carries.
    Repair(String),
}

/// Reads one model reply through the same write gate a hand-built spec meets.
///
/// [`Attempt::Repair`] carries the *validator's own words*
/// ([`alo_store::insight_spec::SpecError`]): those messages name the offending
/// field and the rule it broke — "the billing.documents dataset does not offer
/// the value measure", "a line runs over time" — which is exactly what a second
/// attempt needs.
fn read_reply(text: &str) -> Attempt {
    match parse_chart_reply(text) {
        Ok(ChartReply::Spec(value)) => match ChartSpec::from_value(value) {
            Ok(spec) => Attempt::Chart(Box::new(spec)),
            Err(error) => Attempt::Repair(error.to_string()),
        },
        Ok(ChartReply::Refused(reason)) => Attempt::CannotChart(reason),
        Err(_) => Attempt::Repair("your reply was not a single JSON object".to_owned()),
    }
}

/// Today, as the model reads dates (`YYYY-MM-DD`), so "last quarter" resolves
/// against a real day rather than whatever the model believes today is.
fn today() -> String {
    OffsetDateTime::now_utc()
        .date()
        .format(&Iso8601::DATE)
        .unwrap_or_else(|_| "1970-01-01".to_owned())
}

/// The two turns, in order: ask, and — only if the first reply was refused by
/// the write gate — repair once carrying that refusal.
///
/// Returns the accepted spec and whether it took the repair, or the sentence to
/// show the user when neither turn produced a chart.
async fn propose(config: &alo_ai::AiConfig, question: &str) -> Result<(ChartSpec, bool), Problem> {
    let messages: Vec<ChatMessage> =
        chart_messages(&alo_store::catalog_prompt(), question, &today());
    let first = chart_turn(config, &messages)
        .await
        .map_err(|e| ai_problem(&e))?;
    let refusal = match read_reply(&first) {
        Attempt::Chart(spec) => return Ok((*spec, false)),
        Attempt::CannotChart(reason) => return Err(cannot_chart(&reason)),
        Attempt::Repair(refusal) => refusal,
    };

    let repaired = repair_messages(&messages, &first, &refusal);
    let second = chart_turn(config, &repaired)
        .await
        .map_err(|e| ai_problem(&e))?;
    match read_reply(&second) {
        Attempt::Chart(spec) => Ok((*spec, true)),
        Attempt::CannotChart(reason) => Err(cannot_chart(&reason)),
        // Twice refused: the user gets the *last* reason, which is the one that
        // survived a correction attempt. Nothing is stored and nothing pinned.
        Attempt::Repair(refusal) => Err(cannot_chart(&refusal)),
    }
}

/// The typed refusal: a `422` the client shows as "we could not build a chart
/// from that", with the reason it was given.
fn cannot_chart(reason: &str) -> Problem {
    Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("no chart could be built from that question: {reason}"),
    )
}

/// `POST /insights/ask` `{q}` → `{spec, viz, span, series, repaired}`.
///
/// Stores nothing. The answer is a proposal: the client renders the series as a
/// preview and, if the reader approves, pins the spec through the ordinary tile
/// route — where the same write gate validates it again, because a proposal
/// that travelled through a browser is just another request.
///
/// Degrades like every other AI route: a tenant with no model configured gets a
/// `503`, and the rest of Insights (the gallery, the boards) is untouched.
pub async fn ask(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_ASK_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "question too large",
        ));
    }
    let req: AskBody = parse_body(&body)?;
    let question = req.q.unwrap_or_default().trim().to_owned();
    if question.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "q is required: there is nothing to chart without a question",
        ));
    }
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("a question may be at most {MAX_QUESTION_CHARS} characters"),
        ));
    }

    let config = tenant_ai_config(&account).await?;
    let started = Instant::now();
    let (spec, repaired) = propose(&config, &question).await?;
    let series = evaluate(&account.acc, &spec).await?;
    tracing::debug!(
        dataset = ?spec.dataset,
        measure = ?spec.measure.id,
        viz = ?spec.viz,
        repaired,
        ms = started.elapsed().as_millis(),
        "insights ask"
    );

    Ok(Json(json!({
        "spec": spec.to_value().map_err(|_| Problem::server_error())?,
        "viz": spec.viz,
        "span": span_for(spec.viz),
        "series": series,
        "repaired": repaired,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::insight_catalog::{Dataset, Measure};

    /// What a well-behaved model replies with for "revenue by month this year".
    const GOOD: &str = r#"{
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "dimension": { "id": "issue_date", "grain": "month" },
        "period": { "kind": "last_n", "n": 12, "grain": "month" },
        "viz": "bar"
    }"#;

    fn accepted(text: &str) -> ChartSpec {
        match read_reply(text) {
            Attempt::Chart(spec) => *spec,
            other => panic!("expected an accepted spec, got {other:?}"),
        }
    }

    fn refused(text: &str) -> String {
        match read_reply(text) {
            Attempt::Repair(refusal) => refusal,
            other => panic!("expected a refusal to repair, got {other:?}"),
        }
    }

    #[test]
    fn a_good_reply_is_read_through_the_same_gate_a_hand_built_spec_is() {
        let spec = accepted(GOOD);
        assert_eq!(spec.dataset, Dataset::BillingDocuments);
        assert_eq!(spec.measure.id, Measure::Net);
        assert_eq!(spec.viz, Viz::Bar);
        // And through a code fence, which is what models actually send.
        let fenced = format!("Here you go:\n```json\n{GOOD}\n```");
        assert_eq!(accepted(&fenced), spec);
    }

    #[test]
    fn an_invented_measure_comes_back_as_the_sentence_the_repair_turn_carries() {
        let invented = GOOD.replace("\"net\"", "\"profit\"");
        let refusal = refused(&invented);
        // The validator's own words: what was wrong, and the vocabulary that
        // was available instead.
        assert!(refusal.contains("profit"), "{refusal}");
        assert!(refusal.contains("net"), "{refusal}");
    }

    #[test]
    fn an_incompatible_pairing_is_refused_with_the_rule_it_broke() {
        // A line over customers: a drawing and a breakdown that cannot agree.
        let crossed = GOOD
            .replace("\"viz\": \"bar\"", "\"viz\": \"line\"")
            .replace(
                "\"dimension\": { \"id\": \"issue_date\", \"grain\": \"month\" }",
                "\"dimension\": { \"id\": \"customer\" }",
            );
        let refusal = refused(&crossed);
        assert!(refusal.contains("over time"), "{refusal}");

        // A number tile with a breakdown.
        let numbered = GOOD.replace("\"viz\": \"bar\"", "\"viz\": \"number\"");
        assert!(refused(&numbered).contains("no breakdown"));

        // A spec from a newer alo than this one.
        let future = GOOD.replace("\"schema_version\": 1", "\"schema_version\": 2");
        assert!(refused(&future).contains("schema_version 2"));
    }

    #[test]
    fn prose_instead_of_json_is_refused_in_words_a_model_can_act_on() {
        let refusal = refused("I think revenue rose last quarter.");
        assert!(refusal.contains("single JSON object"), "{refusal}");
    }

    /// A model that says it cannot chart the question is not repaired — it is
    /// believed, and the user is told in one sentence.
    #[test]
    fn a_stated_refusal_is_an_answer_and_not_something_to_correct() {
        let reason = match read_reply(r#"{"error":"I cannot chart the weather."}"#) {
            Attempt::CannotChart(reason) => reason,
            other => panic!("expected a stated refusal, got {other:?}"),
        };
        assert_eq!(reason, "I cannot chart the weather.");
        let problem = cannot_chart(&reason);
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .unwrap_or_default()
                .contains("I cannot chart the weather."),
        );
    }

    /// The prompt tells the model not to filter by record ids; if one gets
    /// through anyway, it is a refusal at the gate or at evaluation — never a
    /// chart that is quietly empty.
    #[test]
    fn an_invented_record_id_never_becomes_a_silently_empty_chart() {
        let with_id = GOOD.replace(
            "\"viz\": \"bar\"",
            "\"filters\": [{ \"id\": \"customer\", \"op\": \"in\", \"values\": [\"acme\"] }], \
             \"viz\": \"bar\"",
        );
        // Shape-valid, so the gate accepts it here; the tenant's own records
        // decide at evaluation (BI1.03), where an id that is not theirs is a
        // 422. What must never happen is silent acceptance of a wrong shape.
        let spec = accepted(&with_id);
        assert_eq!(spec.filters.len(), 1);

        let sql_shaped = GOOD.replace(
            "\"viz\": \"bar\"",
            "\"filters\": [{ \"id\": \"customer\", \"op\": \"in\", \"values\": [\"1' OR '1'='1\"] }], \
             \"viz\": \"bar\"",
        );
        assert!(refused(&sql_shaped).contains("record ids"));
    }

    #[test]
    fn a_body_without_a_question_is_a_body_with_nothing_to_chart() {
        let empty: AskBody = serde_json::from_str("{}").unwrap_or(AskBody { q: None });
        assert!(empty.q.is_none());
        let asked: AskBody =
            serde_json::from_str(r#"{"q":"revenue by month"}"#).unwrap_or(AskBody { q: None });
        assert_eq!(asked.q.unwrap_or_default(), "revenue by month");
    }

    #[test]
    fn a_single_figure_pins_narrow_and_a_chart_pins_wide() {
        assert_eq!(span_for(Viz::Number), 1);
        for viz in [Viz::Bar, Viz::Line, Viz::Pie, Viz::Table] {
            assert_eq!(span_for(viz), 2, "{viz:?}");
        }
    }

    #[test]
    fn todays_date_is_the_shape_the_prompt_promises() {
        let day = today();
        assert_eq!(day.len(), 10, "{day}");
        assert!(day.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 {
            c == '-'
        } else {
            c.is_ascii_digit()
        }));
    }
}
