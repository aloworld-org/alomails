//! The executors of alo Insights' verbs (ADR 0058, AC.3) — what runs when the
//! Insights agent uses one of the intents `alo_ai::insights_intents`
//! describes.
//!
//! Every executor runs through the asker's account door. The four verbs the
//! old tool set already had keep their executors in [`crate::agent_insights`]
//! — the figures are that module's subject matter — and are dispatched from
//! here so the agent has one place to look. What this module itself executes
//! is the *boards*: what is already pinned, read as a list, and one more
//! chart pinned to a board that exists.
//!
//! Two seams are deliberate reuse rather than new reach:
//!
//! - **A board reads as the route's own record.** `dashboard_tiles` renders a
//!   board through [`crate::insights::dashboard_json`] — the serializer
//!   `GET /insights/dashboards` answers with — and each tile's question
//!   through [`crate::agent_insights::asked`], the same rendering every
//!   figure travels with, so what the agent says a tile asks and what the
//!   answer said it asked cannot disagree.
//! - **A pinned chart meets the report's own gate.** `pin_chart` reads its
//!   spec through [`crate::agent_insights::spec_arg`] and *evaluates* it
//!   before anything is written — the same validated-and-answered rule
//!   `insight_report` enforces chart by chart — so an approved pin cannot
//!   leave a broken tile on a board colleagues read.
//!
//! Reading the boards goes through the store, not the listing route, on
//! purpose: `GET /insights/dashboards` seeds a first-time tenant with the
//! Business overview, and an agent *reading* must not leave a board behind.
//! A workspace that has never opened Insights honestly has no boards yet.

use serde_json::{Value, json};

use alo_store::insight_dashboards::Dashboard;
use alo_store::insight_tiles::{NewTile, Tile, TileSpec};

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::insights_ask::span_for;
use crate::state::{Account, AppState};

/// The most boards one listing reports. A workspace's tab strip is a handful;
/// past this the tail is named by count so the model knows it is not looking
/// at everything.
const MAX_BOARDS: usize = 12;

type Reply = Result<axum::Json<Value>, Problem>;

fn ok(result: Value) -> Reply {
    Ok(axum::Json(json!({ "ok": true, "result": result })))
}

/// One tile as the agent reads it: the caption, the chart form, and the
/// question it asks in the words the catalog uses — never the raw spec, which
/// is the model's to build, not to quote. An unreadable tile (a spec from a
/// newer build) is shown with its reason, exactly as the board itself shows a
/// placeholder rather than nothing.
fn tile_line(tile: &Tile) -> Value {
    match &tile.spec {
        TileSpec::Readable(spec) => json!({
            "title": tile.title,
            "viz": tile.viz,
            "readable": true,
            "asks": crate::agent_insights::asked(spec),
        }),
        TileSpec::Unreadable { reason, .. } => json!({
            "title": tile.title,
            "viz": tile.viz,
            "readable": false,
            "specError": reason,
        }),
    }
}

/// One board with its tiles in layout order — the route's own record view,
/// with the tile list the agent was asked for beside it.
async fn board_json(account: &Account, board: &Dashboard) -> Result<Value, Problem> {
    let tiles = account
        .acc
        .insight_tiles(&board.id)
        .await
        .map_err(map_store_err)?;
    let mut rendered = crate::insights::dashboard_json(board);
    rendered["tileCount"] = json!(tiles.len());
    rendered["tiles"] = json!(tiles.iter().map(tile_line).collect::<Vec<_>>());
    Ok(rendered)
}

/// The caller's own board by name, case-insensitively — or a refusal that
/// names the boards there are. Only the caller's boards are ever listed in
/// the refusal: the list comes off their own account door, so another
/// tenant's board names cannot appear in it.
fn resolve_board<'a>(boards: &'a [Dashboard], name: &str) -> Result<&'a Dashboard, Problem> {
    let wanted = name.trim().to_lowercase();
    let matches: Vec<&Dashboard> = boards
        .iter()
        .filter(|board| board.name.trim().to_lowercase() == wanted)
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(unprocessable(if boards.is_empty() {
            format!(
                "no board of yours is called {name} — this workspace has no boards yet; \
                 propose one with insight_report"
            )
        } else {
            format!(
                "no board of yours is called {name}; the boards are: {}",
                boards
                    .iter()
                    .map(|board| board.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })),
        _ => Err(unprocessable(format!(
            "more than one of your boards is called {name} — rename one on the board, \
             then ask again"
        ))),
    }
}

/// `dashboard_tiles` — the boards and their pinned questions, as a list.
///
/// # Errors
/// 422 when a named board does not exist (the refusal listing the caller's own
/// boards) or matches more than once; otherwise the store's own failure. An
/// empty workspace is an honest empty list, never an error.
pub async fn execute_dashboard_tiles(account: &Account, args: &Value) -> Reply {
    let boards = account
        .acc
        .insight_dashboards()
        .await
        .map_err(map_store_err)?;
    if let Some(name) = string_arg(args, "board") {
        let board = resolve_board(&boards, &name)?;
        let rendered = board_json(account, board).await?;
        return ok(json!({
            "kind": "dashboardTiles",
            "total": 1,
            "boards": [rendered],
            "truncated": false,
        }));
    }
    let mut rendered = Vec::new();
    for board in boards.iter().take(MAX_BOARDS) {
        rendered.push(board_json(account, board).await?);
    }
    ok(json!({
        "kind": "dashboardTiles",
        "total": boards.len(),
        "boards": rendered,
        "truncated": boards.len() > MAX_BOARDS,
    }))
}

/// `pin_chart` — one more chart on a board that already exists.
///
/// The chart is validated **and evaluated** before the tile is written — the
/// same rule [`crate::agent_insights::execute_insight_report`] holds every
/// chart of a proposed board to — so an approved pin cannot leave somebody
/// looking at a broken tile.
///
/// # Errors
/// 422 when the board, the title or the spec is missing, when no board (or
/// more than one) carries the name, when the spec breaks a catalog rule, or
/// when the evaluation is refused; the store's own error when the board is at
/// its tile ceiling.
pub async fn execute_pin_chart(account: &Account, args: &Value) -> Reply {
    let name = string_arg(args, "board")
        .ok_or_else(|| unprocessable("say which board, by the name its tab shows"))?;
    let title = string_arg(args, "title")
        .ok_or_else(|| unprocessable("the chart needs a title: it is the caption on the board"))?;
    let (raw, spec) = crate::agent_insights::spec_arg(args)?;
    let boards = account
        .acc
        .insight_dashboards()
        .await
        .map_err(map_store_err)?;
    let board = resolve_board(&boards, &name)?;
    // Answered before pinned: the same evaluation the board runs when it is
    // opened, refused now rather than failing on the wall.
    account
        .acc
        .insight_evaluate(&spec)
        .await
        .map_err(|error| unprocessable(format!("{title}: {error}")))?;
    let tile = account
        .acc
        .create_insight_tile(
            &board.id,
            &NewTile {
                title: title.clone(),
                spec: raw,
                span: span_for(spec.viz),
            },
        )
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "chartPinned",
        "board": { "id": board.id.as_str(), "name": board.name },
        "tile": { "id": tile.as_str(), "title": title, "viz": spec.viz },
    }))
}

/// The module's verbs by name (A4.1c) — Insights' one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The four verbs the old tool set
/// already had keep their executors in [`crate::agent_insights`].
pub(crate) fn dispatch<'a>(
    _state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "insight_catalog" => Box::pin(crate::agent_insights::execute_insight_catalog(
            account, args,
        )),
        "insight_answer" => Box::pin(crate::agent_insights::execute_insight_answer(account, args)),
        "insight_change" => Box::pin(crate::agent_insights::execute_insight_change(account, args)),
        "dashboard_tiles" => Box::pin(execute_dashboard_tiles(account, args)),
        "insight_report" => Box::pin(crate::agent_insights::execute_insight_report(account, args)),
        "pin_chart" => Box::pin(execute_pin_chart(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::insights_intents::INSIGHTS;

    /// Every `/insights` route the router registers is the adapter of a verb
    /// or excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_insights_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = INSIGHTS.uncovered(router, "/insights");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route
        // the app does not have.
        let routes = alo_ai::routes_in(router, "/insights");
        for intent in INSIGHTS.intents {
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
        let dispatch = include_str!("insights_intents.rs");
        for intent in INSIGHTS.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Insights' registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, and the two lists are the same
    /// length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("insights_intents::").count(),
            1,
            "agent.rs names Insights only in MODULES"
        );
        assert!(agent.contains("crate::insights_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    /// AC.3's gate, held structurally: a pinned chart is evaluated before the
    /// tile is written — the same answered-before-saved rule the report
    /// executor enforces — so the two writes cannot drift apart on it.
    #[test]
    fn a_pinned_chart_is_answered_before_it_is_written() {
        let source = include_str!("insights_intents.rs");
        let evaluate = source
            .find("insight_evaluate")
            .expect("pin_chart evaluates the spec");
        let write = source
            .find("create_insight_tile")
            .expect("pin_chart writes the tile");
        assert!(
            evaluate < write,
            "the tile is written before the spec is answered"
        );
    }
}
