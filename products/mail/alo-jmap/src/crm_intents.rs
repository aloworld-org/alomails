//! The executors of alo CRM's verbs (ADR 0058) — what runs when the CRM agent
//! uses one of the intents `alo_ai::crm_intents` describes.
//!
//! Every executor runs through the asker's account door and answers with the
//! **record view** CRM's own routes serve ([`crate::crm_deals::deal_json`],
//! [`crate::crm_deals::event_json`], [`crate::crm_activities::activity_json`],
//! [`crate::crm_reports::report_json`]), so an agent grounds in exactly what a
//! person sees on the board — there is no second summary of a deal. A read
//! returns `{"ok": true, "result": …}` into the turn with money made readable
//! beside its integers ([`crate::billing_intents::ok`], the shared rendering);
//! a write only ever runs from the asker's approval
//! ([`crate::agent::execute_tool`] holds that, not this module).
//!
//! **Resolution is the executor's job, not the model's.** A deal is named by
//! its title, never by a number it does not have; an exact title wins alone,
//! several containing matches are returned for the person to choose, and a
//! write refuses the ambiguity with a sentence rather than picking one
//! ([`crate::agent_crm`], which keeps the three write executors). Nothing here
//! guesses.
//!
//! The reads are new with the move to intents (AA.1): before them, "@crm which
//! deals are open?" had no verb to run and the agent answered from nothing.

use std::collections::HashMap;

use serde_json::{Value, json};
use time::{Date, Month, OffsetDateTime};

use alo_store::crm_deals::{Deal, DealFilter, DealState};
use alo_store::crm_pipelines::Pipeline;
use alo_store::crm_stages::Stage;

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::{map_store_err, parse_iso_date};
use crate::billing_document::today;
use crate::billing_intents::{Reply, ok};
use crate::crm_deals::deal_json;
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many records a list read returns — enough for a question, small enough
/// to sit inside the turn's result window.
const MAX_LISTED: usize = 12;

/// How many of a deal's notes and moves a history read carries per deal.
const MAX_TRAIL: usize = 8;

/// The tenant's boards with their columns (archived columns included, because
/// a closed deal keeps pointing at the column it closed in) — the one lookup
/// every read resolves stage and board names through.
async fn boards(account: &Account) -> Result<Vec<(Pipeline, Vec<Stage>)>, Problem> {
    let pipelines = account
        .acc
        .crm_pipelines(false)
        .await
        .map_err(map_store_err)?;
    let mut out = Vec::with_capacity(pipelines.len());
    for pipeline in pipelines {
        let stages = account
            .acc
            .crm_stages(&pipeline.id, true)
            .await
            .map_err(map_store_err)?;
        out.push((pipeline, stages));
    }
    Ok(out)
}

/// `stage_id` → (stage name, board name), over every board.
fn stage_names(boards: &[(Pipeline, Vec<Stage>)]) -> HashMap<String, (String, String)> {
    let mut names = HashMap::new();
    for (pipeline, stages) in boards {
        for stage in stages {
            names.insert(
                stage.id.as_str().to_owned(),
                (stage.name.clone(), pipeline.name.clone()),
            );
        }
    }
    names
}

/// The record view with the stage and board written in by name — what an agent
/// needs that a screen with the board already open does not.
fn deal_entry(deal: &Deal, names: &HashMap<String, (String, String)>, asker: &str) -> Value {
    let mut entry = deal_json(deal);
    let (stage, pipeline) = match names.get(deal.stage_id.as_str()) {
        Some((stage, pipeline)) => (json!(stage), json!(pipeline)),
        None => (Value::Null, Value::Null),
    };
    if let Some(object) = entry.as_object_mut() {
        object.insert("stageName".to_owned(), stage);
        object.insert("pipelineName".to_owned(), pipeline);
        object.insert("mine".to_owned(), json!(deal.owner_user_id == asker));
    }
    entry
}

/// The deals whose title contains `wanted`, an exact match winning alone.
async fn find_deals(account: &Account, wanted: &str) -> Result<Vec<Deal>, Problem> {
    let wanted = wanted.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(unprocessable("name the deal"));
    }
    let all = account
        .acc
        .crm_deals(&DealFilter::default())
        .await
        .map_err(map_store_err)?;
    let exact: Vec<Deal> = all
        .iter()
        .filter(|d| d.title.to_lowercase() == wanted)
        .cloned()
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    Ok(all
        .into_iter()
        .filter(|d| d.title.to_lowercase().contains(&wanted))
        .collect())
}

/// The deals with one company or contact — matched on the company's name, the
/// contact's name, or the title, so "where are we with Ada?" finds the card
/// whichever field carries her.
async fn deals_with(account: &Account, wanted: &str) -> Result<Vec<Deal>, Problem> {
    let wanted = wanted.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(unprocessable("name the company or the contact"));
    }
    Ok(account
        .acc
        .crm_deals(&DealFilter::default())
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|d| {
            d.company_name.to_lowercase().contains(&wanted)
                || d.contact_name.to_lowercase().contains(&wanted)
                || d.title.to_lowercase().contains(&wanted)
        })
        .collect())
}

/// `open_deals` — the board's open cards, column by column, with a tally per
/// stage and per owner.
pub async fn execute_open_deals(account: &Account, args: &Value) -> Reply {
    let all = boards(account).await?;
    if all.is_empty() {
        return Err(unprocessable(
            "you have no sales board yet — open CRM once and one is made for you",
        ));
    }
    let mut filter = DealFilter {
        state: Some(DealState::Open),
        ..DealFilter::default()
    };
    if let Some(name) = string_arg(args, "pipeline") {
        let picked = pick(
            &name,
            all.iter().map(|(p, _)| (p.name.as_str(), p)).collect(),
            "pipeline",
        )?;
        filter.pipeline_id = Some(picked.id.clone());
    }
    if let Some(name) = string_arg(args, "stage") {
        // Among active columns only — an archived column takes no new work and
        // is not a name a person is asking the open board by.
        let candidates: Vec<(&str, &Stage)> = all
            .iter()
            .filter(|(p, _)| {
                filter
                    .pipeline_id
                    .as_ref()
                    .is_none_or(|id| id.as_str() == p.id.as_str())
            })
            .flat_map(|(_, stages)| stages.iter().filter(|s| s.archived_at.is_none()))
            .map(|s| (s.name.as_str(), s))
            .collect();
        filter.stage_id = Some(pick(&name, candidates, "stage")?.id.clone());
    }
    if let Some(owner) = string_arg(args, "owner") {
        // The asker's own deals are one word; a colleague's are read from the
        // board, which lists every owner — there is no name lookup to guess by.
        if !matches!(owner.trim().to_lowercase().as_str(), "me" | "mine" | "my") {
            return Err(unprocessable(
                "owner filters as \"me\" only — every open deal below names its owner",
            ));
        }
        filter.owner_user_id = Some(account.user.as_str().to_owned());
    }
    let deals = account
        .acc
        .crm_deals(&filter)
        .await
        .map_err(map_store_err)?;
    let names = stage_names(&all);
    let asker = account.user.as_str();
    let listed: Vec<Value> = deals
        .iter()
        .take(MAX_LISTED)
        .map(|d| deal_entry(d, &names, asker))
        .collect();
    // One row per (stage, currency) and per (owner, currency), counted over
    // the whole answer, so the tallies and the list cannot disagree — and a
    // board sold in two currencies is never summed across them.
    let mut by_stage: Vec<Value> = Vec::new();
    let mut by_owner: Vec<Value> = Vec::new();
    let mut stage_tally: HashMap<(String, String), (usize, i64)> = HashMap::new();
    let mut owner_tally: HashMap<(String, String), (usize, i64)> = HashMap::new();
    for deal in &deals {
        let stage = names
            .get(deal.stage_id.as_str())
            .map_or_else(|| deal.stage_id.as_str().to_owned(), |(s, _)| s.clone());
        let s = stage_tally
            .entry((stage, deal.currency.clone()))
            .or_default();
        s.0 += 1;
        s.1 += deal.value_cents;
        let o = owner_tally
            .entry((deal.owner_user_id.clone(), deal.currency.clone()))
            .or_default();
        o.0 += 1;
        o.1 += deal.value_cents;
    }
    // Board order for the stage rows, so the answer reads left to right.
    for (_, stages) in &all {
        for stage in stages {
            let mut currencies: Vec<&(String, String)> = stage_tally
                .keys()
                .filter(|(name, _)| *name == stage.name)
                .collect();
            currencies.sort();
            for key in currencies {
                let (count, value_cents) = stage_tally[key];
                by_stage.push(json!({
                    "stage": key.0,
                    "currency": key.1,
                    "count": count,
                    "valueCents": value_cents,
                }));
            }
        }
    }
    let mut owners: Vec<&(String, String)> = owner_tally.keys().collect();
    owners.sort();
    for key in owners {
        let (count, value_cents) = owner_tally[key];
        by_owner.push(json!({
            "ownerUserId": key.0,
            "mine": key.0 == asker,
            "currency": key.1,
            "count": count,
            "valueCents": value_cents,
        }));
    }
    ok(json!({
        "deals": listed,
        "dealCount": deals.len(),
        "byStage": by_stage,
        "byOwner": by_owner,
        "boards": all.iter().map(|(p, _)| p.name.clone()).collect::<Vec<_>>(),
    }))
}

/// The newest open item, or the newest at all when nothing is open — the deal
/// a company lookup means. Generic because a [`Deal`] cannot be built outside
/// the store, and this rule deserves a pure test.
fn newest<T>(
    mut items: Vec<T>,
    created: impl Fn(&T) -> OffsetDateTime,
    is_open: impl Fn(&T) -> bool,
) -> Option<T> {
    items.sort_by_key(|item| std::cmp::Reverse(created(item)));
    match items.iter().position(is_open) {
        Some(at) => Some(items.remove(at)),
        None => items.into_iter().next(),
    }
}

/// The deal a lookup means when only the company is named.
fn newest_for_company(found: Vec<Deal>) -> Option<Deal> {
    newest(found, |d| d.created_at, |d| d.state() == DealState::Open)
}

/// `deal_lookup` — one deal in full, with its history of moves and its latest
/// notes; or the candidates when the title matches several.
pub async fn execute_deal_lookup(account: &Account, args: &Value) -> Reply {
    let deal = match string_arg(args, "deal").filter(|t| !t.trim().is_empty()) {
        Some(title) => {
            let mut found = find_deals(account, &title).await?;
            match found.len() {
                0 => {
                    return Err(unprocessable(format!(
                        "no deal is titled \"{}\"",
                        title.trim()
                    )));
                }
                1 => found.remove(0),
                _ => {
                    return ok(json!({
                        "deal": Value::Null,
                        "candidates": found.iter().map(deal_json).collect::<Vec<_>>(),
                    }));
                }
            }
        }
        None => {
            let name = string_arg(args, "company")
                .ok_or_else(|| unprocessable("name the deal by its title or by the company"))?;
            let found = deals_with(account, &name).await?;
            newest_for_company(found).ok_or_else(|| {
                unprocessable(format!("there is no deal with \"{}\"", name.trim()))
            })?
        }
    };
    let all = boards(account).await?;
    let names = stage_names(&all);
    let history: Vec<Value> = account
        .acc
        .crm_deal_history(&deal.id)
        .await
        .map_err(map_store_err)?
        .iter()
        .map(|event| {
            let mut entry = crate::crm_deals::event_json(event);
            if let Some(object) = entry.as_object_mut() {
                let stage = names.get(event.to_stage_id.as_str()).map(|(s, _)| s);
                object.insert("toStageName".to_owned(), json!(stage));
            }
            entry
        })
        .collect();
    let activities: Vec<Value> = account
        .acc
        .crm_activities(&deal.id)
        .await
        .map_err(map_store_err)?
        .iter()
        .take(MAX_TRAIL)
        .map(crate::crm_activities::activity_json)
        .collect();
    let mut value = deal_entry(&deal, &names, account.user.as_str());
    if let Some(object) = value.as_object_mut() {
        object.insert("history".to_owned(), Value::Array(history));
        object.insert("activities".to_owned(), Value::Array(activities));
    }
    ok(value)
}

/// The day `pipeline_summary` reads one end of its period from, or the default
/// a bookkeeper means: the year so far.
fn period_day(args: &Value, key: &str, default: Date) -> Result<Date, Problem> {
    match string_arg(args, key).filter(|raw| !raw.trim().is_empty()) {
        None => Ok(default),
        Some(raw) => parse_iso_date(&raw)
            .ok_or_else(|| unprocessable(format!("{key} must be a date, YYYY-MM-DD"))),
    }
}

/// `pipeline_summary` — the board's report: value by column now, won and lost
/// over the period, exactly as `/crm/reports/pipeline` serves it.
pub async fn execute_pipeline_summary(account: &Account, args: &Value) -> Reply {
    let mut pipelines = account
        .acc
        .crm_pipelines(false)
        .await
        .map_err(map_store_err)?;
    if pipelines.is_empty() {
        return Err(unprocessable(
            "you have no sales board yet — open CRM once and one is made for you",
        ));
    }
    let pipeline = match string_arg(args, "pipeline") {
        Some(name) => pick(
            &name,
            pipelines.iter().map(|p| (p.name.as_str(), p)).collect(),
            "pipeline",
        )?
        .clone(),
        None if pipelines.len() > 1 => {
            return Err(unprocessable(format!(
                "you have more than one pipeline: {} — say which",
                pipelines
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        None => pipelines.remove(0),
    };
    let day = today();
    let january_first = Date::from_calendar_date(day.year(), Month::January, 1)
        .map_err(|_| Problem::server_error())?;
    let from = period_day(args, "from", january_first)?;
    let to = period_day(args, "to", day)?;
    if from > to {
        return Err(unprocessable("from is after to"));
    }
    let report = account
        .acc
        .crm_pipeline_report(&pipeline.id, from, to)
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "report": crate::crm_reports::report_json(&report, OffsetDateTime::now_utc()),
    }))
}

/// `company_history` — every deal with one company or contact, each with the
/// latest of what was said and done on it.
pub async fn execute_company_history(account: &Account, args: &Value) -> Reply {
    let name =
        string_arg(args, "company").ok_or_else(|| unprocessable("name the company or contact"))?;
    let found = deals_with(account, &name).await?;
    let all = boards(account).await?;
    let names = stage_names(&all);
    let asker = account.user.as_str();
    let mut listed: Vec<Value> = Vec::new();
    for deal in found.iter().take(MAX_LISTED) {
        let trail: Vec<Value> = account
            .acc
            .crm_activities(&deal.id)
            .await
            .map_err(map_store_err)?
            .iter()
            .take(MAX_TRAIL)
            .map(crate::crm_activities::activity_json)
            .collect();
        let mut entry = deal_entry(deal, &names, asker);
        if let Some(object) = entry.as_object_mut() {
            object.insert("activities".to_owned(), Value::Array(trail));
        }
        listed.push(entry);
    }
    let count_in = |state: DealState| found.iter().filter(|d| d.state() == state).count();
    ok(json!({
        "company": name.trim(),
        "deals": listed,
        "dealCount": found.len(),
        "openCount": count_in(DealState::Open),
        "wonCount": count_in(DealState::Won),
        "lostCount": count_in(DealState::Lost),
    }))
}

/// The module's verbs by name (A4.1c) — CRM's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The three writes keep their executors in [`crate::agent_crm`],
/// and are reached from here so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "open_deals" => Box::pin(execute_open_deals(account, args)),
        "deal_lookup" => Box::pin(execute_deal_lookup(account, args)),
        "pipeline_summary" => Box::pin(execute_pipeline_summary(account, args)),
        "company_history" => Box::pin(execute_company_history(account, args)),
        "create_deal" => Box::pin(crate::agent_crm::execute_create_deal(account, args, state)),
        "move_deal_stage" => Box::pin(crate::agent_crm::execute_move_deal_stage(account, args)),
        "draft_followup" => Box::pin(crate::agent_crm::execute_draft_followup(
            account, args, state,
        )),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_ai::crm_intents::CRM;

    /// Every `/crm/` route the router registers is the adapter of a verb or
    /// excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_crm_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = CRM.uncovered(router, "/crm/");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route the
        // app does not have.
        let routes = alo_ai::routes_in(router, "/crm/");
        for intent in CRM.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("crm_intents.rs");
        for intent in CRM.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// CRM's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("crm_intents::").count(),
            1,
            "agent.rs names CRM only in MODULES"
        );
        assert!(agent.contains("crate::crm_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    #[test]
    fn a_companys_newest_open_deal_wins_over_a_newer_closed_one() {
        use time::Duration;
        let day = |offset: i64| OffsetDateTime::UNIX_EPOCH + Duration::days(offset);
        let pick = |items: Vec<(&'static str, i64, bool)>| {
            newest(items, |item| day(item.1), |item| item.2).map(|item| item.0)
        };
        // An older open card beats a newer closed one…
        assert_eq!(
            pick(vec![
                ("open-old", 0, true),
                ("lost-newest", 9, false),
                ("open-new", 2, true)
            ]),
            Some("open-new")
        );
        // …and with nothing open, the newest card of any state answers.
        assert_eq!(
            pick(vec![("open-old", 0, false), ("lost-newest", 9, false)]),
            Some("lost-newest")
        );
        assert_eq!(pick(Vec::new()), None);
    }
}
