//! The "Ask alo" agent (ADR 0034) — the top-level assistant that either ANSWERS
//! from the user's workspace or PROPOSES one action for the user to approve. It
//! never executes: the jmap layer runs an approved action through the
//! tenant-scoped store. Model-agnostic by design — the model replies with a
//! single JSON envelope (no native function-calling required), parsed here.
//!
//! Trust rule (ADR 0023/0034): the agent proposes, the user approves. Nothing in
//! this module performs an action; it only decides what to propose.

use serde::{Deserialize, Serialize};

use crate::agent_agenda::{AGENDA_GUIDANCE, AGENDA_TOOL_DOC, AGENDA_TOOLS};
use crate::agent_billing::{BILLING_GUIDANCE, BILLING_TOOL_DOC, BILLING_TOOLS};
use crate::agent_chat::{CHAT_GUIDANCE, CHAT_TOOL_DOC, CHAT_TOOLS};
use crate::agent_contacts::{CONTACTS_GUIDANCE, CONTACTS_TOOL_DOC, CONTACTS_TOOLS};
use crate::agent_crm::{CRM_GUIDANCE, CRM_TOOL_DOC, CRM_TOOLS};
use crate::agent_drive::{DRIVE_GUIDANCE, DRIVE_TOOL_DOC, DRIVE_TOOLS};
use crate::agent_finance::{FINANCE_GUIDANCE, FINANCE_TOOL_DOC, FINANCE_TOOLS};
use crate::agent_inventory::{INVENTORY_GUIDANCE, INVENTORY_TOOL_DOC, INVENTORY_TOOLS};
use crate::agent_projects::{PROJECTS_GUIDANCE, PROJECTS_TOOL_DOC, PROJECTS_TOOLS};
use crate::{AiConfig, ChatMessage, InferenceError, WorkspaceSource, chat, render_sources};

/// The core (mail, tasks, calendar) tools the agent may propose, by name.
///
/// A **product** contributes its own list beside this one — billing's is
/// [`BILLING_TOOLS`] — and [`is_agent_tool`] is the allowlist the execution
/// boundary asks. Adding a core tool → describe it in [`AGENT_SYSTEM_TOOLS`]
/// and wire its validation + execution in the jmap agent handler.
pub const AGENT_TOOLS: &[&str] = &[
    "create_task",
    "create_event",
    "mark_read",
    "flag_email",
    "archive_email",
    "trash_email",
    "snooze_email",
    "draft_email",
    "draft_reply",
    "send_email",
    "move_to_folder",
];

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
const AGENT_SYSTEM_HEAD: &str = "You are alo, the assistant across the user's entire workspace. \
For each request you do EXACTLY ONE of two things, and you reply with a SINGLE JSON object and nothing else:\n\
1) ANSWER from the numbered sources below: {\"kind\":\"answer\",\"answer\":\"<text>\"}. Cite each source you use by its number in square brackets like [1]. Use ONLY the sources; if they do not contain the answer, say you could not find it — never invent files, people, or facts.\n\
2) PROPOSE ONE ACTION for the user to approve: {\"kind\":\"action\",\"say\":\"<one short sentence describing what you will do>\",\"action\":{\"tool\":\"<tool>\",\"args\":{...}}}. You NEVER perform the action yourself — you only propose it; the user approves it.\n\
Available tools:\n";

/// The core tools, described. A **product's** tools are described in its own
/// module and spliced in after these ([`system_prompt`]), so a module can gain a
/// tool without this constant being touched.
const AGENT_SYSTEM_TOOLS: &str = "\
- create_task: create a to-do for the user. args: {\"title\": string (required), \"due\": string in \"YYYY-MM-DD\" (optional), \"notes\": string (optional)}.\n\
- create_event: schedule a calendar event. args: {\"title\": string (required), \"start\": string RFC 3339 datetime e.g. \"2026-08-07T14:00:00Z\" (required), \"end\": string RFC 3339 (optional; defaults to one hour after start), \"location\": string (optional), \"notes\": string (optional)}.\n\
- mark_read: mark an email read or unread. args: {\"source\": number, \"read\": boolean}.\n\
- flag_email: flag (star) or unflag an email. args: {\"source\": number, \"flagged\": boolean}.\n\
- archive_email: move an email out of the inbox into Archive. args: {\"source\": number}.\n\
- trash_email: move an email to Trash (delete it from the inbox and archive). args: {\"source\": number}.\n\
- snooze_email: hide an email from the inbox until a chosen time, when it returns to the inbox. args: {\"source\": number, \"until\": string RFC 3339 datetime e.g. \"2026-08-07T09:00:00Z\" (required)}.\n\
- draft_email: write a NEW email and save it to the user's Drafts for them to review and send — it is NEVER sent automatically. args: {\"to\": string email address (required), \"subject\": string (optional), \"body\": string (required)}. Compose the body from the request; do not invent facts. The sender is always the user's own address — never set it.\n\
- draft_reply: write a reply to an email in the sources and save it to the user's Drafts — NEVER sent automatically. args: {\"source\": number (the email to reply to, required), \"body\": string (required)}. The reply goes to that email's sender and keeps its subject thread; compose the body from the request, do not invent facts.\n\
- send_email: SEND a message that is ALREADY in the user's Drafts. This delivers it to its recipients and CANNOT be undone. args: {\"source\": number (a draft in the sources, required)}. Only propose this when the user clearly and explicitly asks to send, and only for a draft that already exists — if there is no draft yet, write one first with draft_email or draft_reply and let the user send it. The user still approves before anything is sent.\n\
- move_to_folder: move an email into one of the user's own mail folders. args: {\"source\": number, \"folder\": string}. Set \"folder\" to EXACTLY one of the folder names listed under \"Folders\" below — never invent a folder. If the user names a folder that is not in that list, ANSWER instead and say that folder does not exist. Prefer the dedicated tools for Archive (archive_email) and Trash (trash_email).\n";

/// The rules that apply across every tool, and the output contract.
const AGENT_SYSTEM_RULES: &str = "\
For any tool that acts on an email, set \"source\" to the number [n] of that email in the numbered sources above; only propose it when the relevant email is present in the sources. \
Resolve any relative date or time (today, tomorrow 3pm, next Friday) against the current date given below into an absolute value (YYYY-MM-DD for a task due, RFC 3339 UTC for an event). \
If the request needs an action no tool covers, ANSWER instead and say you cannot do that yet. Write the answer/say text in the user's language. Output ONLY the JSON object — no markdown, no code fences, no preamble.";

/// The whole system prompt: the core tools, then each product's, then the rules
/// that hold across all of them.
///
/// Built rather than written out so a product agent (ADR 0034) is a tool list
/// plus a paragraph in its own module — the seam every wave after B1 adds to.
#[must_use]
pub fn system_prompt() -> String {
    format!(
        "{AGENT_SYSTEM_HEAD}{AGENT_SYSTEM_TOOLS}{BILLING_TOOL_DOC}{CRM_TOOL_DOC}{PROJECTS_TOOL_DOC}\
         {FINANCE_TOOL_DOC}{INVENTORY_TOOL_DOC}{DRIVE_TOOL_DOC}{AGENDA_TOOL_DOC}{CHAT_TOOL_DOC}\
         {CONTACTS_TOOL_DOC}\
         {BILLING_GUIDANCE}{CRM_GUIDANCE}{PROJECTS_GUIDANCE}{FINANCE_GUIDANCE}{INVENTORY_GUIDANCE}\
         {DRIVE_GUIDANCE}{AGENDA_GUIDANCE}{CHAT_GUIDANCE}{CONTACTS_GUIDANCE}\
         {AGENT_SYSTEM_RULES}"
    )
}

/// Whether `tool` is a tool the agent may execute — the allowlist the execution
/// boundary asks, across every product's set.
///
/// One question rather than one list per product, so a caller (the jmap execute
/// route) cannot check some of them and forget another.
#[must_use]
pub fn is_agent_tool(tool: &str) -> bool {
    AGENT_TOOLS.contains(&tool)
        || BILLING_TOOLS.contains(&tool)
        || CRM_TOOLS.contains(&tool)
        || PROJECTS_TOOLS.contains(&tool)
        || FINANCE_TOOLS.contains(&tool)
        || INVENTORY_TOOLS.contains(&tool)
        || DRIVE_TOOLS.contains(&tool)
        || AGENDA_TOOLS.contains(&tool)
        || CHAT_TOOLS.contains(&tool)
        || CONTACTS_TOOLS.contains(&tool)
}

/// The chat messages for one agent turn. Pure and exported so the prompt is
/// testable without a backend. `today` is the caller's current date
/// (`YYYY-MM-DD`) so the model can resolve relative dates like "tomorrow";
/// `folders` are the user's mail-folder names, so `move_to_folder` can only
/// target a real one.
#[must_use]
pub fn agent_messages(
    request: &str,
    sources: &[WorkspaceSource],
    today: &str,
    folders: &[String],
) -> Vec<ChatMessage> {
    let folder_line = if folders.is_empty() {
        String::new()
    } else {
        format!("\n\nFolders (for move_to_folder): {}", folders.join(", "))
    };
    let user = format!(
        "Today's date is {}.\nRequest: {}\n\nSources:\n{}{}",
        today.trim(),
        request.trim(),
        render_sources(sources),
        folder_line
    );
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: system_prompt(),
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

/// Run one agent turn: build the prompt from the request + access-scoped sources,
/// call the model, and parse its decision. Returns a decision to PROPOSE — it
/// never executes anything.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty.
pub async fn run_agent(
    config: &AiConfig,
    request: &str,
    sources: &[WorkspaceSource],
    today: &str,
    folders: &[String],
) -> Result<AgentDecision, InferenceError> {
    let text = chat(
        config,
        &agent_messages(request, sources, today, folders),
        0.2,
    )
    .await?;
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

    #[test]
    fn prompt_carries_request_sources_date_and_the_tool() {
        let folders = ["Work".to_owned(), "Receipts".to_owned()];
        let msgs = agent_messages(
            "book a slot",
            &[src(1, "message", "Acme thread")],
            "2026-08-07",
            &folders,
        );
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
        let msgs = agent_messages("hi", &[], "2026-08-07", &[]);
        assert!(!msgs[1].content.contains("Folders (for move_to_folder)"));
    }

    #[test]
    fn every_executable_tool_is_described_and_every_described_tool_is_executable() {
        // The allowlist and the prompt are one surface: a tool the model is
        // told about but the execute route refuses is a dead proposal, and a
        // tool it is never told about is dead code.
        let prompt = system_prompt();
        for tool in AGENT_TOOLS
            .iter()
            .chain(BILLING_TOOLS)
            .chain(CRM_TOOLS)
            .chain(PROJECTS_TOOLS)
            .chain(FINANCE_TOOLS)
            .chain(INVENTORY_TOOLS)
            .chain(DRIVE_TOOLS)
            .chain(AGENDA_TOOLS)
            .chain(CHAT_TOOLS)
            .chain(CONTACTS_TOOLS)
        {
            assert!(prompt.contains(&format!("- {tool}:")), "{tool} undescribed");
            assert!(is_agent_tool(tool), "{tool} is not allowed to execute");
        }
        assert_eq!(
            prompt.matches("\n- ").count(),
            AGENT_TOOLS.len()
                + BILLING_TOOLS.len()
                + CRM_TOOLS.len()
                + PROJECTS_TOOLS.len()
                + FINANCE_TOOLS.len()
                + INVENTORY_TOOLS.len()
                + DRIVE_TOOLS.len()
                + AGENDA_TOOLS.len()
                + CHAT_TOOLS.len()
                + CONTACTS_TOOLS.len(),
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

    #[test]
    fn the_output_contract_stays_the_last_thing_the_model_reads() {
        let prompt = system_prompt();
        assert!(prompt.ends_with("no preamble."));
        // A product's tools come after the core ones, and its guidance after
        // its tools — the order the prompt is assembled in.
        let at = |needle: &str| prompt.find(needle).unwrap_or(usize::MAX);
        assert!(at("- create_task:") < at("- create_invoice_draft:"));
        assert!(at("- create_invoice_draft:") < at("- create_deal:"));
        assert!(at("- create_deal:") < at("- log_time:"));
        assert!(at("- log_time:") < at("- categorise_transactions:"));
        assert!(at("- categorise_transactions:") < at("- reorder_proposals:"));
        // Every tool line comes before every product's guidance, so a model
        // reads the whole menu before it reads how to fill an order from it.
        assert!(at("- reorder_proposals:") < at("For a billing tool"));
        assert!(at("For a billing tool") < at("For a CRM tool"));
        assert!(at("For a CRM tool") < at("For a projects tool"));
        assert!(at("For a projects tool") < at("For a finance tool"));
        assert!(at("For a finance tool") < at("For an inventory tool"));
        assert!(at("For an inventory tool") < at("Output ONLY the JSON object"));
    }
}
