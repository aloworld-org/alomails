//! alo Sheets' verbs (ADR 0058, queue item AB.3) — the whole of what the
//! Sheets agent may do, and the words a model reads about it.
//!
//! Nothing here reads or writes a workbook: the executors live in `alo-jmap`
//! (`sheets_intents.rs`, with the five older tool executors it keeps in
//! `agent_sheets.rs`), through the asker's tenant-scoped store — a spreadsheet
//! the asker could not open is not among the things that can be named.
//!
//! **The spreadsheets are the ones in Drive.** A workbook is a Drive node of
//! kind `sheet` whose blob is the editor's own snapshot; that is what every
//! verb below reads and writes, which is why no verb adapts a route — there is
//! no `/sheets/` route surface at all, and the editor itself saves through
//! Drive's own routes.
//!
//! The rules the hand-written tool set learned, kept because each one is a
//! mistake it exists to prevent:
//!
//! - **Every figure is cited to the cell it came from.** A spreadsheet is the
//!   one place in the workspace where the source of a number is an address,
//!   and an answer without one cannot be checked against the grid. Every read
//!   hands back A1 references, and the guidance says to repeat them.
//! - **The agent never does the arithmetic.** It reads what the cells hold and
//!   it writes formulas for the spreadsheet to evaluate; it does not add a
//!   column up in its head and report the answer. A number a model computed
//!   and a number the grid computes are two answers, and only one of them
//!   updates when the data changes.
//! - **A write puts a formula in a cell — never a fact.** `sheet_write_formula`
//!   refuses anything that is not a formula, so there is no path here that
//!   types somebody's revenue, headcount or price into their own spreadsheet.
//!   Data is the user's; a calculation over it is what an agent is for.
//! - **Tidying is about typing, never about meaning.** `sheet_clean_column`
//!   trims the ends, collapses runs of blanks and stores text that is a number
//!   as a number. It does not fix case, spelling, dates or currencies — each
//!   of those is a guess about what a record *means*, and a wrong one silently
//!   rewrites it.
//! - **Both writes wait for a tap.** They change a document the user's
//!   colleagues are reading, so they are declared writes (ADR 0047 §1) and the
//!   only path that runs them is an approval the asker themselves gave.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const WORKBOOK_OPT: Arg = Arg::optional(
    "workbook",
    "text",
    "which spreadsheet, by its name in Drive; the user's only one when left out",
);
const TAB_OPT: Arg = Arg::optional("tab", "text", "which tab; the first when left out");

/// The verbs.
pub const SHEETS_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "list_spreadsheets",
        purpose: "The spreadsheets of the user's own Drive, most recently edited first — each with its name and when it was last touched — or the spreadsheets inside one folder, by the folder's name. It changes nothing. What \"which spreadsheets exist\" means to a person looking at their Drive.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "folder",
                "text",
                "a folder of the user's own Drive, by name; every spreadsheet of theirs when left out",
            ),
            Arg::optional("limit", "integer", "at most 20"),
        ],
        answers: &[
            "which spreadsheets exist",
            "which spreadsheets do we have",
            "what workbooks are in the Finance folder",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "sheet_read",
        purpose: "Read a block of a spreadsheet as it stands — which tabs it has, which cells are in use, and the contents of each cell with its own address (A1, B7). It changes nothing. This is where the addresses every other verb needs come from: read before you explain a formula, write one, or tidy anything, and never work an address out for yourself.",
        effect: Effect::Read,
        args: &[
            WORKBOOK_OPT,
            TAB_OPT,
            Arg::optional("from", "text", "the top-left cell to start at, like \"A1\""),
            Arg::optional("rows", "integer", "how many rows, at most 40"),
        ],
        answers: &[
            "what is in the budget sheet",
            "read me the forecast",
            "which tabs does the workbook have",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "sheet_answer",
        purpose: "The rows of a spreadsheet that mention what the user asked about, returned with the label at the top of each column and every cell's address. It searches; it changes nothing. Answer from the cells it returns and give the address of each figure you quote; when it comes back with nothing, say you could not find it in the sheet rather than answering from anything else.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "question",
                "text",
                "what to look for, in the user's own words",
            ),
            WORKBOOK_OPT,
            Arg::optional("tab", "text", "which tab; every tab when left out"),
        ],
        answers: &[
            "what did we quote Delaunay",
            "which row has the March figures",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "sheet_formula_explain",
        purpose: "Read the formula in ONE cell, what it refers to, and what those cells hold right now. It changes nothing. Explain it in the user's own words from what comes back, name the ranges it reads, and say plainly when the cell holds a value rather than a formula.",
        effect: Effect::Read,
        args: &[
            Arg::required("cell", "text", "the cell, like \"D12\""),
            WORKBOOK_OPT,
            TAB_OPT,
        ],
        answers: &[
            "what does the formula in D12 do",
            "why is the total in F30 what it is",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "sheet_write_formula",
        purpose: "Put a FORMULA into one or more cells. Every entry must be a formula beginning with \"=\" — this cannot type a value, a figure or a piece of text into somebody's data. Read the sheet first: write the addresses sheet_read gave you and no others. A cell that already holds a value is refused unless replace is set, and the refusal names the cells.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "cells",
                "array",
                "the writes, each {\"cell\": the address, like \"D2\", \"formula\": the whole formula, beginning with \"=\"}; at most 50",
            ),
            WORKBOOK_OPT,
            TAB_OPT,
            Arg::optional(
                "replace",
                "boolean",
                "true only when the user has been told which cells already hold something and wants them overwritten",
            ),
        ],
        answers: &[
            "add up the amounts column",
            "put an average under the scores",
        ],
        preview: Some(
            "The proposed formulas will be written into the named cells — calculations for the sheet to evaluate, never a typed figure.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "sheet_clean_column",
        purpose: "Tidy how ONE column was typed — blanks at the ends removed, a run of blanks inside collapsed to one, and text that is a number stored as a number so the sheet can add it up. It changes nothing else: not the case, not the spelling, not a date, not a currency, and never a cell holding a formula. Say what it will change in your own sentence before proposing it.",
        effect: Effect::Write,
        args: &[
            Arg::required("column", "text", "the column, like \"C\""),
            WORKBOOK_OPT,
            TAB_OPT,
            Arg::optional(
                "from_row",
                "integer",
                "the first row to touch; the row under the header when left out",
            ),
            Arg::optional(
                "numbers",
                "boolean",
                "false to leave text that looks like a number alone, for a column of codes or reference numbers",
            ),
        ],
        answers: &["tidy the amounts column", "why does the sum skip some rows"],
        preview: Some(
            "Column {column}'s typing will be tidied — blanks trimmed, numbers stored as numbers; the words, dates and formulas in it stay as they are.",
        ),
        undo: None,
        routes: &[],
    },
];

/// No route is deliberately kept from the agent, because there is nothing to
/// keep: alo Sheets has no route surface of its own. A workbook is a Drive
/// node of kind `sheet`, and the editor loads and saves it through Drive's
/// own routes — which are the Drive module's to account for, verb by verb.
/// The coverage test in `alo-jmap` holds this empty list honest by asserting
/// the router registers no `/sheets` route at all.
pub const SHEETS_EXCLUDED: &[Excluded] = &[];

/// The Sheets paragraph of the agent's general instructions.
///
/// Kept apart from the per-verb purposes above because it says what the
/// *product* is, not what a verb takes: it is the sentence that stops a model
/// doing the arithmetic itself, and the one that stops it answering about a
/// figure it cannot point at.
pub const SHEETS_GUIDANCE: &str = "For a spreadsheet, ALWAYS say which cell a figure came from — \"1 200 in B2\", not \"1 200\" — because an address is the only way the person reading you can check it against their own grid. NEVER work out a total, an average, a difference or a percentage yourself: read what the cells hold, and when the user wants a figure the sheet does not have yet, propose a formula for the sheet to evaluate. A number you calculated is a second answer that stops being true the moment the data changes. Never type a fact into somebody's spreadsheet: their figures, names, dates and prices are theirs, and the only thing you write is a formula over them. When a tidy or a formula is proposed, say in your own words which cells it touches and what it will do to them, and never say a sheet has been changed until the user has approved it.\n";

/// The module, as the registry reads it.
pub static SHEETS: IntentModule = IntentModule {
    intents: SHEETS_INTENTS,
    excluded: SHEETS_EXCLUDED,
    guidance: SHEETS_GUIDANCE,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// No verb adapts a route, and that is the design rather than a gap: the
    /// agent's spreadsheets are Drive nodes reached through the store, and
    /// there is no `/sheets` route surface for a verb to be the verb behind.
    #[test]
    fn every_verb_has_a_purpose_and_a_question_and_no_route() {
        for intent in SHEETS_INTENTS {
            assert!(
                intent.routes.is_empty(),
                "{} claims a route Sheets does not have",
                intent.name
            );
            assert!(
                intent.purpose.ends_with('.'),
                "{} purpose is not a sentence",
                intent.name
            );
            assert!(
                !intent.answers.is_empty(),
                "{} answers nothing",
                intent.name
            );
            if intent.effect == Effect::Write {
                assert!(
                    intent.preview.is_some(),
                    "{} is a write without a preview",
                    intent.name
                );
            }
        }
        assert!(
            SHEETS_EXCLUDED.is_empty(),
            "an exclusion here would name a route Sheets does not register"
        );
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = SHEETS_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SHEETS_INTENTS.len());
        let doc = SHEETS.doc();
        for intent in SHEETS_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(SHEETS_GUIDANCE.ends_with('\n'));
    }

    /// The four reads answer inside the turn; the two writes wait. Declared,
    /// not derived from the names — and the reads say plainly that they change
    /// nothing, because a question about a spreadsheet answered with a button
    /// is the bug ADR 0047 was written about.
    #[test]
    fn the_reads_answer_and_only_the_two_that_change_the_document_wait() {
        let find = |name: &str| SHEETS.find(name).unwrap_or_else(|| panic!("{name}"));
        for reads in [
            "list_spreadsheets",
            "sheet_read",
            "sheet_answer",
            "sheet_formula_explain",
        ] {
            assert_eq!(find(reads).effect, Effect::Read, "{reads}");
            assert!(find(reads).purpose.contains("changes nothing"), "{reads}");
        }
        for changes in ["sheet_write_formula", "sheet_clean_column"] {
            assert_eq!(find(changes).effect, Effect::Write, "{changes}");
        }
        assert!(SHEETS_GUIDANCE.contains("never say a sheet has been changed"));
    }

    /// The rule the whole set is shaped by: a figure is cited to its cell,
    /// and the agent never computes one.
    #[test]
    fn every_figure_is_cited_and_none_is_calculated() {
        let find = |name: &str| SHEETS.find(name).unwrap_or_else(|| panic!("{name}"));
        assert!(SHEETS_GUIDANCE.contains("ALWAYS say which cell a figure came from"));
        assert!(SHEETS_GUIDANCE.contains("NEVER work out a total"));
        assert!(SHEETS_GUIDANCE.contains("propose a formula for the sheet to evaluate"));
        assert!(find("sheet_read").purpose.contains("with its own address"));
        assert!(
            find("sheet_answer")
                .purpose
                .contains("every cell's address")
        );
        assert!(
            find("sheet_answer")
                .purpose
                .contains("give the address of each figure you quote"),
            "the answer verb is the one that must insist on it"
        );
    }

    /// The writing pair, stated where the model reads it: addresses come from
    /// the read, never from the model. A guessed address is how a formula
    /// lands on somebody else's data.
    #[test]
    fn a_write_names_addresses_the_read_verb_gave_it() {
        let find = |name: &str| SHEETS.find(name).unwrap_or_else(|| panic!("{name}"));
        assert!(
            find("sheet_read")
                .purpose
                .contains("never work an address out for yourself")
        );
        let write = find("sheet_write_formula").purpose;
        assert!(write.contains("sheet_read gave you"), "{write}");
        assert!(write.contains("Read the sheet first"), "{write}");
        assert!(write.contains("refused unless replace is set"), "{write}");
    }

    /// The one thing a spreadsheet agent must never do: put a fact in a cell.
    /// A formula is a statement about the user's data; a typed figure is a new
    /// number in a record nobody checked.
    #[test]
    fn nothing_here_writes_a_fact_into_somebody_elses_data() {
        let write = SHEETS
            .find("sheet_write_formula")
            .unwrap_or_else(|| panic!("sheet_write_formula"))
            .purpose;
        assert!(
            write.contains("must be a formula beginning with \"=\""),
            "{write}"
        );
        assert!(
            write.contains("cannot type a value, a figure or a piece of text"),
            "{write}"
        );
        assert!(SHEETS_GUIDANCE.contains("Never type a fact into somebody's spreadsheet"));
        // No verb in the set offers a way to set a value.
        for intent in SHEETS_INTENTS {
            assert!(
                !intent.name.contains("set_value") && !intent.name.contains("write_value"),
                "{} would be a way to type a fact",
                intent.name
            );
        }
    }

    /// Tidying is about how a column was typed. Everything the verb refuses to
    /// touch is named where the model reads it, because each of them is a
    /// guess about meaning that would rewrite a record.
    #[test]
    fn tidying_names_what_it_will_not_touch() {
        let clean = SHEETS
            .find("sheet_clean_column")
            .unwrap_or_else(|| panic!("sheet_clean_column"))
            .purpose;
        assert!(clean.contains("Tidy how ONE column was typed"), "{clean}");
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
        let numbers = SHEETS
            .find("sheet_clean_column")
            .unwrap_or_else(|| panic!("sheet_clean_column"))
            .args
            .iter()
            .find(|arg| arg.name == "numbers")
            .unwrap_or_else(|| panic!("numbers"));
        assert!(numbers.purpose.contains("codes or reference numbers"));
    }
}
