//! Which tools belong to which product (ADR 0034) — the registry that turns
//! [`AgentProduct`] into a tool set, a prompt and, at the far end, a refusal.
//!
//! Before this, `run_agent` took no agent and no scope, so every agent was
//! offered all thirty-three tools and "the Inventory agent" was a name on a
//! generic assistant. The scoping is read twice, and the readings are not
//! equally important:
//!
//! - [`tools_for`] builds the **prompt**, so an agent is told about its own
//!   product's tools and no others. That is a courtesy to the model.
//! - [`offers`] is asked at the **execution boundary**
//!   (`alo-jmap`'s `execute_tool`), which refuses everything else whatever the
//!   model returned. That is the permission system. A prompt that asks nicely
//!   is not one: the model is the untrusted party, and an injected turn will
//!   name a tool it was never offered.
//!
//! Both read this one table, so they cannot disagree about what an agent is
//! allowed to do.

use alo_store::AgentProduct;

use crate::agent_agenda::{AGENDA_GUIDANCE, AGENDA_TOOL_DOC, AGENDA_TOOLS};
use crate::agent_chat::{CHAT_GUIDANCE, CHAT_TOOL_DOC, CHAT_TOOLS};
use crate::agent_contacts::{CONTACTS_GUIDANCE, CONTACTS_TOOL_DOC, CONTACTS_TOOLS};
use crate::agent_crm::{CRM_GUIDANCE, CRM_TOOL_DOC, CRM_TOOLS};
use crate::agent_docs::{DOCS_GUIDANCE, DOCS_TOOL_DOC, DOCS_TOOLS};
use crate::agent_drive::{DRIVE_GUIDANCE, DRIVE_TOOL_DOC, DRIVE_TOOLS};
use crate::agent_finance::{FINANCE_GUIDANCE, FINANCE_TOOL_DOC, FINANCE_TOOLS};
use crate::agent_hr::{HR_GUIDANCE, HR_TOOL_DOC, HR_TOOLS};
use crate::agent_insights::{INSIGHTS_GUIDANCE, INSIGHTS_TOOL_DOC, INSIGHTS_TOOLS};
use crate::agent_inventory::{INVENTORY_GUIDANCE, INVENTORY_TOOL_DOC, INVENTORY_TOOLS};
use crate::agent_mail::{MAIL_GUIDANCE, MAIL_TOOL_DOC, MAIL_TOOLS};
use crate::agent_meet::{MEET_GUIDANCE, MEET_TOOL_DOC, MEET_TOOLS};
use crate::agent_projects::{PROJECTS_GUIDANCE, PROJECTS_TOOL_DOC, PROJECTS_TOOLS};
use crate::agent_sheets::{SHEETS_GUIDANCE, SHEETS_TOOL_DOC, SHEETS_TOOLS};
use crate::agent_sites::{SITES_GUIDANCE, SITES_TOOL_DOC, SITES_TOOLS};
use crate::agent_tasks::{TASKS_GUIDANCE, TASKS_TOOL_DOC, TASKS_TOOLS};
use crate::agent_tool::AgentTool;
use crate::billing_intents::BILLING as BILLING_INTENTS;
use crate::intent::IntentModule;

/// One module's contribution to a product's agent: what it may do, how each
/// tool is described, and the rules that keep a proposal from it honest.
///
/// A product is usually one of these. Mail is two — the address book
/// ([`crate::agent_contacts`]) is Mail's, and lives in its own module because
/// it is its own subject matter, not because it is its own agent.
#[derive(Debug, Clone, Copy)]
pub struct ToolSet {
    /// Hand-written tools, each carrying its own read/write effect (ADR 0047
    /// §1) — the shape every module had before intents; empty once a module
    /// has moved to [`IntentModule`].
    static_tools: &'static [AgentTool],
    /// Hand-written `- name: …` lines the model reads; empty once moved.
    static_doc: &'static str,
    /// The module's verbs (ADR 0058), from which tools and doc lines render.
    intents: Option<&'static IntentModule>,
    /// The paragraph appended after every product's tool lines.
    pub guidance: &'static str,
}

impl ToolSet {
    /// The tools, in prompt order.
    #[must_use]
    pub fn tools(&self) -> Vec<AgentTool> {
        let mut out = self.static_tools.to_vec();
        if let Some(module) = self.intents {
            out.extend(module.tools());
        }
        out
    }

    /// The `- name: …` lines the model reads.
    #[must_use]
    pub fn doc(&self) -> String {
        let mut out = self.static_doc.to_owned();
        if let Some(module) = self.intents {
            out.push_str(&module.doc());
        }
        out
    }

    /// The verbs behind this set, when the module has moved to intents.
    #[must_use]
    pub fn intents(&self) -> Option<&'static IntentModule> {
        self.intents
    }
}

/// One module's three constants, gathered.
const fn set(tools: &'static [AgentTool], doc: &'static str, guidance: &'static str) -> ToolSet {
    ToolSet {
        static_tools: tools,
        static_doc: doc,
        intents: None,
        guidance,
    }
}

/// A module that has moved to intents (ADR 0058): everything renders from
/// its [`IntentModule`].
const fn intents(module: &'static IntentModule) -> ToolSet {
    ToolSet {
        static_tools: &[],
        static_doc: "",
        intents: Some(module),
        guidance: module.guidance,
    }
}

const MAIL_SET: ToolSet = set(MAIL_TOOLS, MAIL_TOOL_DOC, MAIL_GUIDANCE);
/// The address book is Mail's, in its own module because it is its own subject
/// matter — not because it is its own agent.
const CONTACTS_SET: ToolSet = set(CONTACTS_TOOLS, CONTACTS_TOOL_DOC, CONTACTS_GUIDANCE);
const AGENDA_SET: ToolSet = set(AGENDA_TOOLS, AGENDA_TOOL_DOC, AGENDA_GUIDANCE);
const TASKS_SET: ToolSet = set(TASKS_TOOLS, TASKS_TOOL_DOC, TASKS_GUIDANCE);
const CHAT_SET: ToolSet = set(CHAT_TOOLS, CHAT_TOOL_DOC, CHAT_GUIDANCE);
const DRIVE_SET: ToolSet = set(DRIVE_TOOLS, DRIVE_TOOL_DOC, DRIVE_GUIDANCE);
const BILLING_SET: ToolSet = intents(&BILLING_INTENTS);
const CRM_SET: ToolSet = set(CRM_TOOLS, CRM_TOOL_DOC, CRM_GUIDANCE);
const PROJECTS_SET: ToolSet = set(PROJECTS_TOOLS, PROJECTS_TOOL_DOC, PROJECTS_GUIDANCE);
const FINANCE_SET: ToolSet = set(FINANCE_TOOLS, FINANCE_TOOL_DOC, FINANCE_GUIDANCE);
const INVENTORY_SET: ToolSet = set(INVENTORY_TOOLS, INVENTORY_TOOL_DOC, INVENTORY_GUIDANCE);
const HR_SET: ToolSet = set(HR_TOOLS, HR_TOOL_DOC, HR_GUIDANCE);
const SITES_SET: ToolSet = set(SITES_TOOLS, SITES_TOOL_DOC, SITES_GUIDANCE);
/// alo Sheets, whose agent works on a spreadsheet the caller can already open —
/// a Drive node, which is also what gates it (`AgentProduct::module`).
const SHEETS_SET: ToolSet = set(SHEETS_TOOLS, SHEETS_TOOL_DOC, SHEETS_GUIDANCE);
/// alo Docs, on exactly the same footing: a document is a Drive node too, so
/// the same switch gates it and the same reads-answer/writes-propose split
/// applies to it (A2.3).
const DOCS_SET: ToolSet = set(DOCS_TOOLS, DOCS_TOOL_DOC, DOCS_GUIDANCE);
/// alo Insights, whose agent reads the figures through the same query engine
/// the boards do and writes nothing but a board of questions (A2.4).
const INSIGHTS_SET: ToolSet = set(INSIGHTS_TOOLS, INSIGHTS_TOOL_DOC, INSIGHTS_GUIDANCE);
/// alo Meet, whose agent works on a meeting **after** it is over — the record
/// it left behind, and the conversation it came out of (A3.2). Nothing here
/// joins a call: the live participant is a media path and is not decided.
const MEET_SET: ToolSet = set(MEET_TOOLS, MEET_TOOL_DOC, MEET_GUIDANCE);

/// Mail's, including the address book.
const MAIL: &[ToolSet] = &[MAIL_SET, CONTACTS_SET];
const AGENDA: &[ToolSet] = &[AGENDA_SET];
const TASKS: &[ToolSet] = &[TASKS_SET];
const CHAT: &[ToolSet] = &[CHAT_SET];
const DRIVE: &[ToolSet] = &[DRIVE_SET];
const SHEETS: &[ToolSet] = &[SHEETS_SET];
const DOCS: &[ToolSet] = &[DOCS_SET];
const BILLING: &[ToolSet] = &[BILLING_SET];
const CRM: &[ToolSet] = &[CRM_SET];
const PROJECTS: &[ToolSet] = &[PROJECTS_SET];
const FINANCE: &[ToolSet] = &[FINANCE_SET];
const INVENTORY: &[ToolSet] = &[INVENTORY_SET];
const HR: &[ToolSet] = &[HR_SET];
const SITES: &[ToolSet] = &[SITES_SET];
const INSIGHTS: &[ToolSet] = &[INSIGHTS_SET];
const MEET: &[ToolSet] = &[MEET_SET];

/// Every product's tool sets, in the order [`AgentProduct::Workspace`] renders
/// them.
///
/// One table. Adding a product's agent is filling in one row, and the prompt,
/// the allowlist and the boundary all follow from it.
#[must_use]
pub fn tool_sets(product: AgentProduct) -> &'static [ToolSet] {
    match product {
        AgentProduct::Mail => MAIL,
        AgentProduct::Agenda => AGENDA,
        AgentProduct::Tasks => TASKS,
        AgentProduct::Chat => CHAT,
        AgentProduct::Drive => DRIVE,
        AgentProduct::Sheets => SHEETS,
        AgentProduct::Docs => DOCS,
        AgentProduct::Billing => BILLING,
        AgentProduct::Crm => CRM,
        AgentProduct::Projects => PROJECTS,
        AgentProduct::Finance => FINANCE,
        AgentProduct::Inventory => INVENTORY,
        AgentProduct::Hr => HR,
        AgentProduct::Sites => SITES,
        AgentProduct::Insights => INSIGHTS,
        AgentProduct::Meet => MEET,
        // Ask alo works across products, so it is offered all of them — the
        // one agent for which that is the decision rather than the default
        // (ADR 0034).
        AgentProduct::Workspace => WORKSPACE,
    }
}

/// Every product's sets, concatenated, for "Ask alo" — in the order
/// `ALL_AGENT_PRODUCTS` lists the products, which a test holds it to.
const WORKSPACE: &[ToolSet] = &[
    MAIL_SET,
    CONTACTS_SET,
    AGENDA_SET,
    TASKS_SET,
    CHAT_SET,
    DRIVE_SET,
    SHEETS_SET,
    DOCS_SET,
    BILLING_SET,
    CRM_SET,
    PROJECTS_SET,
    FINANCE_SET,
    INVENTORY_SET,
    HR_SET,
    INSIGHTS_SET,
    MEET_SET,
    SITES_SET,
];

/// Who the agent is, said in the first line of its system prompt.
///
/// Not decoration and not a permission: the boundary refuses another product's
/// tool whatever this says. It is here so an agent's *answers* stay in its own
/// subject matter — asked something another product owns, it should say which
/// agent to ask rather than answering plausibly from a search snippet, which
/// the journal names as a failure rather than a partial success.
#[must_use]
pub fn headline(product: AgentProduct) -> &'static str {
    match product {
        AgentProduct::Mail => {
            "You are the alo Mail agent. You work in this person's mail and their address book."
        }
        AgentProduct::Agenda => "You are the alo Agenda agent. You work in this person's calendar.",
        AgentProduct::Tasks => "You are the alo Tasks agent. You work in this person's to-do list.",
        AgentProduct::Chat => {
            "You are the alo Chat agent. You work in the conversations this person can already read."
        }
        AgentProduct::Drive => "You are the alo Drive agent. You work in this person's files.",
        AgentProduct::Sheets => {
            "You are the alo Sheets agent. You work in this person's spreadsheets — the cells, the formulas and the figures in them."
        }
        AgentProduct::Docs => {
            "You are the alo Docs agent. You work in this person's documents — the sections, the paragraphs and the words in them."
        }
        AgentProduct::Billing => {
            "You are the alo Billing agent. You work in this company's quotes and invoices."
        }
        AgentProduct::Crm => {
            "You are the alo CRM agent. You work in this company's contacts, deals and sales board."
        }
        AgentProduct::Projects => {
            "You are the alo Projects agent. You work in this company's projects, tasks and timesheets."
        }
        AgentProduct::Finance => {
            "You are the alo Finance agent. You work in this company's books — its journal, its expenses and its VAT."
        }
        AgentProduct::Inventory => {
            "You are the alo Inventory agent. You work in this company's stock, suppliers and purchase orders."
        }
        AgentProduct::Hr => {
            "You are the alo People agent. You work in this company's staff records, absences and letters."
        }
        AgentProduct::Insights => {
            "You are the alo Insights agent. You work in this company's reports and figures."
        }
        AgentProduct::Meet => "You are the alo Meet agent. You work in this person's meetings.",
        AgentProduct::Sites => "You are the alo Website agent. You work in this company's website.",
        AgentProduct::Workspace => "You are alo, the assistant across the user's entire workspace.",
    }
}

/// Said to every product agent but "Ask alo": stay in your own subject matter.
///
/// Appended by [`crate::agent::system_prompt_for`] rather than written into
/// each headline, so the rule is stated once.
pub(crate) const STAY_IN_PRODUCT: &str = " A question about another part of the workspace is not yours to answer from the sources: say which part it belongs to and that the user can ask that agent, rather than answering plausibly from something a search happened to match.";

/// The tools this product's agent may use, in prompt order.
#[must_use]
pub fn tools_for(product: AgentProduct) -> Vec<AgentTool> {
    let mut out = Vec::new();
    for set in tool_sets(product) {
        out.extend(set.tools());
    }
    out
}

/// Whether this product's agent may use `tool` — **the execution boundary's
/// question** (A1.2).
///
/// Asked of the registry and of nothing else: not of the prompt the agent was
/// given, not of the tool's name, and never of the model's own account of what
/// it is doing. A name no product declares is refused by every product,
/// including [`AgentProduct::Workspace`].
#[must_use]
pub fn offers(product: AgentProduct, tool: &str) -> bool {
    tool_sets(product)
        .iter()
        .any(|set| set.tools().iter().any(|entry| entry.name == tool))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::ALL_AGENT_PRODUCTS;

    /// The whole registry, product by product. Written out rather than derived,
    /// so moving a tool between products is a visible change to this list and
    /// not a silent widening of somebody's reach.
    #[test]
    fn each_product_offers_exactly_its_own_tools() {
        let names = |product| {
            tools_for(product)
                .iter()
                .map(|tool| tool.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(AgentProduct::Mail),
            [
                "correspondence",
                "message_read",
                "mark_read",
                "flag_email",
                "archive_email",
                "trash_email",
                "snooze_email",
                "draft_email",
                "draft_reply",
                "send_email",
                "move_to_folder",
                "find_contact",
            ]
        );
        assert_eq!(
            names(AgentProduct::Agenda),
            [
                "whats_on",
                "am_i_free",
                "find_a_time",
                "meeting_prep",
                "create_event",
                "reschedule_event",
            ]
        );
        assert_eq!(
            names(AgentProduct::Tasks),
            [
                "my_plate",
                "overdue_by_owner",
                "thread_actions",
                "create_task",
                "set_task_priority",
                "chase_task",
                "capture_actions",
            ]
        );
        assert_eq!(names(AgentProduct::Chat), ["catch_up_room", "find_in_chat"]);
        assert_eq!(
            names(AgentProduct::Drive),
            [
                "find_file",
                "file_read",
                "attachment_read",
                "file_rename",
                "file_move",
            ]
        );
        assert_eq!(
            names(AgentProduct::Sheets),
            [
                "sheet_read",
                "sheet_answer",
                "sheet_formula_explain",
                "sheet_write_formula",
                "sheet_clean_column",
            ]
        );
        assert_eq!(
            names(AgentProduct::Docs),
            ["doc_read", "doc_answer", "doc_draft_section", "doc_rewrite"]
        );
        assert_eq!(
            names(AgentProduct::Billing),
            [
                "open_quotes",
                "quote_lookup",
                "customer_lookup",
                "unpaid_invoices",
                "invoice_lookup",
                "billing_totals",
                "create_invoice_draft",
                "quote_to_invoice",
                "draft_payment_reminder",
                "send_quote",
                "issue_invoice",
                "record_payment"
            ]
        );
        assert_eq!(
            names(AgentProduct::Crm),
            ["create_deal", "move_deal_stage", "draft_followup"]
        );
        assert_eq!(
            names(AgentProduct::Projects),
            [
                "log_time",
                "project_status_summary",
                "draft_timesheet_from_calendar"
            ]
        );
        assert_eq!(
            names(AgentProduct::Finance),
            ["categorise_transactions", "vat_summary", "flag_anomalies"]
        );
        assert_eq!(
            names(AgentProduct::Inventory),
            ["reorder_proposals", "stock_answer"]
        );
        assert_eq!(
            names(AgentProduct::Hr),
            ["who_is_off", "draft_letter_from_template"]
        );
        assert_eq!(
            names(AgentProduct::Insights),
            [
                "insight_catalog",
                "insight_answer",
                "insight_change",
                "insight_report",
            ]
        );
        assert_eq!(
            names(AgentProduct::Sites),
            [
                "site_answer",
                "site_page_read",
                "site_seo_review",
                "site_translation_status",
                "site_page_draft",
                "site_page_edit",
                "site_publish",
            ]
        );
        assert_eq!(
            names(AgentProduct::Meet),
            ["meetings_recent", "meeting_record", "meeting_minutes"]
        );
    }

    /// Every tool belongs to exactly one product, and Ask alo is the union.
    /// A tool in two products would be reachable from an agent nobody meant to
    /// give it to; a tool in none would be dead code the boundary refuses.
    #[test]
    fn workspace_is_every_product_once() {
        let mut owned: Vec<&str> = Vec::new();
        for product in ALL_AGENT_PRODUCTS {
            if product == AgentProduct::Workspace {
                continue;
            }
            owned.extend(tools_for(product).iter().map(|tool| tool.name));
        }
        let mut sorted = owned.clone();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "a tool belongs to two products");

        let workspace: Vec<&str> = tools_for(AgentProduct::Workspace)
            .iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(workspace, owned, "Ask alo is every product, in order");
        assert_eq!(workspace.len(), 80);
    }

    /// The boundary's question, over the whole registry: a product offers its
    /// own tools and refuses every other product's.
    #[test]
    fn a_product_refuses_every_other_products_tools() {
        for product in ALL_AGENT_PRODUCTS {
            let mine: Vec<&str> = tools_for(product).iter().map(|t| t.name).collect();
            for other in ALL_AGENT_PRODUCTS {
                for tool in tools_for(other) {
                    assert_eq!(
                        offers(product, tool.name),
                        mine.contains(&tool.name),
                        "{product} and {}",
                        tool.name
                    );
                }
            }
            // And a name no product declares is refused by all of them,
            // Workspace included.
            for stranger in ["", "create_task ", "delete_everything", "read_payroll"] {
                assert!(!offers(product, stranger), "{product} offered {stranger:?}");
            }
        }
        // Stated plainly, because it is the property A1.2 exists for.
        assert!(!offers(AgentProduct::Inventory, "send_email"));
        assert!(!offers(AgentProduct::Mail, "stock_answer"));
        assert!(offers(AgentProduct::Inventory, "stock_answer"));
        assert!(offers(AgentProduct::Workspace, "send_email"));
        // The one A2.1 adds: putting a site on the internet belongs to the
        // Website agent and to nobody else's.
        assert!(offers(AgentProduct::Sites, "site_publish"));
        assert!(!offers(AgentProduct::Mail, "site_publish"));
        assert!(!offers(AgentProduct::Sites, "send_email"));
        // …and the one A2.3 adds: rewriting somebody's prose belongs to the
        // Docs agent, and the two Drive-node products do not share a tool set
        // just because they share a gate.
        assert!(offers(AgentProduct::Docs, "doc_rewrite"));
        assert!(!offers(AgentProduct::Sheets, "doc_rewrite"));
        assert!(!offers(AgentProduct::Docs, "sheet_write_formula"));
        // …and the one A2.4 adds: pinning a board of questions belongs to the
        // Insights agent, whose reads are over figures the other agents' own
        // products own and may not be reached from them.
        assert!(offers(AgentProduct::Insights, "insight_report"));
        assert!(!offers(AgentProduct::Billing, "insight_answer"));
        assert!(!offers(AgentProduct::Insights, "create_invoice_draft"));
        // …and the ones A2.5 adds. Reading a *file* is Drive's and reading a
        // *document by its blocks* is Docs': the two are different tools rather
        // than one tool in two products, which is what
        // `workspace_is_every_product_once` refuses. Moving and renaming are
        // Drive's alone — a document agent that could move somebody's file is a
        // second place the same decision is made.
        assert!(offers(AgentProduct::Drive, "file_read"));
        assert!(offers(AgentProduct::Drive, "file_move"));
        assert!(!offers(AgentProduct::Docs, "file_read"));
        assert!(!offers(AgentProduct::Docs, "file_move"));
        assert!(!offers(AgentProduct::Drive, "doc_read"));
        // An attachment is a file that has not been filed yet, so pulling text
        // out of one is Drive's rather than Mail's (A2.8 owns correspondence).
        assert!(offers(AgentProduct::Drive, "attachment_read"));
        assert!(!offers(AgentProduct::Mail, "attachment_read"));
        // …and the two A2.8 adds, which are the other side of that line:
        // reading a *message* of the asker's own correspondence is Mail's, and
        // no other product may reach it. The Agenda agent's briefing opens the
        // mail that goes with a meeting through `meeting_prep`, which is its
        // own tool with its own bound — not by borrowing this one.
        assert!(offers(AgentProduct::Mail, "correspondence"));
        assert!(offers(AgentProduct::Mail, "message_read"));
        assert!(!offers(AgentProduct::Drive, "message_read"));
        assert!(!offers(AgentProduct::Agenda, "correspondence"));
        assert!(!offers(AgentProduct::Crm, "correspondence"));
        // …and the ones A2.6 adds. Looking across several diaries and moving a
        // meeting that is already in one are Agenda's; a meeting is not a task
        // and not a room, so neither of the products that also deal in
        // scheduled things may reach them.
        assert!(offers(AgentProduct::Agenda, "find_a_time"));
        assert!(offers(AgentProduct::Agenda, "reschedule_event"));
        assert!(!offers(AgentProduct::Tasks, "reschedule_event"));
        assert!(!offers(AgentProduct::Chat, "find_a_time"));
        // Preparing a meeting reads the mail that goes with it, which does not
        // make it a Mail tool: the Mail agent has no way to move a meeting and
        // the Agenda agent has no way to send one an email.
        assert!(offers(AgentProduct::Agenda, "meeting_prep"));
        assert!(!offers(AgentProduct::Mail, "meeting_prep"));
        assert!(!offers(AgentProduct::Agenda, "send_email"));
        // …and the ones A2.7 adds. Reading a room to write down what was agreed
        // in it is Tasks' — it ends in a task, and the Chat agent has no way to
        // make one; chasing somebody is a comment on a task rather than a
        // message, so it is not Chat's either, and Projects, which also deals
        // in tasks, does not share the personal list's tools.
        assert!(offers(AgentProduct::Tasks, "thread_actions"));
        assert!(offers(AgentProduct::Tasks, "chase_task"));
        assert!(!offers(AgentProduct::Chat, "thread_actions"));
        assert!(!offers(AgentProduct::Chat, "capture_actions"));
        assert!(!offers(AgentProduct::Projects, "my_plate"));
        assert!(!offers(AgentProduct::Tasks, "catch_up_room"));
        assert!(!offers(AgentProduct::Tasks, "project_status_summary"));
        // …and the ones A3.2 adds, which are the whole of "no second
        // mechanism": Meet may write the minutes of a sitting into the room it
        // came out of, and it may not put a task on anybody's board or an entry
        // in anybody's diary — those stay the Tasks and Agenda agents' own
        // proposals, accepted one at a time. Reading a meeting's record is
        // Meet's alone: a meeting in the diary is an appointment, and what was
        // said inside one is not the Agenda agent's to read.
        assert!(offers(AgentProduct::Meet, "meeting_record"));
        assert!(offers(AgentProduct::Meet, "meeting_minutes"));
        assert!(!offers(AgentProduct::Meet, "create_task"));
        assert!(!offers(AgentProduct::Meet, "capture_actions"));
        assert!(!offers(AgentProduct::Meet, "create_event"));
        assert!(!offers(AgentProduct::Agenda, "meeting_record"));
        assert!(!offers(AgentProduct::Chat, "meeting_minutes"));
        assert!(!offers(AgentProduct::Meet, "meeting_prep"));
    }
}
