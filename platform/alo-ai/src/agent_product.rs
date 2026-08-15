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
use crate::agent_billing::{BILLING_GUIDANCE, BILLING_TOOL_DOC, BILLING_TOOLS};
use crate::agent_chat::{CHAT_GUIDANCE, CHAT_TOOL_DOC, CHAT_TOOLS};
use crate::agent_contacts::{CONTACTS_GUIDANCE, CONTACTS_TOOL_DOC, CONTACTS_TOOLS};
use crate::agent_crm::{CRM_GUIDANCE, CRM_TOOL_DOC, CRM_TOOLS};
use crate::agent_drive::{DRIVE_GUIDANCE, DRIVE_TOOL_DOC, DRIVE_TOOLS};
use crate::agent_finance::{FINANCE_GUIDANCE, FINANCE_TOOL_DOC, FINANCE_TOOLS};
use crate::agent_hr::{HR_GUIDANCE, HR_TOOL_DOC, HR_TOOLS};
use crate::agent_inventory::{INVENTORY_GUIDANCE, INVENTORY_TOOL_DOC, INVENTORY_TOOLS};
use crate::agent_mail::{MAIL_GUIDANCE, MAIL_TOOL_DOC, MAIL_TOOLS};
use crate::agent_projects::{PROJECTS_GUIDANCE, PROJECTS_TOOL_DOC, PROJECTS_TOOLS};
use crate::agent_sheets::{SHEETS_GUIDANCE, SHEETS_TOOL_DOC, SHEETS_TOOLS};
use crate::agent_sites::{SITES_GUIDANCE, SITES_TOOL_DOC, SITES_TOOLS};
use crate::agent_tasks::{TASKS_GUIDANCE, TASKS_TOOL_DOC, TASKS_TOOLS};
use crate::agent_tool::AgentTool;

/// One module's contribution to a product's agent: what it may do, how each
/// tool is described, and the rules that keep a proposal from it honest.
///
/// A product is usually one of these. Mail is two — the address book
/// ([`crate::agent_contacts`]) is Mail's, and lives in its own module because
/// it is its own subject matter, not because it is its own agent.
#[derive(Debug, Clone, Copy)]
pub struct ToolSet {
    /// The tools, each carrying its own read/write effect (ADR 0047 §1).
    pub tools: &'static [AgentTool],
    /// The `- name: …` lines the model reads.
    pub doc: &'static str,
    /// The paragraph appended after every product's tool lines.
    pub guidance: &'static str,
}

/// One module's three constants, gathered.
const fn set(tools: &'static [AgentTool], doc: &'static str, guidance: &'static str) -> ToolSet {
    ToolSet {
        tools,
        doc,
        guidance,
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
const BILLING_SET: ToolSet = set(BILLING_TOOLS, BILLING_TOOL_DOC, BILLING_GUIDANCE);
const CRM_SET: ToolSet = set(CRM_TOOLS, CRM_TOOL_DOC, CRM_GUIDANCE);
const PROJECTS_SET: ToolSet = set(PROJECTS_TOOLS, PROJECTS_TOOL_DOC, PROJECTS_GUIDANCE);
const FINANCE_SET: ToolSet = set(FINANCE_TOOLS, FINANCE_TOOL_DOC, FINANCE_GUIDANCE);
const INVENTORY_SET: ToolSet = set(INVENTORY_TOOLS, INVENTORY_TOOL_DOC, INVENTORY_GUIDANCE);
const HR_SET: ToolSet = set(HR_TOOLS, HR_TOOL_DOC, HR_GUIDANCE);
const SITES_SET: ToolSet = set(SITES_TOOLS, SITES_TOOL_DOC, SITES_GUIDANCE);
/// alo Sheets, whose agent works on a spreadsheet the caller can already open —
/// a Drive node, which is also what gates it (`AgentProduct::module`).
const SHEETS_SET: ToolSet = set(SHEETS_TOOLS, SHEETS_TOOL_DOC, SHEETS_GUIDANCE);

/// Mail's, including the address book.
const MAIL: &[ToolSet] = &[MAIL_SET, CONTACTS_SET];
const AGENDA: &[ToolSet] = &[AGENDA_SET];
const TASKS: &[ToolSet] = &[TASKS_SET];
const CHAT: &[ToolSet] = &[CHAT_SET];
const DRIVE: &[ToolSet] = &[DRIVE_SET];
const SHEETS: &[ToolSet] = &[SHEETS_SET];
const BILLING: &[ToolSet] = &[BILLING_SET];
const CRM: &[ToolSet] = &[CRM_SET];
const PROJECTS: &[ToolSet] = &[PROJECTS_SET];
const FINANCE: &[ToolSet] = &[FINANCE_SET];
const INVENTORY: &[ToolSet] = &[INVENTORY_SET];
const HR: &[ToolSet] = &[HR_SET];
const SITES: &[ToolSet] = &[SITES_SET];

/// A product whose agent has no tools yet.
///
/// Insights and Meet are real products with real modules; their agents are
/// queued (A2.4, A3.2) and their tool sets are what those items build. Until
/// then such an agent answers from its grounding and proposes nothing — which is
/// a truthful agent, and better than borrowing another product's tools to look
/// busy.
const NONE_YET: &[ToolSet] = &[];

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
        AgentProduct::Billing => BILLING,
        AgentProduct::Crm => CRM,
        AgentProduct::Projects => PROJECTS,
        AgentProduct::Finance => FINANCE,
        AgentProduct::Inventory => INVENTORY,
        AgentProduct::Hr => HR,
        AgentProduct::Sites => SITES,
        AgentProduct::Insights | AgentProduct::Meet => NONE_YET,
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
    BILLING_SET,
    CRM_SET,
    PROJECTS_SET,
    FINANCE_SET,
    INVENTORY_SET,
    HR_SET,
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
        out.extend_from_slice(set.tools);
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
        .any(|set| set.tools.iter().any(|entry| entry.name == tool))
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
            ["whats_on", "am_i_free", "create_event"]
        );
        assert_eq!(names(AgentProduct::Tasks), ["create_task"]);
        assert_eq!(names(AgentProduct::Chat), ["catch_up_room", "find_in_chat"]);
        assert_eq!(names(AgentProduct::Drive), ["find_file"]);
        assert_eq!(
            names(AgentProduct::Billing),
            [
                "create_invoice_draft",
                "quote_to_invoice",
                "draft_payment_reminder"
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
        for empty in [AgentProduct::Insights, AgentProduct::Meet] {
            assert!(tools_for(empty).is_empty(), "{empty} has no tools yet");
        }
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
        assert_eq!(workspace.len(), 45);
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
    }
}
