//! Executing the **Sheets** tools of the Sheet agent (ADR 0034, queue item
//! A2.2) — the acting half of five of the verbs [`alo_ai::sheets_intents`]
//! describes to the model, reached through the module's dispatcher in
//! [`crate::sheets_intents`] (AB.3).
//!
//! The three reading tools run inside the turn ([`crate::agent_turn`]); the two
//! that change a workbook run only from [`crate::agent::agent_execute`], after
//! the person who asked approved the proposal. Everything here goes through the
//! caller's own tenant-scoped store handle, so the Sheet agent reaches exactly
//! the spreadsheets the person who asked could already open — a workbook of
//! another tenant's, or of a colleague's private Drive, is not merely refused
//! here, it is not among the things that can be named
//! ([`alo_store::AccountStore::drive_sheets`] and `drive_find` are personal and
//! tenant-scoped, and the resolver picks out of what they return).
//!
//! Five rules shape this module, and none of them is thin glue:
//!
//! - **Every figure comes back with its address.** A spreadsheet is the one
//!   place where the source of a number is a cell reference, and the item's
//!   requirement is an answer *with the cells cited*. Nothing is returned from
//!   here without one.
//! - **A write edits the stored document, never a rebuild of it.**
//!   [`alo_ai::sheet_grid::set_formula`] and `set_value` change one cell of the
//!   snapshot as it was stored, so styles, merges, filters, notes and every
//!   plugin's data survive a tool the user approved for one column.
//! - **A formula is written; a fact never is.** [`execute_sheet_write_formula`]
//!   refuses anything not beginning with `=`, so there is no path here that
//!   types a figure into somebody's data. And a cell that already holds a value
//!   is refused by name unless the user said to replace it — an agent silently
//!   overwriting a number is the one mistake in a spreadsheet nobody notices
//!   until a report is wrong.
//! - **Tidying is about typing, never meaning.** [`execute_sheet_clean_column`]
//!   applies [`alo_ai::sheet_grid::tidy`] and nothing else, skips every formula
//!   cell, and writes no new version at all when there was nothing to tidy.
//! - **Results carry facts and reason codes, never sentences.** `notAFormula`,
//!   `storedAsNumber`, `nothingToTidy` — a user-facing sentence composed in the
//!   server would be English authored in one language, which is a bug in a
//!   European product (CLAUDE.md). The words the user reads are the model's
//!   own, in their language.

use axum::Json;
use axum::http::StatusCode;
use bytes::Bytes;
use serde_json::{Value, json};

use alo_ai::sheet_grid::{
    self, GridCell, SheetError, Tab, Workbook, cell_ref, column_label, parse_a1, parse_column,
};
use alo_store::{BlobId, DriveNode, DriveNodeId};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::Account;

/// The largest workbook this reads. A snapshot is JSON, so a serious book is a
/// few hundred kilobytes; well past this and the parse costs more than the turn
/// is worth, and the honest answer is to say so rather than to spend a minute
/// on it.
const MAX_SNAPSHOT_BYTES: i64 = 8 * 1024 * 1024;

/// How many rows one `sheet_read` hands back at most, and its default.
const MAX_READ_ROWS: usize = 40;
const DEFAULT_READ_ROWS: usize = 20;

/// How many cells of one row are shown. A row wider than this is reported
/// truncated rather than silently cut.
const MAX_ROW_CELLS: usize = 24;

/// How many rows one `sheet_answer` returns, and how many tabs it searches.
const MAX_ANSWER_ROWS: usize = 8;
const MAX_ANSWER_TABS: usize = 12;

/// How many cells of one referenced range `sheet_formula_explain` shows, and
/// how many references of one formula.
const MAX_REF_CELLS: usize = 12;
const MAX_REFS: usize = 8;

/// How many cells one approved formula write may touch.
const MAX_WRITE_CELLS: usize = 50;

/// The longest formula accepted. Long enough for anything anybody writes by
/// hand, short enough that fifty of them are still a small document.
const MAX_FORMULA_CHARS: usize = 500;

/// How many cells one column tidy may change.
const MAX_CLEAN_CELLS: usize = 500;

// ---- the reading tools -------------------------------------------------------

/// `sheet_read` — a block of a workbook, cell by cell, with addresses.
///
/// The window is bounded and says when it was cut, because the alternative — a
/// tab handed over entire — is both a turn that does not fit and an answer the
/// model would believe was complete.
///
/// # Errors
/// `422` when no spreadsheet of the caller's matches, when the tab or the
/// starting cell cannot be read, or when the stored document is not a workbook;
/// the store's own failure otherwise.
pub async fn execute_sheet_read(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let (node, _, book) = load(account, args).await?;
    let tab = resolve_tab(&book, args)?;
    let start = match string_arg(args, "from") {
        Some(from) => {
            parse_a1(&from).ok_or_else(|| unprocessable(format!("{from} is not a cell address")))?
        }
        None => (0, 0),
    };
    let wanted = args
        .get("rows")
        .and_then(Value::as_u64)
        .and_then(|rows| usize::try_from(rows).ok())
        .unwrap_or(DEFAULT_READ_ROWS)
        .clamp(1, MAX_READ_ROWS);

    let mut rows = Vec::new();
    let mut truncated = false;
    for (row, cells) in tab.rows() {
        if row < start.0 {
            continue;
        }
        let shown: Vec<&GridCell> = cells
            .into_iter()
            .filter(|cell| cell.col >= start.1)
            .collect();
        if shown.is_empty() {
            continue;
        }
        if rows.len() >= wanted {
            truncated = true;
            break;
        }
        let cut = shown.len() > MAX_ROW_CELLS;
        truncated |= cut;
        rows.push(json!({
            "row": u64::from(row) + 1,
            "cells": shown.iter().take(MAX_ROW_CELLS).map(|cell| cell_json(cell)).collect::<Vec<_>>(),
            "truncated": cut,
        }));
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sheetRead",
            "workbook": workbook_ref(&node, &book),
            "tab": tab_ref(tab),
            "from": cell_ref(start.0, start.1),
            "rows": rows,
            // Said plainly: what came back is a window, and there is more.
            "truncated": truncated,
        }
    })))
}

/// `sheet_answer` — the rows of a workbook that mention what was asked about.
///
/// Every returned row carries the whole row, each cell addressed and captioned
/// with the label above its column, so the model answers from the record rather
/// than from the one cell that matched a word.
///
/// # Errors
/// `422` when no spreadsheet matches, when nothing was asked, or when the
/// stored document is not a workbook; the store's own failure otherwise.
pub async fn execute_sheet_answer(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let question = string_arg(args, "question")
        .ok_or_else(|| unprocessable("what to look for in the sheet is required"))?;
    let (node, _, book) = load(account, args).await?;
    // A named tab narrows the search; naming none searches the workbook, which
    // is what somebody asking a question about "the sheet" means.
    let searched: Vec<&Tab> = match string_arg(args, "tab") {
        Some(_) => vec![resolve_tab(&book, args)?],
        None => book.tabs.iter().take(MAX_ANSWER_TABS).collect(),
    };
    let terms = sheet_grid::search_terms(&question);

    let mut rows = Vec::new();
    let mut scanned = 0_u64;
    for tab in &searched {
        scanned += tab.rows().len() as u64;
        for found in sheet_grid::find_rows(tab, &terms, MAX_ANSWER_ROWS) {
            if rows.len() >= MAX_ANSWER_ROWS {
                break;
            }
            rows.push(row_json(tab, found.row, &found.matched));
        }
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sheetAnswer",
            "workbook": workbook_ref(&node, &book),
            "searchedTabs": searched.iter().map(|tab| tab.name.as_str()).collect::<Vec<_>>(),
            "searchedRows": scanned,
            "terms": terms,
            "matched": rows.len(),
            "rows": rows,
        }
    })))
}

/// `sheet_formula_explain` — the formula in one cell, and what it reads.
///
/// The referenced cells are resolved and returned with their current contents:
/// a formula explained without them is a restatement of what the user can
/// already see in the formula bar.
///
/// # Errors
/// `422` when no spreadsheet matches, when the cell address cannot be read, or
/// when the stored document is not a workbook; the store's own failure
/// otherwise.
pub async fn execute_sheet_formula_explain(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let wanted = string_arg(args, "cell")
        .ok_or_else(|| unprocessable("which cell was meant is required"))?;
    let (row, col) = parse_a1(&wanted)
        .ok_or_else(|| unprocessable(format!("{wanted} is not a cell address")))?;
    let (node, _, book) = load(account, args).await?;
    let tab = resolve_tab(&book, args)?;
    let cell = tab.cell(row, col);
    let formula = cell.and_then(|cell| cell.formula.clone());

    let references = formula
        .as_deref()
        .map(|formula| {
            sheet_grid::formula_refs(formula)
                .into_iter()
                .take(MAX_REFS)
                .map(|reference| {
                    let shown = reference.positions(MAX_REF_CELLS);
                    json!({
                        "ref": reference.text,
                        "cells": shown.iter().map(|&(row, col)| match tab.cell(row, col) {
                            Some(cell) => cell_json(cell),
                            // An empty cell inside a range is a fact about the
                            // formula worth reporting: it is why a total is low.
                            None => json!({"cell": cell_ref(row, col), "type": "empty"}),
                        }).collect::<Vec<_>>(),
                        "cellsInRange": reference.size(),
                        "truncated": reference.size() > shown.len() as u64,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sheetFormula",
            "workbook": workbook_ref(&node, &book),
            "tab": tab_ref(tab),
            "cell": cell_ref(row, col),
            // The three states are different answers and are never merged: a
            // formula, a value that is not one, and an empty cell.
            "hasFormula": formula.is_some(),
            "reason": match (formula.is_some(), cell.is_some()) {
                (true, _) => Value::Null,
                (false, true) => json!("notAFormula"),
                (false, false) => json!("emptyCell"),
            },
            "formula": formula,
            "value": cell.map(|cell| cell.text.clone()),
            "functions": formula.as_deref().map(sheet_grid::formula_functions).unwrap_or_default(),
            "references": references,
        }
    })))
}

// ---- the writing tools ---------------------------------------------------------

/// One cell a write was asked to fill.
struct Write {
    row: u32,
    col: u32,
    formula: String,
}

/// `sheet_write_formula` — formulas into named cells of one tab.
///
/// Two refusals do the work here. A formula that is not one (no leading `=`) is
/// refused, so this cannot become a way to type a figure into somebody's data;
/// and a cell that already holds a **value** is refused by name unless the user
/// asked for it to be replaced. A cell holding a formula is replaced without
/// ceremony: that is what "change the formula" means, and the version history
/// holds the old one.
///
/// # Errors
/// `422` for a missing or malformed cell list, an address that will not parse,
/// a duplicate address, something that is not a formula, or an occupied cell
/// with no `replace`; the store's own failure otherwise.
pub async fn execute_sheet_write_formula(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let (node, mut snapshot, book) = load(account, args).await?;
    let tab = resolve_tab(&book, args)?;
    let writes = write_list(args)?;
    let replace = args
        .get("replace")
        .and_then(Value::as_bool)
        .unwrap_or_default();

    // Everything that already holds a value of its own, checked BEFORE a single
    // cell is touched: a half-applied write over somebody's figures is worse
    // than none, and the refusal has to be able to name them all.
    let occupied: Vec<String> = writes
        .iter()
        .filter(|write| {
            tab.cell(write.row, write.col)
                .is_some_and(|cell| cell.formula.is_none())
        })
        .map(|write| cell_ref(write.row, write.col))
        .collect();
    if !occupied.is_empty() && !replace {
        return Err(unprocessable(format!(
            "these cells already hold something: {} — say to replace them",
            occupied.join(", ")
        )));
    }

    let key = tab.key.clone();
    let written: Vec<Value> = writes
        .iter()
        .map(|write| {
            let had = tab.cell(write.row, write.col);
            let reference = cell_ref(write.row, write.col);
            sheet_grid::set_formula(&mut snapshot, &key, write.row, write.col, &write.formula);
            json!({
                "cell": reference,
                "formula": write.formula,
                "replaced": had.map(|cell| cell.text.clone()),
                "replacedFormula": had.and_then(|cell| cell.formula.clone()),
            })
        })
        .collect();

    let version = save(account, &node, &snapshot).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sheetWriteFormula",
            "workbook": workbook_ref(&node, &book),
            "tab": tab_ref(tab),
            "written": written,
            "versionNo": version,
            // A written formula has no answer yet: the cached value is gone and
            // the spreadsheet works it out when the sheet is next opened. Said
            // here so nobody reports a number that was never computed.
            "recalculates": "onOpen",
        }
    })))
}

/// `sheet_clean_column` — the typing of one column, tidied.
///
/// Nothing is written when nothing needed tidying: an approved tool that
/// changes nothing must not leave a version in somebody's history saying it
/// did.
///
/// # Errors
/// `422` for a missing or unreadable column, or when the stored document is not
/// a workbook; the store's own failure otherwise.
pub async fn execute_sheet_clean_column(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let wanted = string_arg(args, "column")
        .ok_or_else(|| unprocessable("which column was meant is required"))?;
    // A column letter, or a cell — somebody who says "tidy C2 down" has named
    // both the column and where to start.
    let (col, from_cell) = match parse_column(&wanted) {
        Some(col) => (col, None),
        None => match parse_a1(&wanted) {
            Some((row, col)) => (col, Some(row)),
            None => return Err(unprocessable(format!("{wanted} is not a column"))),
        },
    };
    let numbers = args.get("numbers").and_then(Value::as_bool).unwrap_or(true);

    let (node, mut snapshot, book) = load(account, args).await?;
    let tab = resolve_tab(&book, args)?;
    // Stated, else where the cell said, else under the header, else the top.
    let from = match args.get("from_row").and_then(Value::as_u64) {
        Some(row) => u32::try_from(row.max(1) - 1).unwrap_or(0),
        None => from_cell.unwrap_or_else(|| tab.header_row().map_or(0, |row| row + 1)),
    };

    let mut changes = Vec::new();
    let mut skipped_formulas = 0_u64;
    let mut scanned = 0_u64;
    let mut truncated = false;
    for cell in tab.cells.values().filter(|cell| cell.col == col) {
        if cell.row < from {
            continue;
        }
        scanned += 1;
        // A formula cell is never tidied: its text is an answer the sheet
        // computed, and writing that answer back as a value would replace the
        // calculation with a frozen copy of it.
        if cell.formula.is_some() {
            skipped_formulas += 1;
            continue;
        }
        // A cell the sheet already holds as a number has no typing to tidy —
        // a number has no blanks around it and is already a number. Rewriting
        // it would be a version in somebody's history saying a column changed
        // when nothing about it did.
        if cell.numeric {
            continue;
        }
        if changes.len() >= MAX_CLEAN_CELLS {
            truncated = true;
            break;
        }
        let tidied = sheet_grid::tidy(&cell.text, numbers);
        if !tidied.changes() {
            continue;
        }
        changes.push((cell.row, cell.text.clone(), tidied));
    }

    let mut applied = Vec::new();
    for (row, was, tidied) in &changes {
        let value = tidied
            .number
            .clone()
            .unwrap_or_else(|| Value::String(tidied.text.clone()));
        sheet_grid::set_value(&mut snapshot, &tab.key, *row, col, &value);
        applied.push(json!({
            "cell": cell_ref(*row, col),
            "was": was,
            "now": value,
            "did": tidied.reasons(),
        }));
    }
    // The version is only written when something actually changed.
    let version = if applied.is_empty() {
        None
    } else {
        Some(save(account, &node, &snapshot).await?)
    };

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sheetCleanColumn",
            "workbook": workbook_ref(&node, &book),
            "tab": tab_ref(tab),
            "column": column_label(col),
            "header": tab.header_label(col),
            "from": cell_ref(from, col),
            "scanned": scanned,
            "changed": applied.len(),
            "skippedFormulas": skipped_formulas,
            "cells": applied,
            "truncated": truncated,
            "versionNo": version,
            "reason": if applied.is_empty() { json!("nothingToTidy") } else { Value::Null },
        }
    })))
}

// ---- resolving, loading, saving --------------------------------------------------

/// The spreadsheet an argument names, its stored snapshot, and that snapshot
/// read.
///
/// Names, never ids: the candidates come from the caller's own Drive, so a
/// workbook belonging to another tenant — or to a colleague — is not among the
/// things that can be named here.
async fn load(account: &Account, args: &Value) -> Result<(DriveNode, Value, Workbook), Problem> {
    let node = resolve_workbook(account, args).await?;
    let snapshot = read_snapshot(account, &node).await?;
    let book = Workbook::read(&snapshot).map_err(sheet_problem)?;
    Ok((node, snapshot, book))
}

/// The workbook node an argument names, or the caller's only one.
async fn resolve_workbook(account: &Account, args: &Value) -> Result<DriveNode, Problem> {
    let Some(wanted) = string_arg(args, "workbook") else {
        let mut sheets = account.acc.drive_sheets(50).await.map_err(map_store_err)?;
        return match sheets.len() {
            0 => Err(unprocessable("there is no spreadsheet in your drive yet")),
            1 => Ok(sheets.remove(0)),
            _ => Err(unprocessable(format!(
                "more than one spreadsheet: {} — say which",
                names(&sheets)
            ))),
        };
    };
    // Searched by name first, so a drive with more spreadsheets than one
    // listing holds is still reachable; the listing is what a refusal names.
    let found: Vec<DriveNode> = account
        .acc
        .drive_find(&wanted, 20)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|node| node.kind == "sheet")
        .collect();
    if found.is_empty() {
        let sheets = account.acc.drive_sheets(50).await.map_err(map_store_err)?;
        return Err(unprocessable(if sheets.is_empty() {
            "there is no spreadsheet in your drive yet".to_owned()
        } else {
            format!(
                "no spreadsheet of yours is called {wanted} — you have: {}",
                names(&sheets)
            )
        }));
    }
    pick(
        &wanted,
        found
            .iter()
            .map(|node| (node.name.as_str(), node.clone()))
            .collect(),
        "spreadsheet",
    )
}

/// The names of some workbooks, for a refusal that lists them.
fn names(nodes: &[DriveNode]) -> String {
    nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The tab an argument names, or the workbook's first.
fn resolve_tab<'a>(book: &'a Workbook, args: &Value) -> Result<&'a Tab, Problem> {
    let wanted = string_arg(args, "tab");
    book.tab(wanted.as_deref()).ok_or_else(|| {
        unprocessable(format!(
            "no tab of this spreadsheet is called {} — it has: {}",
            wanted.unwrap_or_default(),
            book.tabs
                .iter()
                .map(|tab| tab.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// The stored snapshot of a workbook node.
async fn read_snapshot(account: &Account, node: &DriveNode) -> Result<Value, Problem> {
    let Some(blob) = node.blob_id.clone() else {
        // A sheet node with no blob has never been saved: it is a new, empty
        // workbook, and that is an answer rather than a failure.
        return Ok(json!({"sheets": {"sheet-1": {"id": "sheet-1", "name": "Sheet1"}}}));
    };
    if node.size > MAX_SNAPSHOT_BYTES {
        return Err(unprocessable(
            "that spreadsheet is too large for the agent to read",
        ));
    }
    let bytes = account
        .acc
        .blob_bytes_for_send(&BlobId::new(blob))
        .await
        .map_err(map_store_err)?;
    if bytes.is_empty() {
        return Ok(json!({"sheets": {"sheet-1": {"id": "sheet-1", "name": "Sheet1"}}}));
    }
    serde_json::from_slice(&bytes).map_err(|_| sheet_problem(SheetError::NotJson))
}

/// Stores an edited snapshot as a new version of the node, exactly as the
/// editor's own save does (a blob, then a version) — so the agent's change is
/// in the same history as everybody else's and can be rolled back the same way.
async fn save(account: &Account, node: &DriveNode, snapshot: &Value) -> Result<i32, Problem> {
    let bytes = serde_json::to_vec(snapshot).map_err(|_| Problem::server_error())?;
    let size = i64::try_from(bytes.len()).map_err(|_| Problem::server_error())?;
    let blob = account
        .acc
        .put_blob(Bytes::from(bytes), Some("application/json"))
        .await
        .map_err(map_store_err)?;
    account
        .acc
        .drive_add_version(
            &DriveNodeId::new(node.id.as_str().to_owned()),
            blob.as_str(),
            size,
        )
        .await
        .map_err(map_store_err)
}

/// The `422` a snapshot that cannot be read as a workbook earns, carrying the
/// reason code rather than a sentence.
fn sheet_problem(error: SheetError) -> Problem {
    Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("that spreadsheet cannot be read: {}", error.as_str()),
    )
}

/// The `cells` argument of a formula write, validated whole.
///
/// Validated before anything is applied and reported per entry, because a
/// write that fails on its fortieth cell having applied thirty-nine is a
/// spreadsheet nobody can reason about.
fn write_list(args: &Value) -> Result<Vec<Write>, Problem> {
    let entries = args
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| unprocessable("say which cells to write, and what to put in them"))?;
    if entries.is_empty() {
        return Err(unprocessable(
            "say which cells to write, and what to put in them",
        ));
    }
    if entries.len() > MAX_WRITE_CELLS {
        return Err(unprocessable(format!(
            "at most {MAX_WRITE_CELLS} cells at a time, and {} were asked for",
            entries.len()
        )));
    }
    let mut writes: Vec<Write> = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let at = index + 1;
        let cell = string_arg(entry, "cell")
            .ok_or_else(|| unprocessable(format!("entry {at} does not say which cell")))?;
        let (row, col) = parse_a1(&cell)
            .ok_or_else(|| unprocessable(format!("{cell} is not a cell address")))?;
        let formula = string_arg(entry, "formula")
            .ok_or_else(|| unprocessable(format!("entry {at} has no formula")))?;
        // The rule the whole tool rests on: this writes calculations, never
        // data. Anything else is refused rather than quietly prefixed with '='.
        if !formula.starts_with('=') {
            return Err(unprocessable(format!(
                "{cell} was given {formula}, which is not a formula — a formula starts with ="
            )));
        }
        if formula.chars().count() > MAX_FORMULA_CHARS {
            return Err(unprocessable(format!(
                "the formula for {cell} is longer than {MAX_FORMULA_CHARS} characters"
            )));
        }
        if writes.iter().any(|w| w.row == row && w.col == col) {
            return Err(unprocessable(format!(
                "{cell} is named twice, so which formula it should hold is not stated"
            )));
        }
        writes.push(Write { row, col, formula });
    }
    Ok(writes)
}

// ---- the shapes a result carries -------------------------------------------------

/// Which workbook a result is about.
fn workbook_ref(node: &DriveNode, book: &Workbook) -> Value {
    json!({
        "id": node.id.as_str(),
        // The Drive name is what the user sees and searches by; the workbook's
        // own name is what an imported file called itself, and the two differ
        // often enough to report both.
        "name": node.name,
        "workbookName": book.name,
        "tabs": book.tabs.iter().map(|tab| tab.name.as_str()).collect::<Vec<_>>(),
    })
}

/// Which tab, and how much is in it.
fn tab_ref(tab: &Tab) -> Value {
    json!({
        "name": tab.name,
        "usedRange": tab.used_range(),
        "filledCells": tab.cells.len(),
        "headerRow": tab.header_row().map(|row| u64::from(row) + 1),
    })
}

/// One cell: its address, what it holds, and what kind of thing that is.
fn cell_json(cell: &GridCell) -> Value {
    json!({
        "cell": cell.reference(),
        "text": cell.text,
        "type": if cell.formula.is_some() {
            "formula"
        } else if cell.numeric {
            "number"
        } else {
            "text"
        },
        "formula": cell.formula,
    })
}

/// One row of a search result: every cell of it, captioned with the label above
/// its column, and the ones that matched marked.
fn row_json(tab: &Tab, row: u32, matched: &[(u32, u32)]) -> Value {
    let cells: Vec<Value> = tab
        .cells
        .values()
        .filter(|cell| cell.row == row)
        .take(MAX_ROW_CELLS)
        .map(|cell| {
            let mut value = cell_json(cell);
            if let Some(object) = value.as_object_mut() {
                object.insert("header".to_owned(), json!(tab.header_label(cell.col)));
                object.insert(
                    "matched".to_owned(),
                    json!(matched.contains(&(cell.row, cell.col))),
                );
            }
            value
        })
        .collect();
    json!({
        "tab": tab.name,
        "row": u64::from(row) + 1,
        "cells": cells,
    })
}
