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

use alo_store::{ALL_AGENT_PRODUCTS, AgentProduct};

use crate::agenda_intents::AGENDA as AGENDA_INTENTS;
use crate::agent_tool::AgentTool;
use crate::billing_intents::BILLING as BILLING_INTENTS;
use crate::chat_intents::CHAT as CHAT_INTENTS;
use crate::crm_intents::CRM as CRM_INTENTS;
use crate::docs_intents::DOCS as DOCS_INTENTS;
use crate::drive_intents::DRIVE as DRIVE_INTENTS;
use crate::finance_intents::FINANCE as FINANCE_INTENTS;
use crate::hr_intents::HR as HR_INTENTS;
use crate::insights_intents::INSIGHTS as INSIGHTS_INTENTS;
use crate::intent::IntentModule;
use crate::inventory_intents::INVENTORY as INVENTORY_INTENTS;
use crate::mail_intents::MAIL as MAIL_INTENTS;
use crate::meet_intents::MEET as MEET_INTENTS;
use crate::projects_intents::PROJECTS as PROJECTS_INTENTS;
use crate::sheets_intents::SHEETS as SHEETS_INTENTS;
use crate::sites_intents::SITES as SITES_INTENTS;
use crate::tasks_intents::TASKS as TASKS_INTENTS;

/// One module's contribution to a product's agent: what it may do, how each
/// tool is described, and the rules that keep a proposal from it honest.
///
/// A product is one of these. Until AC.4 Mail was two — the address book
/// carried its own hand-written set — which is why a product maps to a *list*
/// of sets rather than to one.
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

/// Every product's tool sets, in the order [`AgentProduct::Workspace`] renders
/// them.
///
/// One table. Adding a product's agent is filling in one row, and the prompt,
/// the allowlist and the boundary all follow from it.
/// The modules that have moved to intents (ADR 0058), one row each — the whole
/// of a module's registration in this crate (A4.1c). A loop that lands a module
/// adds its row here and nothing else in this file; two loops landing at once
/// conflict on neighbouring lines, and the resolution is to keep both. A row's
/// product sees the module as its own; Ask alo sees every row.
pub const MOVED: &[(AgentProduct, &IntentModule)] = &[
    (AgentProduct::Agenda, &AGENDA_INTENTS),
    (AgentProduct::Billing, &BILLING_INTENTS),
    (AgentProduct::Chat, &CHAT_INTENTS),
    (AgentProduct::Crm, &CRM_INTENTS),
    (AgentProduct::Docs, &DOCS_INTENTS),
    (AgentProduct::Drive, &DRIVE_INTENTS),
    (AgentProduct::Finance, &FINANCE_INTENTS),
    (AgentProduct::Hr, &HR_INTENTS),
    (AgentProduct::Insights, &INSIGHTS_INTENTS),
    (AgentProduct::Inventory, &INVENTORY_INTENTS),
    (AgentProduct::Mail, &MAIL_INTENTS),
    (AgentProduct::Meet, &MEET_INTENTS),
    (AgentProduct::Projects, &PROJECTS_INTENTS),
    (AgentProduct::Sheets, &SHEETS_INTENTS),
    (AgentProduct::Sites, &SITES_INTENTS),
    (AgentProduct::Tasks, &TASKS_INTENTS),
];

/// The hand-written sets a product still carries — empty once it has moved.
fn static_sets(product: AgentProduct) -> &'static [ToolSet] {
    match product {
        AgentProduct::Mail => &[],
        AgentProduct::Agenda => &[],
        AgentProduct::Tasks => &[],
        AgentProduct::Chat => &[],
        AgentProduct::Drive => &[],
        AgentProduct::Sheets => &[],
        AgentProduct::Docs => &[],
        AgentProduct::Billing => &[],
        AgentProduct::Crm => &[],
        AgentProduct::Projects => &[],
        AgentProduct::Finance => &[],
        AgentProduct::Inventory => &[],
        AgentProduct::Hr => &[],
        AgentProduct::Sites => &[],
        AgentProduct::Insights => &[],
        AgentProduct::Meet => &[],
        // Ask alo works across products, so it is offered all of them — the
        // one agent for which that is the decision rather than the default
        // (ADR 0034).
        AgentProduct::Workspace => &[],
    }
}

/// A product's tool sets in prompt order: what it still carries by hand, then
/// its moved modules in [`MOVED`] order. Ask alo ([`AgentProduct::Workspace`])
/// is every product once, in [`ALL_AGENT_PRODUCTS`] order — a list nobody
/// maintains by hand, so a module registered once is offered everywhere it
/// should be.
#[must_use]
pub fn tool_sets(product: AgentProduct) -> Vec<ToolSet> {
    if product == AgentProduct::Workspace {
        return ALL_AGENT_PRODUCTS
            .iter()
            .copied()
            .filter(|each| *each != AgentProduct::Workspace)
            .flat_map(tool_sets)
            .collect();
    }
    let mut out = static_sets(product).to_vec();
    out.extend(
        MOVED
            .iter()
            .filter(|(owner, _)| *owner == product)
            .map(|(_, module)| intents(module)),
    );
    out
}

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
                "unread_summary",
                "thread_lookup",
                "who_i_emailed",
                "find_contact",
                "mark_read",
                "flag_email",
                "archive_email",
                "trash_email",
                "snooze_email",
                "draft_email",
                "draft_reply",
                "send_email",
                "move_to_folder",
            ]
        );
        assert_eq!(
            names(AgentProduct::Agenda),
            [
                "whats_on",
                "am_i_free",
                "find_a_time",
                "meeting_prep",
                "event_lookup",
                "colleague_free",
                "create_event",
                "reschedule_event",
                "cancel_event",
                "respond_to_invitation",
            ]
        );
        assert_eq!(
            names(AgentProduct::Tasks),
            [
                "my_plate",
                "overdue_by_owner",
                "thread_actions",
                "board_tasks",
                "task_lookup",
                "create_task",
                "set_task_priority",
                "chase_task",
                "capture_actions",
                "complete_task",
                "reassign_task",
            ]
        );
        assert_eq!(
            names(AgentProduct::Chat),
            [
                "my_rooms",
                "unread_rooms",
                "room_members",
                "catch_up_room",
                "find_in_chat",
                "post_message",
                "create_room",
            ]
        );
        assert_eq!(
            names(AgentProduct::Drive),
            [
                "recent_files",
                "list_folder",
                "shared_with_me",
                "find_file",
                "file_read",
                "attachment_read",
                "create_folder",
                "file_rename",
                "file_move",
            ]
        );
        assert_eq!(
            names(AgentProduct::Sheets),
            [
                "list_spreadsheets",
                "sheet_read",
                "sheet_answer",
                "sheet_formula_explain",
                "sheet_write_formula",
                "sheet_clean_column",
            ]
        );
        assert_eq!(
            names(AgentProduct::Docs),
            [
                "list_documents",
                "doc_read",
                "doc_answer",
                "create_document",
                "doc_draft_section",
                "doc_rewrite",
            ]
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
            [
                "open_deals",
                "deal_lookup",
                "pipeline_summary",
                "company_history",
                "create_deal",
                "move_deal_stage",
                "draft_followup"
            ]
        );
        assert_eq!(
            names(AgentProduct::Projects),
            [
                "active_projects",
                "project_status_summary",
                "who_is_on_what",
                "time_this_week",
                "log_time",
                "draft_timesheet_from_calendar"
            ]
        );
        assert_eq!(
            names(AgentProduct::Finance),
            [
                "ledger_summary",
                "vat_summary",
                "flag_anomalies",
                "unmatched_bank_lines",
                "expenses_awaiting",
                "account_balance",
                "categorise_transactions",
                "approve_expense"
            ]
        );
        assert_eq!(
            names(AgentProduct::Inventory),
            [
                "stock_answer",
                "stock_below_minimum",
                "open_purchase_orders",
                "supplier_prices",
                "recent_moves",
                "reorder_proposals",
                "receive_delivery",
            ]
        );
        assert_eq!(
            names(AgentProduct::Hr),
            [
                "who_is_off",
                "who_works_here",
                "my_leave_balance",
                "open_leave_requests",
                "open_checklists",
                "approve_leave_request",
                "draft_letter_from_template",
            ]
        );
        assert_eq!(
            names(AgentProduct::Insights),
            [
                "insight_catalog",
                "insight_answer",
                "insight_change",
                "dashboard_tiles",
                "insight_report",
                "pin_chart",
            ]
        );
        assert_eq!(
            names(AgentProduct::Sites),
            [
                "site_answer",
                "site_pages",
                "site_status",
                "site_orders",
                "site_bookings",
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
            [
                "meetings_recent",
                "meeting_record",
                "upcoming_meetings",
                "meeting_lookup",
                "meeting_minutes",
                "schedule_meeting",
            ]
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
        assert_eq!(workspace.len(), 134);
    }

    /// A moved module registers once (A4.1c): its row in [`MOVED`] is what puts
    /// its verbs in its product's prompt and in Ask alo's, and nothing else in
    /// this file names it — the property that lets several loops land modules
    /// without editing the same match arms.
    #[test]
    fn a_moved_module_is_one_row() {
        assert!(
            MOVED
                .iter()
                .any(|(owner, _)| *owner == AgentProduct::Billing)
        );
        for (owner, module) in MOVED {
            let verbs: Vec<&str> = module.intents.iter().map(|intent| intent.name).collect();
            assert!(!verbs.is_empty(), "{owner} moved an empty module");
            let mine: Vec<&str> = tools_for(*owner).iter().map(|tool| tool.name).collect();
            for verb in &verbs {
                assert!(mine.contains(verb), "{owner} does not list its own {verb}");
                assert!(
                    offers(*owner, verb),
                    "{owner} does not offer its own {verb}"
                );
                assert!(
                    offers(AgentProduct::Workspace, verb),
                    "Ask alo does not offer {verb}"
                );
            }
        }
        let source = include_str!("agent_product.rs");
        for module in [
            concat!("AGENDA_", "INTENTS"),
            concat!("BILLING_", "INTENTS"),
            concat!("CRM_", "INTENTS"),
            concat!("DOCS_", "INTENTS"),
            concat!("DRIVE_", "INTENTS"),
            concat!("INVENTORY_", "INTENTS"),
            concat!("MAIL_", "INTENTS"),
            concat!("MEET_", "INTENTS"),
            concat!("PROJECTS_", "INTENTS"),
            concat!("SHEETS_", "INTENTS"),
            concat!("TASKS_", "INTENTS"),
        ] {
            assert_eq!(
                source.matches(module).count(),
                2,
                "{module} is named by its import and its row, nowhere else"
            );
        }
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
        // came out of, and it may not put a task on anybody's board — that
        // stays the Tasks agent's own proposals, accepted one at a time. (The
        // diary half moved with AC.2: `schedule_meeting` runs the Agenda
        // module's shared calendar write as the asker, so the mechanism is
        // still one — asserted below.) Reading a meeting's record is Meet's
        // alone: a meeting in the diary is an appointment, and what was said
        // inside one is not the Agenda agent's to read.
        assert!(offers(AgentProduct::Meet, "meeting_record"));
        assert!(offers(AgentProduct::Meet, "meeting_minutes"));
        assert!(!offers(AgentProduct::Meet, "create_task"));
        assert!(!offers(AgentProduct::Meet, "capture_actions"));
        assert!(!offers(AgentProduct::Meet, "create_event"));
        assert!(!offers(AgentProduct::Agenda, "meeting_record"));
        assert!(!offers(AgentProduct::Chat, "meeting_minutes"));
        assert!(!offers(AgentProduct::Meet, "meeting_prep"));
        // …and the ones AC.1 adds: speaking in a room is Chat's alone, and
        // only as a previewed proposal in the asker's own name. No other
        // agent may reach a room's feed, and the Chat agent still cannot
        // turn what it read there into a task or an email.
        assert!(offers(AgentProduct::Chat, "post_message"));
        assert!(offers(AgentProduct::Chat, "create_room"));
        assert!(!offers(AgentProduct::Meet, "post_message"));
        assert!(!offers(AgentProduct::Tasks, "post_message"));
        assert!(!offers(AgentProduct::Chat, "send_email"));
        assert!(!offers(AgentProduct::Chat, "create_task"));
        // …and the ones AC.2 adds: the diary ahead and one meeting's notes
        // are the Meet agent's to read, and scheduling one is its own verb —
        // which runs the Agenda module's shared calendar write as the asker,
        // so `create_event` itself still belongs to Agenda alone and Meet
        // still cannot put a task on anybody's board.
        assert!(offers(AgentProduct::Meet, "upcoming_meetings"));
        assert!(offers(AgentProduct::Meet, "meeting_lookup"));
        assert!(offers(AgentProduct::Meet, "schedule_meeting"));
        assert!(!offers(AgentProduct::Agenda, "schedule_meeting"));
        assert!(!offers(AgentProduct::Agenda, "upcoming_meetings"));
        assert!(!offers(AgentProduct::Meet, "create_event"));
        assert!(!offers(AgentProduct::Meet, "whats_on"));
        assert!(!offers(AgentProduct::Meet, "create_task"));
    }
}
