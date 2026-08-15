//! The **Sheets** tool set of the agent (ADR 0034, queue item A2.2) — the names
//! alo Sheets contributes to its own agent, and the words that tell a model what
//! they take.
//!
//! The same seam every product before it uses ([`crate::agent_sites`]): a tool
//! list carrying each tool's effect, a description block, and a paragraph of
//! guidance. Nothing here reads or writes a workbook — the reading tools are
//! executed inside the turn and the writes only from an approval, both by
//! `alo-jmap`'s `agent_sheets` over [`crate::sheet_grid`], against the caller's
//! own tenant-scoped store.
//!
//! Five rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **Every figure is cited to the cell it came from.** A spreadsheet is the
//!   one place in the workspace where the source of a number is an address, and
//!   an answer without one cannot be checked against the grid. Every read hands
//!   back A1 references, and the guidance says to repeat them.
//! - **The agent never does the arithmetic.** It reads what the cells hold and
//!   it writes formulas for the spreadsheet to evaluate; it does not add a
//!   column up in its head and report the answer. A number a model computed and
//!   a number the grid computes are two answers, and only one of them updates
//!   when the data changes.
//! - **A write puts a formula in a cell — never a fact.** `sheet_write_formula`
//!   refuses anything that is not a formula, so there is no path here that types
//!   somebody's revenue, headcount or price into their own spreadsheet. Data is
//!   the user's; a calculation over it is what an agent is for.
//! - **Tidying is about typing, never about meaning.** `sheet_clean_column`
//!   trims the ends, collapses runs of blanks and stores text that is a number
//!   as a number. It does not fix case, spelling, dates or currencies — each of
//!   those is a guess about what a record *means*, and a wrong one silently
//!   rewrites it.
//! - **Both writes wait for a tap.** They change a document the user's
//!   colleagues are reading, so they are declared writes (ADR 0047 §1) and the
//!   only path that runs them is an approval the asker themselves gave.

use crate::agent_tool::AgentTool;

/// The Sheets tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const SHEETS_TOOLS: &[AgentTool] = &[
    AgentTool::read("sheet_read"),
    AgentTool::read("sheet_answer"),
    AgentTool::read("sheet_formula_explain"),
    AgentTool::write("sheet_write_formula"),
    AgentTool::write("sheet_clean_column"),
];

/// The description of each Sheets tool, spliced into the agent's system prompt.
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Sheets has.
pub const SHEETS_TOOL_DOC: &str = "\
- sheet_read: read a block of a spreadsheet as it stands — which tabs it has, which cells are in use, and the contents of each cell with its own address (A1, B7). It changes nothing. args: {\"workbook\": string (which spreadsheet, by its name in Drive, optional — their only one when left out), \"tab\": string (which tab, optional — the first when left out), \"from\": string (the top-left cell to start at, like \"A1\", optional), \"rows\": number (how many rows, optional, at most 40)}. This is where the addresses every other tool needs come from: read before you explain a formula, write one, or tidy anything, and never work an address out for yourself.\n\
- sheet_answer: find the rows of a spreadsheet that mention what the user asked about, returned with the label at the top of each column and every cell's address. It searches; it changes nothing. args: {\"question\": string (what to look for, in the user's own words, REQUIRED), \"workbook\": string (optional), \"tab\": string (optional — every tab when left out)}. Answer from the cells it returns and give the address of each figure you quote. When it comes back with nothing, say you could not find it in the sheet rather than answering from anything else.\n\
- sheet_formula_explain: read the formula in ONE cell, what it refers to, and what those cells hold right now. It changes nothing. args: {\"cell\": string (the cell, like \"D12\", REQUIRED), \"workbook\": string (optional), \"tab\": string (optional)}. Explain it in the user's own words from what comes back, name the ranges it reads, and say plainly when the cell holds a value rather than a formula.\n\
- sheet_write_formula: put a FORMULA into one or more cells. Every entry must be a formula beginning with \"=\" — this tool cannot type a value, a figure or a piece of text into somebody's data. args: {\"cells\": [{\"cell\": string (like \"D2\"), \"formula\": string (the whole formula, beginning with \"=\")}] (REQUIRED, at most 50), \"workbook\": string (optional), \"tab\": string (optional), \"replace\": true (only when the user has been told which cells already hold something and wants them overwritten)}. Read the sheet first: write the addresses sheet_read gave you and no others. A cell that already holds a value is refused unless replace is set, and the refusal names the cells.\n\
- sheet_clean_column: tidy how ONE column was typed — blanks at the ends removed, a run of blanks inside collapsed to one, and text that is a number stored as a number so the sheet can add it up. It changes nothing else: not the case, not the spelling, not a date, not a currency, and never a cell holding a formula. args: {\"column\": string (the column, like \"C\", REQUIRED), \"workbook\": string (optional), \"tab\": string (optional), \"from_row\": number (the first row to touch, optional — the row under the header when left out), \"numbers\": false (leave text that looks like a number alone, for a column of codes or reference numbers)}. Say what it will change in your own sentence before proposing it.\n";

/// The Sheets paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model doing the
/// arithmetic itself, and the one that stops it answering about a figure it
/// cannot point at.
pub const SHEETS_GUIDANCE: &str = "For a spreadsheet, ALWAYS say which cell a figure came from — \"1 200 in B2\", not \"1 200\" — because an address is the only way the person reading you can check it against their own grid. NEVER work out a total, an average, a difference or a percentage yourself: read what the cells hold, and when the user wants a figure the sheet does not have yet, propose a formula for the sheet to evaluate. A number you calculated is a second answer that stops being true the moment the data changes. Never type a fact into somebody's spreadsheet: their figures, names, dates and prices are theirs, and the only thing you write is a formula over them. When a tidy or a formula is proposed, say in your own words which cells it touches and what it will do to them, and never say a sheet has been changed until the user has approved it.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_spreadsheet_tool_is_described_to_the_model() {
        for tool in SHEETS_TOOLS {
            assert!(
                SHEETS_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = SHEETS_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, SHEETS_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(SHEETS_TOOL_DOC.ends_with('\n'));
        assert!(SHEETS_TOOL_DOC.starts_with("- "));
        assert!(SHEETS_GUIDANCE.ends_with('\n'));
    }

    fn line(name: &str) -> String {
        SHEETS_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .expect("the tool is described")
            .to_owned()
    }

    /// The three reads answer inside the turn; the two writes wait. Declared,
    /// not derived from the names — and the reads say plainly that they change
    /// nothing, because a question about a spreadsheet answered with a button
    /// is the bug ADR 0047 was written about.
    #[test]
    fn the_reads_answer_and_only_the_two_that_change_the_document_wait() {
        for reads in ["sheet_read", "sheet_answer", "sheet_formula_explain"] {
            assert!(crate::is_read_tool(reads), "{reads}");
            assert!(line(reads).contains("changes nothing"), "{reads}");
        }
        for changes in ["sheet_write_formula", "sheet_clean_column"] {
            assert!(!crate::is_read_tool(changes), "{changes}");
        }
        assert!(SHEETS_GUIDANCE.contains("never say a sheet has been changed"));
    }

    /// The rule the whole tool set is shaped by: a figure is cited to its cell,
    /// and the agent never computes one.
    #[test]
    fn every_figure_is_cited_and_none_is_calculated() {
        assert!(SHEETS_GUIDANCE.contains("ALWAYS say which cell a figure came from"));
        assert!(SHEETS_GUIDANCE.contains("NEVER work out a total"));
        assert!(SHEETS_GUIDANCE.contains("propose a formula for the sheet to evaluate"));
        assert!(line("sheet_read").contains("with its own address"));
        assert!(line("sheet_answer").contains("every cell's address"));
        assert!(
            line("sheet_answer").contains("give the address of each figure you quote"),
            "the answer tool is the one that must insist on it"
        );
    }

    /// The writing pair, stated in the prompt: addresses come from the read,
    /// never from the model. A guessed address is how a formula lands on
    /// somebody else's data.
    #[test]
    fn a_write_names_addresses_the_read_tool_gave_it() {
        assert!(line("sheet_read").contains("never work an address out for yourself"));
        let write = line("sheet_write_formula");
        assert!(write.contains("sheet_read gave you"), "{write}");
        assert!(write.contains("Read the sheet first"), "{write}");
        assert!(write.contains("refused unless replace is set"), "{write}");
    }

    /// The one thing a spreadsheet agent must never do: put a fact in a cell.
    /// A formula is a statement about the user's data; a typed figure is a new
    /// number in a record nobody checked.
    #[test]
    fn nothing_here_writes_a_fact_into_somebody_elses_data() {
        let write = line("sheet_write_formula");
        assert!(
            write.contains("must be a formula beginning with \"=\""),
            "{write}"
        );
        assert!(
            write.contains("cannot type a value, a figure or a piece of text"),
            "{write}"
        );
        assert!(SHEETS_GUIDANCE.contains("Never type a fact into somebody's spreadsheet"));
        // No tool in the set offers a way to set a value.
        for tool in SHEETS_TOOLS {
            assert!(
                !tool.name.contains("set_value") && !tool.name.contains("write_value"),
                "{} would be a way to type a fact",
                tool.name
            );
        }
    }

    /// Tidying is about how a column was typed. Everything the tool refuses to
    /// touch is named where the model reads it, because each of them is a guess
    /// about meaning that would rewrite a record.
    #[test]
    fn tidying_names_what_it_will_not_touch() {
        let clean = line("sheet_clean_column");
        assert!(clean.contains("tidy how ONE column was typed"), "{clean}");
        for named in [
            "not the case",
            "not the spelling",
            "not a date",
            "not a currency",
        ] {
            assert!(clean.contains(named), "{named} is not named: {clean}");
        }
        assert!(clean.contains("never a cell holding a formula"), "{clean}");
        // The escape hatch for a column of codes, so the tidy is not a trap.
        assert!(clean.contains("codes or reference numbers"), "{clean}");
    }
}
