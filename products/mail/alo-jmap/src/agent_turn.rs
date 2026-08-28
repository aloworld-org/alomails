//! One agent turn, with its reading tools run **inside it** (ADR 0047).
//!
//! Before this, every tool the model chose came back as a proposal, reads
//! included: `@inventory is the X100 in stock?` answered with a sentence about
//! what it was willing to look up, and the stock figure arrived only after a
//! tap. That was never a safety property — it was one envelope and one code
//! path, grown into a rule.
//!
//! So a turn is a small loop rather than a single call: ask, and if what comes
//! back is a **read**, run it, put its result among the sources, and ask again
//! so the answer is cited to the record instead of to a search snippet. A
//! **write** still comes back as a proposal and is the caller's to record.
//!
//! Two bounds hold it down, and both are here rather than in the prompt:
//!
//! - **At most [`MAX_READS`] read executions per turn.** A confused or injected
//!   turn cannot spend a workspace's inference budget going round; on the
//!   seventh it answers with what it has and says so.
//! - **A write is refused at the execution boundary**, not asked not to happen.
//!   [`crate::agent::execute_tool`] is given [`Approval::InTurn`] here, and that
//!   is what makes "reads only" true no matter what the model returns.
//!
//! Everything runs through the **asker's** account door, exactly as the
//! retrieval that grounds the turn already did. A read's blast radius under
//! prompt injection is therefore the one workspace search already had: a
//! hostile message can make an agent look up something the person who
//! triggered the turn could already look up, and nothing further.

use alo_ai::{AgentAsk, AgentDecision, AiConfig, InferenceError, WorkspaceSource};
use alo_store::{AgentProduct, ChatAgentId, ChatChannelId};

use crate::agent::{Approval, ToolRun, execute_tool};
use crate::state::{Account, AppState};

/// How many reading tools one turn may run (ADR 0047 §2, raised by ADR 0058:
/// "what did we quote Northstar" is customer → quotes → quote).
const MAX_READS: usize = 6;

/// How much of a tool's result is shown to the model. Enough for a diary month
/// or a stock record; short enough that one verbose tool cannot crowd out the
/// question it was meant to answer.
///
/// Crate-visible because one tool's result is written **against** it: the
/// Insights catalog ([`crate::agent_insights`]) is a menu a model has to read
/// whole, and half a menu is a menu with invented spellings at the end of it, so
/// a test there holds the rendering under this number rather than under a copy
/// of it.
pub(crate) const MAX_RESULT_CHARS: usize = 4_000;

/// The `kind` a tool result carries in the numbered sources, so the model can
/// tell what it looked up from what a search happened to match.
const RESULT_KIND: &str = "tool result";

/// Said when the turn used every lookup it had and still wanted another. It
/// says which question went unanswered rather than pretending to one.
const OUT_OF_LOOKUPS: &str = "I looked things up as far as I'm allowed to for one question and still couldn't get to an \
     answer. Could you narrow it down, or ask me one part of it at a time?";

/// Where a turn is happening, for the boundary check and the audit record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TurnContext<'a> {
    /// The agent taking it; `None` for the command palette's assistant, which
    /// is not a row in `chat_agents`.
    pub agent: Option<&'a ChatAgentId>,
    /// The room it is happening in; `None` outside chat.
    pub channel: Option<&'a ChatChannelId>,
}

impl<'a> TurnContext<'a> {
    /// A turn with no room and no agent record — the command palette.
    pub(crate) const fn palette() -> Self {
        Self {
            agent: None,
            channel: None,
        }
    }

    /// A turn an agent is taking in a room.
    pub(crate) const fn in_room(agent: &'a ChatAgentId, channel: &'a ChatChannelId) -> Self {
        Self {
            agent: Some(agent),
            channel: Some(channel),
        }
    }

    /// How a tool run from inside this turn arrives at the boundary: with
    /// nobody's approval, and therefore reads only.
    const fn run(self) -> ToolRun<'a> {
        ToolRun {
            approval: Approval::InTurn,
            agent: self.agent,
            channel: self.channel,
        }
    }
}

/// Everything one turn is asked from: the words, the access-scoped grounding,
/// the clock, and where it is happening.
///
/// A struct rather than five more parameters, because the two surfaces that
/// build one — a room and the command palette — should be visibly filling in
/// the same form.
pub(crate) struct Turn<'a> {
    /// The product whose agent is taking it, which decides what it is offered
    /// (A1.2). [`AgentProduct::Workspace`] for the command palette's "Ask alo".
    ///
    /// This is the *prompt's* copy of the scope. The one that is enforced is
    /// read from the agent's own row at the execution boundary, so a turn that
    /// claimed a wider product here would be offered tools it still could not
    /// run.
    pub product: AgentProduct,
    /// What was asked, in the asker's own words.
    pub request: &'a str,
    /// The grounding the caller retrieved through the asker's access.
    pub sources: &'a [WorkspaceSource],
    /// Today's date, and the clock the asker is on.
    pub today: &'a str,
    /// The asker's own mail folders, for `move_to_folder`.
    pub folders: &'a [String],
    /// The agent and the room, if there are any.
    pub context: TurnContext<'a>,
}

/// What one turn ended as.
pub(crate) enum TurnResult {
    /// Something to say, grounded in the sources and in whatever was looked up.
    Answer(String),
    /// A change the asker has to approve before it happens.
    Propose {
        /// The tool the caller records against the agent's message.
        action: alo_ai::ProposedAction,
        /// The sentence that goes in the room.
        say: String,
    },
}

/// Run one turn to a result, executing its reading tools along the way.
///
/// `sources` is the access-scoped grounding the caller retrieved; the tool
/// results are appended to it as further numbered sources, so a citation means
/// the same thing whichever it points at.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty —
/// the tool runs themselves never fail a turn, because a refusal or a bad
/// argument is something the model can be told about and answer around.
pub(crate) async fn take_turn(
    state: &AppState,
    account: &Account,
    config: &AiConfig,
    turn: &Turn<'_>,
) -> Result<TurnResult, InferenceError> {
    let Turn {
        product,
        request,
        today,
        folders,
        context,
        ..
    } = *turn;
    let mut sources = turn.sources.to_vec();
    // Built twice rather than held across the loop: `sources` grows by a tool
    // result between the two calls, so an `AgentAsk` borrowing it cannot
    // outlive one round.
    let mut decided = alo_ai::run_agent(
        config,
        &AgentAsk {
            product,
            request,
            sources: &sources,
            today,
            folders,
        },
    )
    .await?;

    for used in 0..MAX_READS {
        let action = match step(decided) {
            Step::Done(result) => return Ok(result),
            Step::RunRead(action) => action,
        };
        let detail = run_read(state, account, &action, context).await;
        sources.push(WorkspaceSource {
            index: sources.len() + 1,
            kind: RESULT_KIND.to_owned(),
            title: action.tool.clone(),
            detail,
        });
        let more_allowed = used + 1 < MAX_READS;
        decided = alo_ai::run_agent_after_read(
            config,
            &AgentAsk {
                product,
                request,
                sources: &sources,
                today,
                folders,
            },
            more_allowed,
        )
        .await?;
    }
    Ok(finish(decided))
}

/// What a decision means for the turn: either the turn is over, or there is a
/// read to run before asking again.
enum Step {
    /// Nothing left to look up.
    Done(TurnResult),
    /// A reading tool to execute, whose result grounds the next question.
    RunRead(alo_ai::ProposedAction),
}

/// Read one decision (ADR 0047 §1). **The whole read-versus-write split lives
/// here**, and it asks the registry — never the tool's name, never its
/// description, and never the model's own word for what it is doing.
///
/// A write is proposed, exactly as before. A read is run. Anything the registry
/// does not know is proposed, so an unfamiliar name meets the allowlist at the
/// execution boundary rather than the path that skips approval.
///
/// **Product scope is deliberately not asked here** (A1.2). A read belonging to
/// another product still takes the read path and is refused by the boundary,
/// which tells the model *which* agent owns it — so the turn ends with the
/// agent saying who to ask instead of putting a button on a lookup, which is
/// the bug ADR 0047 exists to remove. Checking it here as well would be a
/// second copy of a permission rule, and two copies are how they come to
/// disagree.
fn step(decided: AgentDecision) -> Step {
    match decided {
        AgentDecision::Answer(answer) => Step::Done(TurnResult::Answer(answer)),
        AgentDecision::Action { action, say } => {
            if alo_ai::is_read_tool(&action.tool) {
                Step::RunRead(action)
            } else {
                Step::Done(TurnResult::Propose { action, say })
            }
        }
    }
}

/// The decision, once no more reads may run.
///
/// A read still being asked for here is the bound biting: it must not become a
/// proposal, because a button that reads something is the bug ADR 0047 exists
/// to remove — so the turn says what happened instead.
fn finish(decided: AgentDecision) -> TurnResult {
    match step(decided) {
        Step::Done(result) => result,
        Step::RunRead(_) => TurnResult::Answer(OUT_OF_LOOKUPS.to_owned()),
    }
}

/// Run one reading tool and render its result for the model.
///
/// A refusal is not an error here: the model is told plainly that the lookup
/// did not work, so it can answer around it or say what it could not find —
/// which is a far better turn than one that dies silently.
async fn run_read(
    state: &AppState,
    account: &Account,
    action: &alo_ai::ProposedAction,
    context: TurnContext<'_>,
) -> String {
    match execute_tool(state, account, &action.tool, &action.args, &context.run()).await {
        Ok(axum::Json(body)) => {
            let result = body.get("result").unwrap_or(&body);
            truncate(&result.to_string())
        }
        // The `Problem` detail is the executor's own words about the arguments
        // ("to is before from", "no folder by that name") and carries no
        // content of anybody's records — safe to hand back to the model, and
        // the only thing that lets it correct itself.
        Err(problem) => format!(
            "this lookup did not run: {}",
            problem.detail.as_deref().unwrap_or("it was refused")
        ),
    }
}

/// Cut a result to [`MAX_RESULT_CHARS`], on a character boundary, saying so.
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{cut}… (result truncated; ask for a narrower range to see the rest)")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn action(tool: &str) -> AgentDecision {
        AgentDecision::Action {
            action: alo_ai::ProposedAction {
                tool: tool.to_owned(),
                args: serde_json::json!({}),
            },
            say: "doing it".to_owned(),
        }
    }

    /// The bound biting must not produce a button on a read.
    #[test]
    fn a_read_still_wanted_at_the_bound_becomes_an_answer() {
        match finish(action("stock_answer")) {
            TurnResult::Answer(text) => assert!(text.contains("narrow it down")),
            TurnResult::Propose { .. } => panic!("a read must never become a proposal"),
        }
    }

    #[test]
    fn a_write_at_the_bound_is_still_proposed() {
        match finish(action("create_task")) {
            TurnResult::Propose { action, say } => {
                assert_eq!(action.tool, "create_task");
                assert_eq!(say, "doing it");
            }
            TurnResult::Answer(_) => panic!("a write must still wait for a tap"),
        }
    }

    /// A tool nobody declared is not a read, so it takes the proposal path —
    /// where the allowlist refuses it — rather than the one that skips
    /// approval.
    #[test]
    fn an_unknown_tool_is_not_treated_as_a_read() {
        assert!(matches!(
            finish(action("delete_everything")),
            TurnResult::Propose { .. }
        ));
        assert!(matches!(step(action("delete_everything")), Step::Done(_)));
    }

    /// **Another product's read takes the read path and is refused there**
    /// (A1.2), rather than being turned into a button here.
    ///
    /// The scope is not asked in this module on purpose: the diary lookup goes
    /// to [`crate::agent::execute_tool`] whoever asked for it, and the
    /// Inventory agent gets back "whats_on is not a tool the inventory agent
    /// has", which the next call turns into an answer naming the agent that
    /// owns it. Putting the check here as well would make a lookup wear a
    /// button — the exact bug ADR 0047 removed — and would be a second copy of
    /// a permission rule.
    #[test]
    fn another_products_read_still_takes_the_read_path() {
        assert!(matches!(step(action("whats_on")), Step::RunRead(_)));
        // What makes that safe is the boundary, not this: no product but
        // Agenda and Ask alo may actually run it.
        for product in alo_store::ALL_AGENT_PRODUCTS {
            assert_eq!(
                alo_ai::offers(product, "whats_on"),
                matches!(product, AgentProduct::Agenda | AgentProduct::Workspace),
                "{product}"
            );
        }
    }

    /// The refusal a foreign lookup comes back with is handed to the model as
    /// the tool's result, so the turn can say who to ask instead of dying
    /// silently. This pins the rendering of that refusal, which is the only
    /// part of it this module owns.
    #[test]
    fn a_refused_lookup_is_reported_to_the_model_rather_than_failing_the_turn() {
        let problem = crate::error::Problem::with(
            axum::http::StatusCode::FORBIDDEN,
            "whats_on is not a tool the inventory agent has — ask the agent whose product it belongs to",
        );
        let said = format!(
            "this lookup did not run: {}",
            problem.detail.as_deref().unwrap_or("it was refused")
        );
        assert!(said.contains("not a tool the inventory agent has"));
        assert!(said.contains("did not run"));
    }

    /// **A read never becomes a proposal, and a write always does** — over
    /// every tool that exists, not a sample. `TurnResult::Propose` is the only
    /// thing either surface turns into a `chat_proposals` row, so this is the
    /// property that keeps a read out of that table.
    #[test]
    fn every_read_runs_and_every_write_waits() {
        for entry in alo_ai::all_tools() {
            // Ask alo has every tool, so this stays a statement about the
            // read/write split and not about product scope (which
            // `a_read_belonging_to_another_product_is_never_run_in_the_turn`
            // covers over every product).
            match step(action(entry.name)) {
                Step::RunRead(ran) => {
                    assert!(entry.is_read(), "{} ran without approval", entry.name);
                    assert_eq!(ran.tool, entry.name);
                }
                Step::Done(TurnResult::Propose { action, .. }) => {
                    assert!(!entry.is_read(), "{} was put behind a button", entry.name);
                    assert_eq!(action.tool, entry.name);
                }
                Step::Done(TurnResult::Answer(_)) => {
                    panic!("{} became an answer with no tool run", entry.name)
                }
            }
        }
        // Thirty-four of them, which is the whole point of ADR 0047 — eleven
        // from the products A1 covered, the Website agent's three (A2.1), its
        // language count (A2.1b), the Sheet agent's three (A2.2), the Docs
        // agent's two (A2.3), the Insights agent's three (A2.4), the Drive
        // agent's two (A2.5), the Agenda agent's two (A2.6), the Tasks agent's
        // three (A2.7), the Mail agent's two (A2.8) and the Meet agent's two
        // (A3.2); the rename, the move, the reschedule, the priority, the
        // chase, the capture and the minutes are writes and are counted on the
        // other side.
        let reads = alo_ai::all_tools().iter().filter(|t| t.is_read()).count();
        assert_eq!(reads, 34);
    }

    #[test]
    fn an_answer_passes_straight_through() {
        match finish(AgentDecision::Answer("42 in stock [1].".to_owned())) {
            TurnResult::Answer(text) => assert_eq!(text, "42 in stock [1]."),
            TurnResult::Propose { .. } => panic!("an answer is not a proposal"),
        }
    }

    #[test]
    fn a_long_result_is_cut_on_a_character_boundary_and_says_so() {
        let short = "é".repeat(10);
        assert_eq!(truncate(&short), short);
        let long = "é".repeat(MAX_RESULT_CHARS + 50);
        let cut = truncate(&long);
        assert!(cut.starts_with(&"é".repeat(MAX_RESULT_CHARS)));
        assert!(cut.contains("truncated"));
        // Exactly at the bound is not truncated.
        assert_eq!(
            truncate(&"é".repeat(MAX_RESULT_CHARS)).chars().count(),
            MAX_RESULT_CHARS
        );
    }

    /// An in-turn run carries no approval, whichever surface it is on. This is
    /// the property the write refusal rests on.
    #[test]
    fn a_turn_never_carries_an_approval() {
        assert_eq!(TurnContext::palette().run().approval, Approval::InTurn);
        let agent = ChatAgentId::new("a1".to_owned());
        let channel = ChatChannelId::new("c1".to_owned());
        let run = TurnContext::in_room(&agent, &channel).run();
        assert_eq!(run.approval, Approval::InTurn);
        assert_eq!(run.agent.map(ChatAgentId::as_str), Some("a1"));
        assert_eq!(run.channel.map(ChatChannelId::as_str), Some("c1"));
    }
}
