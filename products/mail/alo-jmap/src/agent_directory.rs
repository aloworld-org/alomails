//! The agent directory (queue item A3.3; ADR 0034 §"an agent in every
//! product") — **what each agent is for, what it may touch, and what it has
//! done**, per tenant.
//!
//! A read surface over three things that already exist, and deliberately not a
//! fourth mechanism:
//!
//! - *what it is for* — the agent's own name and one-line description, which
//!   are tenant data written in the tenant's own language when the default set
//!   was seeded (`crate::chat_agent_names`) and editable afterwards. The
//!   English `alo_ai::agent_product::headline` is **not** used here: it is the
//!   first line of a system prompt, addressed to a model in the second person,
//!   and putting it on this wire would be a hardcoded English string in a
//!   European product's directory.
//! - *what it may touch* — [`alo_ai::tools_for`], the same registry the
//!   execution boundary refuses tools from, plus the rail module the agent is
//!   gated on. So the directory cannot claim a reach the boundary would not
//!   allow: there is one table, and this reads it.
//! - *what it has done* — [`alo_store::AgentRecord`] for the tallies and
//!   `agent_tool_runs` for the individual runs.
//!
//! # What "per tenant" does and does not mean
//!
//! The roster is the tenant's, and every agent in it is filtered by the
//! caller's own module switches exactly as `GET /chat/agents` is (A1.5): an
//! agent of an app this person may not open is absent here, because a directory
//! that described it would be the recall the switch exists to prevent.
//!
//! The **record** is narrower still and on purpose. The tallies count only
//! rooms the caller can see, and the runs are the caller's *own* — a run is an
//! act taken through one person's access, so a colleague reading which diaries
//! and rooms were opened for somebody else would learn from the directory
//! precisely what the access rules withhold. A tenant-wide view of who asked
//! what is an audit surface with an admin gate, and it is not this door.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::{AgentProduct, AgentToolRun, ChatAgent, ChatAgentId};

use crate::chat_agent::agent_json;
use crate::chat_agent_names::agent_seed_for;
use crate::chat_agent_routes::{ListQuery, map_store_err};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// How many of an agent's runs one directory entry carries.
///
/// A window rather than a history: this answers "has it been doing anything,
/// and what kind of thing", and a person who wants the whole log is asking a
/// different question of a different surface.
const RECENT_RUNS: i64 = 20;

/// What an agent may touch, as the registry states it: every tool of its
/// product, each carrying the read/write bit that decides whether its result
/// lands in the room or waits for a tap (ADR 0047 §1).
///
/// The names are the model's own words for the tools, not sentences: they are
/// stable ids a client renders through its own catalogue, which is what keeps
/// this route free of English.
fn tools_json(product: AgentProduct) -> Vec<Value> {
    alo_ai::tools_for(product)
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "effect": tool.effect.as_str(),
            })
        })
        .collect()
}

/// One run, as the directory reports it.
///
/// **No `args`.** What the record has to answer is which tool ran, whether it
/// worked and when — and a tool's arguments carry the very things that must not
/// be repeated onto a new surface: the body of a drafted email, a person's
/// name, the text of a document. Narrower is the whole point of a summary.
fn run_json(run: &AgentToolRun) -> Value {
    json!({
        "id": run.id.as_str(),
        "tool": run.tool,
        "effect": run.effect,
        "ok": run.ok,
        "channel": run.channel.as_ref().map(alo_store::ChatChannelId::as_str),
        "at": run
            .created_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    })
}

/// One directory entry: the agent as the rest of chat spells it, plus what it
/// may touch.
///
/// Built on [`agent_json`] rather than beside it, so the directory and the
/// composer's `@` list can never disagree about an agent's id, name or record.
fn entry_json(agent: &ChatAgent, record: Option<&alo_store::AgentRecord>) -> Value {
    let mut value = agent_json(agent, record);
    if let Some(object) = value.as_object_mut() {
        // The rail switch that decides whether this person has the agent at
        // all — `null` for the two products that have none (mail is the
        // account itself; Ask alo is not a module), and another product's word
        // for the two that are Drive nodes. Named rather than derived
        // client-side, because the client would have to repeat the mapping.
        object.insert(
            "gatedOn".to_owned(),
            match agent.product.module() {
                Some(module) => Value::String(module.as_str().to_owned()),
                None => Value::Null,
            },
        );
        object.insert("tools".to_owned(), Value::Array(tools_json(agent.product)));
    }
    value
}

/// `GET /chat/agents/directory[?lang=nl]` → the tenant's agents, each with what
/// it is for, what it may touch and what it has done.
///
/// Seeds the default set on a tenant's first read, exactly as
/// `GET /chat/agents` does and through the same call, so opening the directory
/// first is not a way to see an empty workspace.
///
/// # Errors
/// 401 unauthenticated.
pub async fn list_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let agents = account
        .acc
        .agents_or_seed(&agent_seed_for(query.lang()))
        .await
        .map_err(map_store_err)?;
    // Best-effort, as the agent list already treats it: a tally that cannot be
    // computed shows as nothing done rather than failing a directory whose
    // other two thirds are fine.
    let records = account.acc.agent_records().await.unwrap_or_default();
    Ok(Json(json!({
        "agents": agents
            .iter()
            .map(|a| entry_json(a, records.get(a.id.as_str())))
            .collect::<Vec<_>>()
    })))
}

/// `GET /chat/agents/{id}/directory` → one agent's entry, with the runs behind
/// its tallies.
///
/// A route of its own rather than `recent` on every entry in the roster: the
/// list is drawn for seventeen agents at once, and seventeen windows of runs is
/// a page of somebody's history nobody asked for.
///
/// # Errors
/// 404 when this tenant has no such agent, **or it belongs to a module this
/// caller may not open** — the same answer an id that was never issued gets, so
/// the refusal is not an oracle for which apps a colleague has.
pub async fn agent_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatAgentId::new(id);
    // The gate, asked once and before anything else is read.
    let agent = account.acc.agent(&id).await.map_err(map_store_err)?;
    let records = account.acc.agent_records().await.unwrap_or_default();
    let runs = account
        .acc
        .agent_tool_runs_for(&id, RECENT_RUNS)
        .await
        .map_err(map_store_err)?;
    let mut value = entry_json(&agent, records.get(agent.id.as_str()));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "recent".to_owned(),
            Value::Array(runs.iter().map(run_json).collect()),
        );
    }
    Ok(Json(value))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::ALL_AGENT_PRODUCTS;

    /// The directory's account of what an agent may touch is the registry's,
    /// for every product — not a list written out here that could fall behind
    /// the boundary that actually refuses tools.
    #[test]
    fn what_the_directory_says_an_agent_may_touch_is_what_the_boundary_allows() {
        for product in ALL_AGENT_PRODUCTS {
            let listed = tools_json(product);
            assert_eq!(listed.len(), alo_ai::tools_for(product).len(), "{product}");
            for entry in &listed {
                let name = entry["name"].as_str().unwrap();
                assert!(
                    alo_ai::offers(product, name),
                    "{product} is described as reaching {name} and would be refused it"
                );
                assert!(
                    matches!(entry["effect"].as_str(), Some("read" | "write")),
                    "{product}/{name} has no effect"
                );
            }
            // …and nothing another product owns is described here.
            for other in ALL_AGENT_PRODUCTS {
                if other == product || product == AgentProduct::Workspace {
                    continue;
                }
                for tool in alo_ai::tools_for(other) {
                    if alo_ai::offers(product, tool.name) {
                        continue;
                    }
                    assert!(
                        !listed.iter().any(|e| e["name"] == tool.name),
                        "{product} is described as reaching {}'s {}",
                        other,
                        tool.name
                    );
                }
            }
        }
        // Ask alo is the one agent whose directory entry is every tool, which
        // is a decision rather than an omission (ADR 0034).
        assert_eq!(
            tools_json(AgentProduct::Workspace).len(),
            alo_ai::tools_for(AgentProduct::Workspace).len()
        );
        assert!(!tools_json(AgentProduct::Mail).is_empty());
    }
}
