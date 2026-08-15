//! One agent turn (ADR 0034) — an agent that either ANSWERS from the user's
//! workspace or PROPOSES one action for the user to approve. It never executes:
//! the jmap layer runs an approved action through the tenant-scoped store.
//! Model-agnostic by design — the model replies with a single JSON envelope (no
//! native function-calling required), parsed here.
//!
//! **Every turn is a product's turn** (A1.2). The prompt is built for one
//! [`AgentProduct`] and offers that product's tools and nobody else's, so the
//! Inventory agent is told about stock and not about payroll.
//! [`AgentProduct::Workspace`] is "Ask alo", the one agent that works across
//! products and is therefore offered all of them. None of this is a permission
//! system on its own: the boundary that refuses a tool an agent was not offered
//! is `alo-jmap`'s `execute_tool`, and it asks [`crate::agent_product::offers`]
//! rather than trusting that the prompt was obeyed.
//!
//! Trust rule (ADR 0023/0034/0047): a tool that changes something is proposed
//! and the user approves. Nothing in this module performs an action; it only
//! decides what to propose.

use serde::{Deserialize, Serialize};

pub use alo_store::AgentProduct;

use crate::agent_product::{tool_sets, tools_for};
use crate::agent_tool::{AgentTool, find_tool};
use crate::{AiConfig, ChatMessage, InferenceError, WorkspaceSource, chat, render_sources};

/// Every tool that exists, across every product, in prompt order.
///
/// One list rather than twelve, so a caller cannot ask some of them and forget
/// another — the mistake [`is_agent_tool`] was written to prevent, now with the
/// effect bit carried along beside the name. **This is what exists, not what
/// any one agent may use**: for that, ask
/// [`crate::agent_product::offers`].
#[must_use]
pub fn all_tools() -> Vec<AgentTool> {
    tools_for(AgentProduct::Workspace)
}

/// One action the agent proposes. `args` is validated tool-by-tool at the
/// execution boundary — never trusted here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedAction {
    /// Tool name; must be one of [`AGENT_TOOLS`] to be executable.
    pub tool: String,
    /// Tool arguments, shape defined per tool and validated before execution.
    pub args: serde_json::Value,
}

/// What the agent decided for one turn: answer, or propose a single action.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentDecision {
    /// A grounded answer (cites the numbered sources); nothing to execute.
    Answer(String),
    /// A proposed action + a one-line human description; executed only on approval.
    Action {
        /// The tool + args the agent wants to run, pending approval.
        action: ProposedAction,
        /// A short, human sentence describing what it will do.
        say: String,
    },
}

/// The opening of the system prompt: what the agent is, and the two shapes its
/// reply may take.
const AGENT_SYSTEM_HEAD: &str = "For each request you do EXACTLY ONE of two things, and you reply with a SINGLE JSON object and nothing else:\n\
1) ANSWER from the numbered sources below: {\"kind\":\"answer\",\"answer\":\"<text>\"}. Cite each source you use by its number in square brackets like [1]. Use ONLY the sources; if they do not contain the answer, say you could not find it — never invent files, people, or facts.\n\
2) USE ONE TOOL: {\"kind\":\"action\",\"say\":\"<one short sentence describing what you will do>\",\"action\":{\"tool\":\"<tool>\",\"args\":{...}}}. What happens next depends on the tool, and the two lists after the descriptions below say which is which: a READING tool runs immediately and comes back to you as a source to answer from, while a tool that CHANGES something is only proposed and waits for the user to approve it — you never perform a change yourself.\n";

/// Said to an agent whose product is not searched for it (A1.3).
///
/// Billing, CRM, Projects, Finance, Inventory and People reach their records
/// through a reading tool rather than through retrieval — the module gate rides
/// with the tool, and a search predicate would be a second door into
/// role-gated rows (`alo_store::agent_ground`). An agent that is not told this
/// reads an empty source list as "there is nothing", which is the wrong answer;
/// it should look the record up instead.
const GROUND_BY_TOOL: &str = "Nothing in your product is searched for you before you are asked: \
the numbered sources are whatever else matched, never your own records. Reach those with one of \
your reading tools, and never answer a question about them from a source that merely mentions the \
subject.\n";

/// Said instead of a tool list to an agent whose product has none yet
/// (Insights, Meet, Sites until their waves land).
const NO_TOOLS_YET: &str = "You have no tools in this product yet, so you ANSWER from the numbered sources and never \
     return an action. If the request needs something done, say plainly that you cannot do it yet.\n";

/// The rules that apply whatever the product, and the output contract.
///
/// Rules that belong to **one** product live in that product's guidance — the
/// `source` numbering of an email moved to [`crate::agent_mail`] in A1.2,
/// because an agent with no email tool was being told how to fill in an
/// argument it would never have.
const AGENT_SYSTEM_RULES: &str = "\
Resolve any relative date or time (today, tomorrow 3pm, next Friday) against the current date given below into an absolute value (YYYY-MM-DD for a task due, RFC 3339 UTC for an event). \
If the request needs an action no tool covers, ANSWER instead and say you cannot do that yet. Write the answer/say text in the user's language. Output ONLY the JSON object — no markdown, no code fences, no preamble.";

/// The whole system prompt for one product's agent: who it is, the tools it —
/// and only it — may use, then the rules that hold whatever the product.
///
/// Built rather than written out, so a product agent (ADR 0034) is a row in
/// [`crate::agent_product`] plus a tool list and a paragraph in its own module.
#[must_use]
pub fn system_prompt_for(product: AgentProduct) -> String {
    let sets = tool_sets(product);
    let mut docs = String::new();
    let mut guidance = String::new();
    for set in sets {
        docs.push_str(set.doc);
        guidance.push_str(set.guidance);
    }
    let no_tools = sets.iter().all(|set| set.tools.is_empty());
    let tools = if no_tools {
        NO_TOOLS_YET.to_owned()
    } else {
        format!("Available tools:\n{docs}{}", effect_block(product))
    };
    // A product with tools but no retrieval is told where its records actually
    // are. One with neither is told nothing extra: it has no lookup to offer,
    // and `NO_TOOLS_YET` has already said so.
    let ground = if no_tools
        || product == AgentProduct::Workspace
        || !alo_store::agent_ground::sources_for(product).is_empty()
    {
        ""
    } else {
        GROUND_BY_TOOL
    };
    let stay = if product == AgentProduct::Workspace {
        ""
    } else {
        crate::agent_product::STAY_IN_PRODUCT
    };
    format!(
        "{}{stay}\n{AGENT_SYSTEM_HEAD}{tools}{ground}{guidance}{AGENT_SYSTEM_RULES}",
        crate::agent_product::headline(product)
    )
}

/// Whether `tool` is a tool that exists at all — the allowlist the execution
/// boundary asks first, across every product's set.
///
/// One question rather than one list per product, so a caller (the jmap execute
/// route) cannot check some of them and forget another. It says nothing about
/// **whose** tool it is: that is [`crate::agent_product::offers`], and the
/// boundary asks both.
#[must_use]
pub fn is_agent_tool(tool: &str) -> bool {
    all_tools().iter().any(|entry| entry.name == tool)
}

/// Whether `tool` is one that only reads, and may therefore run inside a turn
/// with nobody's approval (ADR 0047 §1).
///
/// The answer comes from the registry and from nowhere else: not from the
/// tool's name, not from its description, and never from the model's own word
/// for what it is doing — the model is the untrusted party in this design, and
/// an injected turn would call a write a read. **A name the registry does not
/// know is not a read**, so an unfamiliar tool takes the proposal path and
/// meets [`is_agent_tool`] at the boundary rather than the path that skips
/// approval.
#[must_use]
pub fn is_read_tool(tool: &str) -> bool {
    let all = all_tools();
    find_tool(&all, tool).is_some_and(|entry| entry.is_read())
}

/// The read/write split, rendered for the model out of the declarations
/// themselves (ADR 0047 §1).
///
/// It used to be a hand-written sentence — "It only READS; it changes nothing."
/// — repeated in eleven tool descriptions, which is a claim the code could not
/// check and the behaviour could drift from. Generating it here means the
/// prompt and the execution boundary read the same list, so they cannot
/// disagree about which tools need a tap.
fn effect_block(product: AgentProduct) -> String {
    let offered = tools_for(product);
    let names = |want_read: bool| {
        offered
            .iter()
            .filter(|tool| tool.is_read() == want_read)
            .map(|tool| tool.name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    // A product with only reads (or only writes) is told about the half it
    // has; naming an empty list would read as "and here are the others",
    // which is the sentence that invites a tool nobody offered.
    if names(true).is_empty() {
        return format!(
            "\nEvery tool you have CHANGES something. Choosing one PROPOSES it: the user reads your \
\"say\" line and approves it before anything happens, and you never see a result — {}.\n",
            names(false)
        );
    }
    if names(false).is_empty() {
        return format!(
            "\nEvery tool you have only READS. It changes nothing, so choosing one does not ask the \
user anything: it runs straight away and you are then asked again with its result, so you can ANSWER \
from what it found. Prefer one of these over the sources whenever the question is about a record they \
cover — {}.\n",
            names(true)
        );
    }
    format!(
        "\nThese tools only READ. They change nothing, so choosing one does not ask the user anything: it runs \
straight away and you are then asked again with its result, so you can ANSWER from what it found. Prefer one of \
these over the sources whenever the question is about a record they cover — {}.\n\
Every other tool CHANGES something. Choosing one PROPOSES it: the user reads your \"say\" line and approves it \
before anything happens, and you never see a result — {}.\n",
        names(true),
        names(false)
    )
}

/// Everything one turn is asked from.
///
/// A struct rather than five parameters repeated across four functions: the
/// **product** joined them in A1.2, and a fifth positional argument of the same
/// shape as its neighbours is how a caller ends up passing the wrong agent's
/// scope without the compiler noticing.
#[derive(Debug, Clone, Copy)]
pub struct AgentAsk<'a> {
    /// Whose agent is taking this turn — what it may be offered (ADR 0034).
    pub product: AgentProduct,
    /// The request, in the asker's own words.
    pub request: &'a str,
    /// The grounding, retrieved through the asker's own access.
    pub sources: &'a [WorkspaceSource],
    /// The caller's current date (`YYYY-MM-DD`), so the model can resolve
    /// relative dates like "tomorrow".
    pub today: &'a str,
    /// The user's mail-folder names, so `move_to_folder` can only target a real
    /// one. Ignored unless this product actually has that tool.
    pub folders: &'a [String],
}

/// The chat messages for one agent turn. Pure and exported so the prompt is
/// testable without a backend.
#[must_use]
pub fn agent_messages(ask: &AgentAsk<'_>) -> Vec<ChatMessage> {
    // Only an agent that can move a message needs to know the folders. For
    // every other product the list is noise the model has to read past, and
    // noise in a prompt is where an invented tool call comes from.
    let folder_line =
        if ask.folders.is_empty() || !crate::agent_product::offers(ask.product, "move_to_folder") {
            String::new()
        } else {
            format!(
                "\n\nFolders (for move_to_folder): {}",
                ask.folders.join(", ")
            )
        };
    let user = format!(
        "Today's date is {}.\nRequest: {}\n\nSources:\n{}{}",
        ask.today.trim(),
        ask.request.trim(),
        render_sources(ask.sources),
        folder_line
    );
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: system_prompt_for(ask.product),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: user,
        },
    ]
}

/// Slice the JSON object out of the model's text, so a stray code fence or one
/// line of preamble does not break parsing. Shared with [`crate::insights`],
/// which reads a different envelope out of the same kind of reply.
pub(crate) fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

/// Parse the model's reply into an [`AgentDecision`]. Tolerant of code fences and
/// surrounding text; strict about the envelope shape.
///
/// # Errors
/// [`InferenceError::Empty`] if no valid envelope is present.
pub fn parse_decision(text: &str) -> Result<AgentDecision, InferenceError> {
    #[derive(Deserialize)]
    struct Envelope {
        kind: String,
        #[serde(default)]
        answer: Option<String>,
        #[serde(default)]
        action: Option<ProposedAction>,
        #[serde(default)]
        say: Option<String>,
    }
    let json = extract_json(text).ok_or(InferenceError::Empty)?;
    let env: Envelope = serde_json::from_str(json).map_err(|_| InferenceError::Empty)?;
    match env.kind.as_str() {
        "answer" => {
            let answer = env.answer.unwrap_or_default().trim().to_owned();
            if answer.is_empty() {
                return Err(InferenceError::Empty);
            }
            Ok(AgentDecision::Answer(answer))
        }
        "action" => {
            let action = env.action.ok_or(InferenceError::Empty)?;
            if action.tool.trim().is_empty() {
                return Err(InferenceError::Empty);
            }
            Ok(AgentDecision::Action {
                action,
                say: env.say.unwrap_or_default().trim().to_owned(),
            })
        }
        _ => Err(InferenceError::Empty),
    }
}

/// Told to the model on the second and later calls of a read turn, when there
/// is still a lookup left in the budget.
const MORE_READS_LEFT: &str = "\n\nThe last source above is the result of a tool you just ran. \
ANSWER the request from it now if it contains what you need — that is what you looked it up for. \
Only run one more reading tool if the answer genuinely needs a second lookup.";

/// Told instead when the lookup budget is spent (ADR 0047 §2).
const NO_READS_LEFT: &str = "\n\nThe last source above is the result of a tool you just ran, and it \
was your LAST lookup — you may not run another reading tool for this request. ANSWER from what you \
have; if it is not enough, say plainly what you could not find out and ask the user to narrow the \
question. You may still propose a tool that CHANGES something.";

/// The chat messages for the second and later calls of a read turn (ADR 0047
/// §2), where `sources` already carries the tool results as further numbered
/// sources. `more_allowed` says whether the turn's lookup budget has anything
/// left.
///
/// Pure and exported so the prompt is testable without a backend.
#[must_use]
pub fn after_read_messages(ask: &AgentAsk<'_>, more_allowed: bool) -> Vec<ChatMessage> {
    let mut messages = agent_messages(ask);
    if let Some(user) = messages.last_mut() {
        user.content.push_str(if more_allowed {
            MORE_READS_LEFT
        } else {
            NO_READS_LEFT
        });
    }
    messages
}

/// Run one agent turn: build the prompt from the request + access-scoped sources,
/// call the model, and parse its decision. Returns a decision to PROPOSE — it
/// never executes anything.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty.
pub async fn run_agent(
    config: &AiConfig,
    ask: &AgentAsk<'_>,
) -> Result<AgentDecision, InferenceError> {
    let text = chat(config, &agent_messages(ask), 0.2).await?;
    parse_decision(&text)
}

/// Ask again once a reading tool has run and its result has joined `sources`
/// (ADR 0047 §2) — the second half of what makes a read *answer* rather than
/// propose.
///
/// Identical in every other respect to [`run_agent`]: same prompt, same
/// envelope, same refusal to execute anything. `more_allowed` is false on the
/// last lookup of the turn, so the model is told to answer with what it has
/// rather than asking for a lookup it will not get.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty.
pub async fn run_agent_after_read(
    config: &AiConfig,
    ask: &AgentAsk<'_>,
    more_allowed: bool,
) -> Result<AgentDecision, InferenceError> {
    let text = chat(config, &after_read_messages(ask, more_allowed), 0.2).await?;
    parse_decision(&text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn src(i: usize, kind: &str, title: &str) -> WorkspaceSource {
        WorkspaceSource {
            index: i,
            kind: kind.to_owned(),
            title: title.to_owned(),
            detail: String::new(),
        }
    }

    /// A1.3: the sentence that tells an agent where its records actually are is
    /// said to exactly the products that are not searched for — never to one
    /// that is, never to Ask alo, and never to an agent with no tool to look
    /// anything up with. Read off `agent_ground`'s own table, so the prompt and
    /// the retrieval cannot drift apart.
    #[test]
    fn only_a_product_with_tools_but_no_retrieval_is_told_to_look_it_up() {
        for product in alo_store::ALL_AGENT_PRODUCTS {
            let prompt = system_prompt_for(product);
            let searched = !alo_store::agent_ground::sources_for(product).is_empty();
            let has_tools = !tools_for(product).is_empty();
            let expected = has_tools && !searched && product != AgentProduct::Workspace;
            assert_eq!(
                prompt.contains(GROUND_BY_TOOL),
                expected,
                "{product} is told the wrong thing about its grounding"
            );
        }
        // Stated plainly, because it is the pair A1.3 turns on.
        assert!(system_prompt_for(AgentProduct::Inventory).contains(GROUND_BY_TOOL));
        assert!(!system_prompt_for(AgentProduct::Mail).contains(GROUND_BY_TOOL));
    }

    #[test]
    fn parses_an_answer_envelope() {
        let d = parse_decision(r#"{"kind":"answer","answer":"It's in [1]."}"#).unwrap();
        assert_eq!(d, AgentDecision::Answer("It's in [1].".to_owned()));
    }

    #[test]
    fn parses_an_action_envelope() {
        let text = r#"{"kind":"action","say":"Create a task to follow up.","action":{"tool":"create_task","args":{"title":"Follow up with Acme","due":"2026-08-07"}}}"#;
        match parse_decision(text).unwrap() {
            AgentDecision::Action { action, say } => {
                assert_eq!(action.tool, "create_task");
                assert_eq!(action.args["title"], "Follow up with Acme");
                assert!(say.contains("Create"));
            }
            other => panic!("expected an action, got {other:?}"),
        }
    }

    #[test]
    fn tolerates_code_fences_and_preamble() {
        let text = "Sure!\n```json\n{\"kind\":\"answer\",\"answer\":\"Hi\"}\n```";
        assert_eq!(
            parse_decision(text).unwrap(),
            AgentDecision::Answer("Hi".to_owned())
        );
    }

    #[test]
    fn rejects_garbage_empty_and_malformed() {
        assert!(parse_decision("no json here").is_err());
        assert!(parse_decision(r#"{"kind":"answer","answer":"   "}"#).is_err());
        assert!(parse_decision(r#"{"kind":"action","say":"x"}"#).is_err()); // no action
        assert!(parse_decision(r#"{"kind":"action","action":{"tool":"","args":{}}}"#).is_err());
        assert!(parse_decision(r#"{"kind":"other"}"#).is_err());
    }

    fn ask<'a>(
        product: AgentProduct,
        request: &'a str,
        sources: &'a [WorkspaceSource],
        folders: &'a [String],
    ) -> AgentAsk<'a> {
        AgentAsk {
            product,
            request,
            sources,
            today: "2026-08-07",
            folders,
        }
    }

    #[test]
    fn prompt_carries_request_sources_date_and_the_tool() {
        let folders = ["Work".to_owned(), "Receipts".to_owned()];
        let sources = [src(1, "message", "Acme thread")];
        let msgs = agent_messages(&ask(
            AgentProduct::Workspace,
            "book a slot",
            &sources,
            &folders,
        ));
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].content.contains("create_task"));
        assert!(msgs[1].content.contains("book a slot"));
        assert!(msgs[1].content.contains("Acme thread"));
        assert!(msgs[1].content.contains("2026-08-07"));
        // The user's real folder names are offered for move_to_folder.
        assert!(msgs[1].content.contains("Work, Receipts"));
    }

    #[test]
    fn prompt_omits_the_folder_line_when_there_are_none() {
        let msgs = agent_messages(&ask(AgentProduct::Workspace, "hi", &[], &[]));
        assert!(!msgs[1].content.contains("Folders (for move_to_folder)"));
    }

    /// The folder list is Mail's business. An Inventory agent has no
    /// `move_to_folder`, so naming the user's mail folders at it is a menu for
    /// a tool it cannot call — and the shortest route to an invented one.
    #[test]
    fn only_an_agent_that_can_move_a_message_is_told_the_folders() {
        let folders = ["Work".to_owned(), "Receipts".to_owned()];
        for product in [AgentProduct::Mail, AgentProduct::Workspace] {
            let msgs = agent_messages(&ask(product, "file this", &[], &folders));
            assert!(msgs[1].content.contains("Work, Receipts"), "{product}");
        }
        for product in [
            AgentProduct::Inventory,
            AgentProduct::Agenda,
            AgentProduct::Sites,
        ] {
            let msgs = agent_messages(&ask(product, "file this", &[], &folders));
            assert!(
                !msgs[1].content.contains("Folders (for move_to_folder)"),
                "{product}"
            );
        }
    }

    /// **A product's agent is offered its own product's tools and no others**
    /// (A1.2). The prompt is not the permission system — the boundary is — but
    /// an agent told about a tool it cannot run would spend every turn
    /// proposing refusals.
    #[test]
    fn a_products_prompt_describes_exactly_that_products_tools() {
        for product in alo_store::ALL_AGENT_PRODUCTS {
            let prompt = system_prompt_for(product);
            let mine = crate::agent_product::tools_for(product);
            for tool in &mine {
                assert!(
                    prompt.contains(&format!("- {}:", tool.name)),
                    "{product} is not told about its own {}",
                    tool.name
                );
            }
            for tool in all_tools() {
                if mine.iter().any(|entry| entry.name == tool.name) {
                    continue;
                }
                assert!(
                    !prompt.contains(&format!("- {}:", tool.name)),
                    "{product} is offered {}, which is another product's",
                    tool.name
                );
            }
            assert_eq!(
                prompt.matches("\n- ").count(),
                mine.len(),
                "{product} describes exactly the tools it has"
            );
            // Whoever it is, it is told so, and the output contract is still
            // the last thing it reads.
            assert!(prompt.starts_with("You are "));
            assert!(prompt.ends_with("no preamble."));
        }
        // The three products whose agents are still to be built say so plainly
        // rather than leaving an empty menu under a heading.
        for empty in [
            AgentProduct::Insights,
            AgentProduct::Meet,
            AgentProduct::Sites,
        ] {
            let prompt = system_prompt_for(empty);
            assert!(prompt.contains("no tools in this product yet"), "{empty}");
            assert!(!prompt.contains("Available tools:"), "{empty}");
        }
        // And only Ask alo is free to answer about anything.
        assert!(!system_prompt_for(AgentProduct::Workspace).contains("not yours to answer"));
        assert!(system_prompt_for(AgentProduct::Hr).contains("not yours to answer"));
    }

    #[test]
    fn every_executable_tool_is_described_and_every_described_tool_is_executable() {
        // The allowlist and the prompt are one surface: a tool the model is
        // told about but the execute route refuses is a dead proposal, and a
        // tool it is never told about is dead code.
        let prompt = system_prompt_for(AgentProduct::Workspace);
        for tool in all_tools() {
            let name = tool.name;
            assert!(prompt.contains(&format!("- {name}:")), "{name} undescribed");
            assert!(is_agent_tool(name), "{name} is not allowed to execute");
        }
        assert_eq!(
            prompt.matches("\n- ").count(),
            all_tools().len(),
            "the prompt describes exactly the tools that exist"
        );
        // A name from neither list is not executable, whatever it looks like.
        for stranger in [
            "",
            "create_task ",
            "delete_invoice",
            "delete_deal",
            "post_entry",
        ] {
            assert!(!is_agent_tool(stranger), "{stranger:?} must not be allowed");
        }
    }

    /// ADR 0047 §1. The split is a property of the tool, declared once, and the
    /// eleven reads are the eleven the ADR names — no more, and not by prefix.
    #[test]
    fn the_registry_declares_which_tools_only_read() {
        let reads: Vec<&str> = all_tools()
            .iter()
            .filter(|t| t.is_read())
            .map(|t| t.name)
            .collect();
        assert_eq!(
            reads,
            [
                "find_contact",
                "whats_on",
                "am_i_free",
                "catch_up_room",
                "find_in_chat",
                "find_file",
                "project_status_summary",
                "vat_summary",
                "flag_anomalies",
                "stock_answer",
                "who_is_off",
            ]
        );
        assert_eq!(all_tools().len(), 33);
        for name in &reads {
            assert!(is_read_tool(name), "{name} is declared a read");
        }
        // A write is never a read, however much its name looks like a lookup.
        for name in ["draft_letter_from_template", "categorise_transactions"] {
            assert!(is_agent_tool(name));
            assert!(!is_read_tool(name), "{name} must still wait for a tap");
        }
        // And a name nobody declared is not a read — so it takes the proposal
        // path, where the allowlist refuses it, rather than the path that skips
        // approval. This is the silent failure the ADR rejects a naming
        // convention to avoid.
        for stranger in ["", "find_everything", "stock_answer ", "read_payroll"] {
            assert!(
                !is_read_tool(stranger),
                "{stranger:?} must not run un-tapped"
            );
        }
    }

    /// The prompt's statement of the split is generated from the same list the
    /// boundary asks, so prose and behaviour cannot drift (ADR 0047 §1).
    #[test]
    fn the_prompt_states_the_split_from_the_declarations() {
        let prompt = system_prompt_for(AgentProduct::Workspace);
        let block = effect_block(AgentProduct::Workspace);
        assert!(prompt.contains(&block));
        let (reading, changing) = block
            .split_once("Every other tool CHANGES something")
            .expect("both halves are rendered");
        for tool in all_tools() {
            let half = if tool.is_read() { reading } else { changing };
            assert!(
                half.contains(tool.name),
                "{} is on the wrong side of the prompt's split",
                tool.name
            );
        }
        // The hand-written claim this replaced must not come back: two
        // statements of one fact is exactly how they came to disagree.
        assert!(
            !prompt.contains("It only READS"),
            "the split is rendered, never written out per tool"
        );
    }

    /// A product with only reads, or only writes, is told about the half it
    /// has. The two-list sentence would otherwise name an empty list, which
    /// reads as "and here are the others" — an invitation to a tool nobody
    /// offered.
    #[test]
    fn a_one_sided_product_is_told_about_the_half_it_has() {
        // Every Billing tool changes something.
        let billing = system_prompt_for(AgentProduct::Billing);
        assert!(billing.contains("Every tool you have CHANGES something"));
        assert!(!billing.contains("These tools only READ"));

        // Mail's nine writes plus the address book's one read: two-sided, so
        // it gets the two-list sentence.
        let mail = system_prompt_for(AgentProduct::Mail);
        assert!(mail.contains("These tools only READ"));
        assert!(mail.contains("Every other tool CHANGES something"));
        assert!(mail.contains("find_contact"));

        // Chat has only lookups.
        let chat = system_prompt_for(AgentProduct::Chat);
        assert!(chat.contains("Every tool you have only READS"));
        assert!(!chat.contains("Every other tool CHANGES"));

        // Drive has exactly one tool and it is a lookup.
        let drive = system_prompt_for(AgentProduct::Drive);
        assert!(drive.contains("Every tool you have only READS"));
        assert!(!drive.contains("Every other tool CHANGES"));
        assert!(drive.contains("find_file"));

        // Tasks has exactly one and it changes something.
        let tasks = system_prompt_for(AgentProduct::Tasks);
        assert!(tasks.contains("Every tool you have CHANGES something"));
        assert!(tasks.contains("create_task"));
    }

    #[test]
    fn the_second_call_tells_the_model_the_result_is_there_and_what_is_left() {
        let sources = [src(1, "tool result", "stock_answer")];
        let asked = AgentAsk {
            product: AgentProduct::Inventory,
            request: "is the X100 in stock?",
            sources: &sources,
            today: "2026-08-14",
            folders: &[],
        };
        let more = after_read_messages(&asked, true);
        let last = &more[1].content;
        assert!(last.contains("is the X100 in stock?"));
        assert!(last.contains("stock_answer"));
        assert!(last.contains("result of a tool you just ran"));
        assert!(last.contains("Only run one more reading tool"));

        // On the last lookup it is told so, and told to say what it could not
        // find rather than asking for a lookup it will not get.
        let done = after_read_messages(&asked, false);
        let last = &done[1].content;
        assert!(last.contains("LAST lookup"));
        assert!(last.contains("ask the user to narrow the question"));
        assert!(!last.contains("Only run one more reading tool"));
        // Same system prompt either way — one contract, not two — and it is
        // still the Inventory agent's, not everybody's.
        assert_eq!(more[0].content, done[0].content);
        assert_eq!(more[0].content, system_prompt_for(AgentProduct::Inventory));
    }

    #[test]
    fn the_output_contract_stays_the_last_thing_the_model_reads() {
        let prompt = system_prompt_for(AgentProduct::Workspace);
        assert!(prompt.ends_with("no preamble."));
        // Ask alo reads the products in the order `ALL_AGENT_PRODUCTS` lists
        // them, and each product's guidance after all of the tool lines.
        let at = |needle: &str| prompt.find(needle).unwrap_or(usize::MAX);
        assert!(at("- draft_email:") < at("- find_contact:"));
        assert!(at("- find_contact:") < at("- whats_on:"));
        assert!(at("- whats_on:") < at("- create_task:"));
        assert!(at("- create_task:") < at("- catch_up_room:"));
        assert!(at("- catch_up_room:") < at("- find_file:"));
        assert!(at("- find_file:") < at("- create_invoice_draft:"));
        assert!(at("- create_invoice_draft:") < at("- create_deal:"));
        assert!(at("- create_deal:") < at("- log_time:"));
        assert!(at("- log_time:") < at("- categorise_transactions:"));
        assert!(at("- categorise_transactions:") < at("- reorder_proposals:"));
        assert!(at("- reorder_proposals:") < at("- who_is_off:"));
        // Every tool line comes before every product's guidance, so a model
        // reads the whole menu before it reads how to fill an order from it.
        assert!(at("- who_is_off:") < at("For a billing tool"));
        assert!(at("For a billing tool") < at("For a CRM tool"));
        assert!(at("For a CRM tool") < at("For a projects tool"));
        assert!(at("For a projects tool") < at("For a finance tool"));
        assert!(at("For a finance tool") < at("For an inventory tool"));
        assert!(at("For an inventory tool") < at("For an HR tool"));
        assert!(at("For an HR tool") < at("Output ONLY the JSON object"));
    }
}
