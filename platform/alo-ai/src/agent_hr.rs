//! The **HR** tool set of the agent (ADR 0034, ADR 0035 wave B6.09) — the names
//! alo HR contributes to the one agent, and the words that tell a model what
//! they take.
//!
//! The sixth product on the seam [`crate::agent_billing`] opened: a tool list
//! and a paragraph, in the product's own module. Nothing here reads, writes or
//! decides anything — the proposal is parsed by [`crate::agent`], and an
//! approved proposal is executed by the jmap layer against the caller's
//! tenant-scoped store.
//!
//! Three rules shape the wording below, and each is a mistake it exists to
//! prevent. They are stricter than any other product's because the records
//! behind them are about **people**, and a workplace assistant that gossips is
//! worse than no assistant at all.
//!
//! - **Names and days, never a reason.** The absence layer
//!   (`docs/design/hr.md` § "The absence layer, and why it is not a calendar")
//!   deliberately does not load the policy, the kind of leave or the note, so
//!   `who_is_off` cannot leak them. The description says so in the model's own
//!   words, because a model that believed it knew why somebody was away would
//!   eventually write "Amara is off sick" into a chat room — a health
//!   disclosure our own store took care never to make.
//! - **Absence is never turned into a judgement.** No count of how often
//!   somebody is off, no comparison between colleagues, no conclusion about
//!   anybody's reliability. Beyond being unkind, an inference *about a person*
//!   drawn from workplace data is exactly what the EU AI Act's Annex III 4
//!   regulates, and § *The EU AI Act posture* of the design note is a written
//!   refusal to build it.
//! - **Silence is not presence.** Somebody the layer does not name may be at a
//!   customer, on another country's public holiday, or not an employee at all.
//!   A model that answered "everybody else is in" would be stating a fact
//!   nobody read.
//! - **The company's words, never ours.** `draft_letter_from_template` fills in
//!   a letter *this tenant wrote*; it has no free-form path, and the wording
//!   below says so twice because the failure it prevents — a model composing an
//!   employment confirmation out of its own head, in a company's name, about a
//!   real person — is a document somebody would act on
//!   (`docs/design/hr.md` § "The two tools that do ship").

use crate::agent_tool::AgentTool;

/// The HR tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
///
/// `draft_letter_from_template` arrived with the templates it merges (B6.09b),
/// not before: a tool described to a model but refused by the execute route is a
/// dead proposal, and the invariant test in [`crate::agent`] holds this list and
/// the prompt to exactly the tools that can act.
pub const HR_TOOLS: &[AgentTool] = &[
    AgentTool::read("who_is_off"),
    AgentTool::write("draft_letter_from_template"),
];

/// The description of each HR tool, spliced into the agent's system prompt
/// after the Inventory tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools HR has.
pub const HR_TOOL_DOC: &str = "\
- who_is_off: read which colleagues are away from work over a stated range of days — the same team absence view everybody in the workspace already sees. It books nothing, approves nothing, cancels nothing and tells nobody. args: {\"from\": string \"YYYY-MM-DD\" (the first day of the range, REQUIRED), \"to\": string \"YYYY-MM-DD\" (the last day, optional — the same single day when left out)}. It returns names and days and NOTHING ELSE. There is no reason, no kind of leave and no note in what it reads, so never state or guess WHY anybody is away — not illness, not holiday, not parental or unpaid leave, not anything. Never answer with who is IN: a colleague this tool does not name may be at a customer, on another country's public holiday, or not an employee at all. Use this when the user asks who is off, who is away, or whether somebody is around on a given day.\n\
- draft_letter_from_template: fill in one of THIS COMPANY'S OWN letter templates about a colleague — an employment confirmation, a letter for a landlord, a reference — and leave the result in the user's Drafts. It sends nothing and tells nobody: the user reads it, edits it and sends it themselves. args: {\"template\": string (the name of a letter this company has already written, REQUIRED), \"employee\": string (the colleague the letter is about, by name, REQUIRED), \"to\": string (an address to put the draft to, optional — left empty for the user to fill in)}. You can fill in ONLY a letter the company has written: when nothing matches, say which letters exist and stop there. NEVER write, extend, reword or invent a letter about a person — the company's own words are the whole point, and improvising one is the single worst thing this tool set could do. The letter says only what its own template asks for, out of the staff directory and the company's details; it can carry no pay, no bank account, no date of birth, no home address and no national id, so never offer to add one.\n";

/// The HR paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool line above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model saying
/// something about a colleague that no record anywhere states.
pub const HR_GUIDANCE: &str = "For an HR tool, the answer is names and days and nothing else: never state, guess or imply WHY a colleague is away, because sickness, holiday, parental leave and unpaid leave are indistinguishable to these tools by design and are among the most sensitive facts a workplace holds. Never turn an absence into anything else — no assessment of a person, no comparison between colleagues, no tally of how often somebody is off, no conclusion about anybody's reliability, availability or commitment. Never rank, score, screen, shortlist or evaluate a person in any way at all: alo does not do that and there is no tool for it, so answer that you cannot rather than doing it in prose. A colleague the tool did not name is not thereby at work: say who is away, never who is in. And no HR tool reads pay, a home address, a national id or a bank account — never state one, whatever the user says they already know.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_hr_tool_is_described_to_the_model() {
        for tool in HR_TOOLS {
            assert!(
                HR_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = HR_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, HR_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(HR_TOOL_DOC.ends_with('\n'));
        assert!(HR_TOOL_DOC.starts_with("- "));
        assert!(HR_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn the_reading_tool_says_it_changes_nothing_and_tells_nobody() {
        let line = HR_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- who_is_off:"))
            .expect("who_is_off is described");
        // That it only reads is declared in the registry and rendered into the
        // prompt from there (ADR 0047 §1) — asserted where it is decided.
        assert!(crate::is_read_tool("who_is_off"));
        assert!(line.contains("books nothing"), "{line}");
        assert!(line.contains("approves nothing"), "{line}");
        // The design note's deliberate absence, in the model's own words: an
        // automated "X is off today" announcement is a cut, not an oversight
        // (`docs/design/hr.md` § Out of scope for B6).
        assert!(line.contains("tells nobody"), "{line}");
    }

    #[test]
    fn the_letter_tool_can_only_fill_in_a_letter_the_company_wrote() {
        // The one mistake *this* tool can make that nothing downstream catches:
        // a model that, finding no template, writes the employment confirmation
        // itself. The executor answers such a proposal with a 422 — these words
        // are what stop the model getting that far and then arguing with it.
        let line = HR_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- draft_letter_from_template:"))
            .expect("draft_letter_from_template is described");
        assert!(line.contains("THIS COMPANY'S OWN"), "{line}");
        assert!(
            line.contains("ONLY a letter the company has written"),
            "{line}"
        );
        assert!(
            line.contains("NEVER write, extend, reword or invent a letter"),
            "{line}"
        );
        assert!(line.contains("say which letters exist and stop"), "{line}");
        // It drafts; it does not send, and it does not announce.
        assert!(line.contains("sends nothing and tells nobody"), "{line}");
        assert!(line.contains("Drafts"), "{line}");
    }

    #[test]
    fn no_letter_this_tool_fills_in_can_state_pay_or_anything_private() {
        let line = HR_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- draft_letter_from_template:"))
            .expect("draft_letter_from_template is described");
        for refused in [
            "no pay",
            "no bank account",
            "no date of birth",
            "no home address",
            "no national id",
        ] {
            assert!(line.contains(refused), "{refused} is not refused: {line}");
        }
        assert!(line.contains("never offer to add one"), "{line}");
    }

    #[test]
    fn nothing_here_offers_a_reason_a_kind_of_leave_or_a_note() {
        // The one mistake this tool set can make that nothing downstream
        // catches: a health fact composed by a model out of an absence. The
        // arguments carry no such field, and the words forbid the inference.
        for forbidden in [
            "\"reason\"",
            "\"kind\"",
            "\"policy\"",
            "\"note\"",
            "\"type\"",
        ] {
            assert!(!HR_TOOL_DOC.contains(forbidden), "{forbidden}");
        }
        assert!(HR_TOOL_DOC.contains("never state or guess WHY"));
        assert!(HR_TOOL_DOC.contains("not illness"));
        assert!(HR_GUIDANCE.contains("never state, guess or imply WHY"));
    }

    #[test]
    fn silence_is_never_reported_as_presence() {
        assert!(HR_TOOL_DOC.contains("Never answer with who is IN"));
        assert!(HR_GUIDANCE.contains("say who is away, never who is in"));
    }

    #[test]
    fn nothing_here_evaluates_ranks_or_scores_a_person() {
        // The AI Act posture of `docs/design/hr.md`, held in the words the model
        // reads: screening is not a scheduling cut, it is a refusal, and a model
        // asked for it must decline rather than improvise one in prose.
        assert!(HR_GUIDANCE.contains("Never rank, score, screen, shortlist or evaluate"));
        assert!(HR_GUIDANCE.contains("there is no tool for it"));
        assert!(HR_GUIDANCE.contains("no tally of how often somebody is off"));
        for forbidden in [
            "screen_cv",
            "rank_applicants",
            "score_candidate",
            "shortlist_applicants",
            "assess_performance",
            "attendance_score",
        ] {
            assert!(!HR_TOOL_DOC.contains(forbidden), "{forbidden}");
            assert!(
                crate::agent_tool::find_tool(HR_TOOLS, forbidden).is_none(),
                "{forbidden}"
            );
        }
    }

    #[test]
    fn nothing_hr_offers_approves_decides_or_reads_pay() {
        // The writes that decide something about a person, none of which an
        // agent may propose: a leave decision is a manager's act, and an
        // employee record is HR's to type with the person in front of them.
        for forbidden in [
            "approve_leave",
            "reject_leave",
            "create_employee",
            "archive_employee",
            "hire_applicant",
            "payroll_export",
        ] {
            assert!(!HR_TOOL_DOC.contains(forbidden), "{forbidden}");
            assert!(
                crate::agent_tool::find_tool(HR_TOOLS, forbidden).is_none(),
                "{forbidden}"
            );
        }
        assert!(HR_GUIDANCE.contains("no HR tool reads pay"));
    }
}
