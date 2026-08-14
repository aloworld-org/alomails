//! The **Contacts** tool of the agent (ADR 0034) — what alo Contacts lends the
//! one agent.
//!
//! One tool, and it reads. "What is Ben's address?" is a daily question, and
//! the agent could previously only answer it from whatever documents happened
//! to quote an address — which is how a stale one gets repeated for years.
//!
//! Two rules shape it:
//!
//! - **A personal address book is personal.** This searches the caller's own
//!   contacts, not the company directory. Those are different things with
//!   different rules, and conflating them would quietly turn "who do I know"
//!   into "who works here".
//! - **Several matches are reported, never resolved.** Two people called Ben
//!   is the normal case, and a tool that picked one would put the wrong
//!   address in a message somebody then sends. The answer names both and asks.

use crate::agent_tool::AgentTool;

/// The Contacts tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// One tool, and it reads: an address book the agent could write to is one a
/// misheard name can quietly corrupt.
pub const CONTACTS_TOOLS: &[AgentTool] = &[AgentTool::read("find_contact")];

/// What the Contacts tool takes, in the words the model reads.
///
/// Read-versus-write is declared in [`CONTACTS_TOOLS`] and rendered into the
/// prompt from there (ADR 0047 §1), never restated here.
pub const CONTACTS_TOOL_DOC: &str = "\
- find_contact: look somebody up in the user's own address book. args: {\"query\": string (the name, address or company the user said, required), \"limit\": integer (optional, at most 10)}. Use this when the user asks for somebody's address, number or details, or asks who somebody is. If more than one person matches, say so and name them rather than choosing one — two people with the same first name is ordinary, and picking the wrong one puts the wrong address in whatever is written next.\n";

/// The rules that keep a Contacts proposal honest, appended to the system prompt.
pub const CONTACTS_GUIDANCE: &str = "For find_contact, pass the user's own words through as the query and never invent a surname, a company or an address to narrow it. This searches the user's PERSONAL address book, not a company directory: if nobody matches, say the address book has no such person rather than guessing at a colleague. Never state a contact detail that did not come from a tool result.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_tool_is_described() {
        for tool in CONTACTS_TOOLS {
            assert!(
                CONTACTS_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} is offered to the model with no description",
                tool.name
            );
        }
    }

    /// An address book the agent could write to is one that can be quietly
    /// corrupted by a misheard name.
    #[test]
    fn nothing_here_can_change_an_address_book() {
        for tool in CONTACTS_TOOLS {
            assert!(
                tool.is_read(),
                "{} would let the agent edit somebody's contacts",
                tool.name
            );
        }
    }

    /// The failure that makes a wrong answer expensive: an invented address is
    /// one somebody sends a contract to.
    #[test]
    fn the_model_is_forbidden_from_inventing_details() {
        assert!(CONTACTS_GUIDANCE.contains("Never state a contact detail"));
        assert!(CONTACTS_TOOL_DOC.contains("rather than choosing one"));
    }
}
