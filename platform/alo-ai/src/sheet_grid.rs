//! Reading and writing a spreadsheet's stored snapshot (queue item A2.2).
//!
//! An alo Sheet is a Drive node of kind `sheet` whose blob is the editor's own
//! JSON workbook snapshot (`web/src/drive/SheetEditor.tsx`). Everything the
//! Sheet agent does — answering from the data, explaining a formula, writing a
//! formula, tidying a column — is a read or a write of that document, and all
//! of it is here: pure, synchronous, no store handle, no model. The executors
//! in `alo-jmap`'s `agent_sheets` fetch the bytes and put them back.
//!
//! Four rules shape this module, and each is a mistake it exists to prevent:
//!
//! - **A cell is cited, never summarised.** Everything read out carries its A1
//!   address, because the item's requirement is an answer *with the cells
//!   cited*, and an agent that says "your total is 4 200" without saying where
//!   it read it cannot be checked. [`cell_ref`] and [`parse_a1`] are the one
//!   place addresses are formed and read.
//! - **A write edits the document it was given.** Mutations go through
//!   [`set_formula`] and [`set_value`], which reach into the caller's own parsed
//!   snapshot and change one cell of it. Nothing here re-serialises a workbook
//!   from the model below: a snapshot carries styles, merges, filters,
//!   validations and plugin data this file does not model, and rebuilding it
//!   from what we understood would silently delete all of them.
//! - **Nothing here computes.** No sum, no average, no arithmetic on anybody's
//!   figures. A formula written into a cell is stored for the spreadsheet
//!   engine to evaluate when the sheet is next opened; a number read out of a
//!   cell is passed on as the literal it was stored as. An agent that did its
//!   own arithmetic would produce a second, invisible answer that disagrees
//!   with the grid.
//! - **Everything is bounded.** A workbook can hold a million cells; a turn
//!   holds a few hundred at most. Every reader takes a limit and reports having
//!   hit it, so a truncated read is never mistaken for a complete one.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// Univer's `CellValueType` — the `t` beside a cell's value.
const T_STRING: u64 = 1;
const T_NUMBER: u64 = 2;
const T_BOOLEAN: u64 = 3;

/// The largest column index this module will form an address for (`ZZ`).
///
/// Not a spreadsheet limit — a sanity bound, so a corrupt or hostile snapshot
/// claiming a column at index four billion cannot make us build a string of
/// letters proportional to it.
pub const MAX_COLUMN: u32 = 701;

/// The largest row index likewise (a million rows, Excel's own ceiling).
pub const MAX_ROW: u32 = 1_048_575;

/// Why a snapshot could not be read as a workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetError {
    /// The blob was not JSON at all.
    NotJson,
    /// JSON, but not a workbook: no `sheets` object to read.
    NotAWorkbook,
    /// A workbook with no tabs in it.
    NoTabs,
}

impl SheetError {
    /// The reason code the tool result carries. A code and not a sentence: the
    /// words a user reads are the model's or the client's, in their language.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotJson => "notJson",
            Self::NotAWorkbook => "notAWorkbook",
            Self::NoTabs => "noTabs",
        }
    }
}

/// What one cell holds, as the snapshot stored it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    /// Zero-based row.
    pub row: u32,
    /// Zero-based column.
    pub col: u32,
    /// The value as text — the string itself, the number's own literal, or
    /// `TRUE`/`FALSE`. For a formula cell this is the value the engine last
    /// cached, which may be empty if it has never been opened.
    pub text: String,
    /// The formula, when the cell is one (`=SUM(B2:B9)`).
    pub formula: Option<String>,
    /// Whether the sheet **holds** this as a number, rather than as text that
    /// happens to look like one. A column of numbers typed as text is the
    /// single most common thing wrong with a spreadsheet, and this is the bit
    /// that tells the two apart — so it is what the cell stores, never what the
    /// cell is labelled with.
    pub numeric: bool,
}

impl GridCell {
    /// This cell's A1 address.
    #[must_use]
    pub fn reference(&self) -> String {
        cell_ref(self.row, self.col)
    }
}

/// One tab of a workbook, with the cells that have anything in them.
#[derive(Debug, Clone)]
pub struct Tab {
    /// The key this tab has in the snapshot's `sheets` object — what a write
    /// must be addressed to. Not shown to anybody; the name is.
    pub key: String,
    /// The tab's name, as it reads on screen.
    pub name: String,
    /// Only the cells that hold something, by (row, column).
    pub cells: BTreeMap<(u32, u32), GridCell>,
}

impl Tab {
    /// The cell at a position, if anything is there.
    #[must_use]
    pub fn cell(&self, row: u32, col: u32) -> Option<&GridCell> {
        self.cells.get(&(row, col))
    }

    /// The last row and column that hold anything — the used range — or `None`
    /// for an empty tab.
    #[must_use]
    pub fn extent(&self) -> Option<(u32, u32)> {
        let mut last = None;
        for &(row, col) in self.cells.keys() {
            let (r, c) = last.unwrap_or((row, col));
            last = Some((r.max(row), c.max(col)));
        }
        last
    }

    /// The used range in A1 notation (`A1:D42`), or `None` for an empty tab.
    #[must_use]
    pub fn used_range(&self) -> Option<String> {
        let (row, col) = self.extent()?;
        let (first_row, first_col) = self.first()?;
        Some(format!(
            "{}:{}",
            cell_ref(first_row, first_col),
            cell_ref(row, col)
        ))
    }

    /// The topmost, leftmost filled position.
    fn first(&self) -> Option<(u32, u32)> {
        let mut first: Option<(u32, u32)> = None;
        for &(row, col) in self.cells.keys() {
            let (r, c) = first.unwrap_or((row, col));
            first = Some((r.min(row), c.min(col)));
        }
        first
    }

    /// The row that labels the columns, if the tab has one.
    ///
    /// The first filled row, and only when **every** cell in it is text and
    /// none of them is a formula. That is deliberately strict: a header guessed
    /// wrong is worse than none at all, because it would caption somebody's
    /// figures with a number that happens to sit on top of them.
    #[must_use]
    pub fn header_row(&self) -> Option<u32> {
        let (row, _) = self.first()?;
        let cells: Vec<&GridCell> = self
            .cells
            .iter()
            .filter(|&(&(r, _), _)| r == row)
            .map(|(_, cell)| cell)
            .collect();
        if cells.len() < 2 {
            // One word on its own is a title, not a header row.
            return None;
        }
        cells
            .iter()
            .all(|cell| !cell.numeric && cell.formula.is_none() && !cell.text.trim().is_empty())
            .then_some(row)
    }

    /// The label above a column, from [`Self::header_row`].
    #[must_use]
    pub fn header_label(&self, col: u32) -> Option<&str> {
        let row = self.header_row()?;
        self.cell(row, col).map(|cell| cell.text.as_str())
    }

    /// The filled rows, in order, each with its cells left to right.
    #[must_use]
    pub fn rows(&self) -> Vec<(u32, Vec<&GridCell>)> {
        let mut out: Vec<(u32, Vec<&GridCell>)> = Vec::new();
        for cell in self.cells.values() {
            match out.last_mut() {
                Some((row, cells)) if *row == cell.row => cells.push(cell),
                _ => out.push((cell.row, vec![cell])),
            }
        }
        out
    }
}

/// A workbook snapshot, read.
#[derive(Debug, Clone)]
pub struct Workbook {
    /// The workbook's own name, which need not be the Drive node's.
    pub name: String,
    /// Its tabs, in the order the snapshot orders them.
    pub tabs: Vec<Tab>,
}

impl Workbook {
    /// Reads a stored snapshot.
    ///
    /// Tolerant on purpose about everything that is not structure: a cell with
    /// no type, a row key that is not a number, a tab with no name — all are
    /// skipped or defaulted rather than failing the read, because a workbook
    /// that has been through an import, an export and three versions of an
    /// editor is not a document we get to be strict with. The three things that
    /// *are* refused are the three that leave nothing to read at all.
    ///
    /// # Errors
    /// [`SheetError`] when the bytes are not JSON, are not a workbook, or hold
    /// no tabs.
    pub fn read(raw: &Value) -> Result<Self, SheetError> {
        let book = raw.as_object().ok_or(SheetError::NotAWorkbook)?;
        let sheets = book
            .get("sheets")
            .and_then(Value::as_object)
            .ok_or(SheetError::NotAWorkbook)?;
        let ordered: Vec<String> = match book.get("sheetOrder").and_then(Value::as_array) {
            // The stated order, minus any key that no longer exists, plus any
            // tab the order forgot — an editor that crashed mid-rename can
            // leave either.
            Some(order) => {
                let mut keys: Vec<String> = order
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|key| sheets.contains_key(*key))
                    .map(str::to_owned)
                    .collect();
                for key in sheets.keys() {
                    if !keys.iter().any(|seen| seen == key) {
                        keys.push(key.clone());
                    }
                }
                keys
            }
            None => sheets.keys().cloned().collect(),
        };
        let tabs: Vec<Tab> = ordered
            .into_iter()
            .map(|key| {
                let sheet = sheets.get(&key);
                let name = sheet
                    .and_then(|s| s.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| key.clone());
                let cells = sheet
                    .and_then(|s| s.get("cellData"))
                    .map(read_cells)
                    .unwrap_or_default();
                Tab { key, name, cells }
            })
            .collect();
        if tabs.is_empty() {
            return Err(SheetError::NoTabs);
        }
        Ok(Self {
            name: book
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            tabs,
        })
    }

    /// The tab a name refers to, or the first one when nothing was named.
    ///
    /// An exact name wins over a partial one, and two partial matches are no
    /// match: the same rule the rest of the agent surface resolves names by, so
    /// "Q1" does not silently pick "Q1 draft" over "Q1".
    #[must_use]
    pub fn tab(&self, wanted: Option<&str>) -> Option<&Tab> {
        let Some(wanted) = wanted else {
            return self.tabs.first();
        };
        let needle = wanted.trim().to_lowercase();
        if needle.is_empty() {
            return self.tabs.first();
        }
        if let Some(exact) = self
            .tabs
            .iter()
            .find(|tab| tab.name.trim().to_lowercase() == needle)
        {
            return Some(exact);
        }
        let mut partial = self
            .tabs
            .iter()
            .filter(|tab| tab.name.to_lowercase().contains(&needle));
        let first = partial.next()?;
        partial.next().is_none().then_some(first)
    }
}

/// Reads one tab's `cellData` — `{row: {column: {v, t, f}}}`, keys as strings.
fn read_cells(data: &Value) -> BTreeMap<(u32, u32), GridCell> {
    let mut cells = BTreeMap::new();
    let Some(rows) = data.as_object() else {
        return cells;
    };
    for (row_key, columns) in rows {
        let Ok(row) = row_key.parse::<u32>() else {
            continue;
        };
        let Some(columns) = columns.as_object() else {
            continue;
        };
        for (col_key, cell) in columns {
            let Ok(col) = col_key.parse::<u32>() else {
                continue;
            };
            if row > MAX_ROW || col > MAX_COLUMN {
                continue;
            }
            if let Some(cell) = read_cell(row, col, cell) {
                cells.insert((row, col), cell);
            }
        }
    }
    cells
}

/// One `{v, t, f}`, or `None` when there is nothing in it.
///
/// A cell holding only a style is not a cell holding something: the grid is
/// full of them wherever anybody has ever coloured a row, and returning them
/// would put hundreds of blanks in front of the model.
fn read_cell(row: u32, col: u32, cell: &Value) -> Option<GridCell> {
    let cell = cell.as_object()?;
    let formula = cell
        .get("f")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_owned);
    let value = cell.get("v");
    let declared = cell.get("t").and_then(Value::as_u64);
    let (text, numeric) = match value {
        // A string is text however the cell is *labelled*. A snapshot that says
        // `{"v": " 1300 ", "t": 2}` is a column somebody pasted in and the
        // editor typed optimistically — it is exactly the thing a tidy exists
        // to fix, so calling it a number here would hide it from the one tool
        // that can.
        Some(Value::String(text)) => (text.clone(), false),
        Some(Value::Number(number)) => (number.to_string(), declared != Some(T_STRING)),
        Some(Value::Bool(flag)) => ((if *flag { "TRUE" } else { "FALSE" }).to_owned(), false),
        _ => (String::new(), false),
    };
    let numeric = numeric && declared != Some(T_BOOLEAN);
    if text.trim().is_empty() && formula.is_none() {
        return None;
    }
    Some(GridCell {
        row,
        col,
        text,
        formula,
        numeric,
    })
}

// ---- addresses ---------------------------------------------------------------

/// `0` → `A`, `25` → `Z`, `26` → `AA`. Capped at [`MAX_COLUMN`].
#[must_use]
pub fn column_label(col: u32) -> String {
    let mut n = col.min(MAX_COLUMN) + 1;
    let mut out = String::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.insert(0, char::from(b'A' + u8::try_from(rem).unwrap_or(0)));
        n = (n - 1) / 26;
    }
    out
}

/// A zero-based position as an A1 address.
#[must_use]
pub fn cell_ref(row: u32, col: u32) -> String {
    format!("{}{}", column_label(col), row.saturating_add(1))
}

/// An A1 address (`B7`, `$b$7`) as a zero-based `(row, column)`.
///
/// The `$` of an absolute reference is accepted and dropped: it is a statement
/// about copying a formula, not about which cell is meant, and a model that
/// wrote `$B$7` meant B7.
#[must_use]
pub fn parse_a1(text: &str) -> Option<(u32, u32)> {
    let cleaned: String = text.trim().chars().filter(|c| *c != '$').collect();
    if cleaned.is_empty() {
        return None;
    }
    let split = cleaned.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = cleaned.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut col: u32 = 0;
    for letter in letters.chars() {
        let digit = u32::from(letter.to_ascii_uppercase() as u8 - b'A') + 1;
        col = col.checked_mul(26)?.checked_add(digit)?;
    }
    let row = digits.parse::<u32>().ok()?.checked_sub(1)?;
    let col = col.checked_sub(1)?;
    (row <= MAX_ROW && col <= MAX_COLUMN).then_some((row, col))
}

/// A column letter on its own (`C`, `ab`) as a zero-based index.
#[must_use]
pub fn parse_column(text: &str) -> Option<u32> {
    let cleaned: String = text.trim().chars().filter(|c| *c != '$').collect();
    if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    parse_a1(&format!("{cleaned}1")).map(|(_, col)| col)
}

// ---- reading a formula --------------------------------------------------------

/// One thing a formula points at: the text as written, and the cells it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaRef {
    /// As it appears in the formula (`B2`, `B2:B9`).
    pub text: String,
    /// Top-left and bottom-right, zero-based.
    pub from: (u32, u32),
    pub to: (u32, u32),
}

impl FormulaRef {
    /// How many cells it covers.
    #[must_use]
    pub fn size(&self) -> u64 {
        let rows = u64::from(self.to.0.saturating_sub(self.from.0)) + 1;
        let cols = u64::from(self.to.1.saturating_sub(self.from.1)) + 1;
        rows * cols
    }

    /// The positions it covers, up to `limit`.
    #[must_use]
    pub fn positions(&self, limit: usize) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for row in self.from.0..=self.to.0 {
            for col in self.from.1..=self.to.1 {
                if out.len() >= limit {
                    return out;
                }
                out.push((row, col));
            }
        }
        out
    }
}

/// The cells and ranges a formula refers to, in the order they are written.
///
/// A scanner rather than a parser: it recognises `A1` and `A1:B9` and ignores
/// everything else, which is the right trade for explaining a formula. It
/// deliberately does not resolve a name, another sheet's range or a table
/// reference — reporting those as cells of *this* tab would be a wrong answer,
/// and reporting nothing is a true one.
#[must_use]
pub fn formula_refs(formula: &str) -> Vec<FormulaRef> {
    let bytes: Vec<char> = formula.chars().collect();
    let mut out: Vec<FormulaRef> = Vec::new();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == '"' {
            in_string = !in_string;
            i += 1;
            continue;
        }
        if in_string || !(ch.is_ascii_alphabetic() || ch == '$') {
            i += 1;
            continue;
        }
        // A run of address characters. A run touching '(' is a function name,
        // and a run touching '!' names another sheet — neither is a reference
        // into this tab.
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == '$') {
            i += 1;
        }
        let first: String = bytes[start..i].iter().collect();
        let followed_by = bytes.get(i).copied();
        if followed_by == Some('(') || followed_by == Some('!') {
            continue;
        }
        if start > 0 && (bytes[start - 1] == '!' || bytes[start - 1].is_ascii_digit()) {
            continue;
        }
        let Some(from) = parse_a1(&first) else {
            continue;
        };
        // A colon makes it a range, if what follows is also an address.
        if followed_by == Some(':') {
            let second_start = i + 1;
            let mut j = second_start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == '$') {
                j += 1;
            }
            let second: String = bytes[second_start..j].iter().collect();
            if let Some(to) = parse_a1(&second) {
                out.push(FormulaRef {
                    text: format!("{first}:{second}"),
                    from: (from.0.min(to.0), from.1.min(to.1)),
                    to: (from.0.max(to.0), from.1.max(to.1)),
                });
                i = j;
                continue;
            }
        }
        out.push(FormulaRef {
            text: first,
            from,
            to: from,
        });
    }
    out
}

/// The function names a formula calls, upper-cased, in order and without
/// repeats — `=SUM(A1:A9)/COUNT(A1:A9)` is `["SUM", "COUNT"]`.
#[must_use]
pub fn formula_functions(formula: &str) -> Vec<String> {
    let chars: Vec<char> = formula.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        if chars[i] == '"' {
            in_string = !in_string;
            i += 1;
            continue;
        }
        if in_string || !(chars[i].is_ascii_alphabetic() || chars[i] == '_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len()
            && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
        {
            i += 1;
        }
        if chars.get(i) == Some(&'(') {
            let name: String = chars[start..i].iter().collect();
            let name = name.to_uppercase();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

// ---- answering from the data --------------------------------------------------

/// One row that matched a question, and which of its cells did.
#[derive(Debug, Clone)]
pub struct RowMatch {
    /// The tab it is in, by name.
    pub tab: String,
    /// Zero-based row.
    pub row: u32,
    /// Which of the row's cells contained one of the terms.
    pub matched: Vec<(u32, u32)>,
}

/// The words of a question worth searching for.
///
/// Everything two characters or shorter is dropped, and so is a short list of
/// words every question contains. Not a stop-word list for a language we might
/// not be in — just the shape of a question — because a term nobody typed as
/// data ("what", "the") matches nothing and only costs a scan.
#[must_use]
pub fn search_terms(question: &str) -> Vec<String> {
    const ASKING: &[&str] = &[
        "what", "which", "who", "whose", "where", "when", "how", "why", "the", "and", "for", "was",
        "were", "are", "did", "does", "our", "this", "that", "with", "from", "much", "many",
        "total", "sheet", "cell", "row", "column", "tab",
    ];
    let mut terms: Vec<String> = Vec::new();
    for word in question
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .map(str::trim)
    {
        let lower = word.to_lowercase();
        if lower.chars().count() < 3 || ASKING.contains(&lower.as_str()) {
            continue;
        }
        if !terms.contains(&lower) {
            terms.push(lower);
        }
    }
    terms
}

/// The rows of a tab that contain any of the terms, best first.
///
/// "Best" is how many distinct terms the row contains, then how far up it is.
/// The header row is never a match: a question about "revenue" wants the rows
/// of revenue, not the word at the top of the column.
#[must_use]
pub fn find_rows(tab: &Tab, terms: &[String], limit: usize) -> Vec<RowMatch> {
    if terms.is_empty() {
        return Vec::new();
    }
    let header = tab.header_row();
    let mut scored: Vec<(usize, u32, RowMatch)> = Vec::new();
    for (row, cells) in tab.rows() {
        if Some(row) == header {
            continue;
        }
        let mut hit: Vec<(u32, u32)> = Vec::new();
        let mut distinct = 0;
        for term in terms {
            let mut found = false;
            for cell in &cells {
                if cell.text.to_lowercase().contains(term.as_str()) {
                    if !hit.contains(&(cell.row, cell.col)) {
                        hit.push((cell.row, cell.col));
                    }
                    found = true;
                }
            }
            if found {
                distinct += 1;
            }
        }
        if distinct > 0 {
            scored.push((
                distinct,
                row,
                RowMatch {
                    tab: tab.name.clone(),
                    row,
                    matched: hit,
                },
            ));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, found)| found)
        .collect()
}

// ---- tidying a column ----------------------------------------------------------

/// What tidying one cell's text would do to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tidied {
    /// The text after tidying.
    pub text: String,
    /// Blanks removed from the ends.
    pub trimmed: bool,
    /// A run of blanks inside collapsed to one space (a non-breaking space
    /// counts as a blank, and is the one a pasted column is usually full of).
    pub collapsed: bool,
    /// Text that is a number, and would be stored as one.
    pub number: Option<Value>,
}

impl Tidied {
    /// Whether tidying would change anything at all.
    #[must_use]
    pub fn changes(&self) -> bool {
        self.trimmed || self.collapsed || self.number.is_some()
    }

    /// The reason codes for what it did, for the tool result.
    #[must_use]
    pub fn reasons(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.trimmed {
            out.push("trimmed");
        }
        if self.collapsed {
            out.push("collapsed");
        }
        if self.number.is_some() {
            out.push("storedAsNumber");
        }
        out
    }
}

/// What tidying would do to one piece of text.
///
/// Three things and no more: the ends, the runs of blanks inside, and text that
/// is a number stored as text. Deliberately nothing else — not case, not
/// spelling, not dates, not a currency symbol. Every one of those is a decision
/// about what somebody's data *means*, and a wrong guess silently rewrites a
/// record; these three are decisions about how it was typed.
///
/// `numbers` off leaves a numeric string alone, for the column of order codes
/// that only looks like figures.
#[must_use]
pub fn tidy(text: &str, numbers: bool) -> Tidied {
    let trimmed_text = text.trim_matches(is_blank);
    let trimmed = trimmed_text.len() != text.len();
    let mut collapsed = false;
    let mut out = String::with_capacity(trimmed_text.len());
    let mut blank_run = false;
    for ch in trimmed_text.chars() {
        if is_blank(ch) {
            if blank_run {
                collapsed = true;
                continue;
            }
            blank_run = true;
            // A non-breaking space on its own is still a change to a space.
            if ch != ' ' {
                collapsed = true;
            }
            out.push(' ');
            continue;
        }
        blank_run = false;
        out.push(ch);
    }
    let number = numbers.then(|| numeric_value(&out)).flatten();
    Tidied {
        text: out,
        trimmed,
        collapsed,
        number,
    }
}

/// Whether a character counts as a blank for tidying — ordinary whitespace plus
/// the non-breaking and narrow spaces a pasted column carries.
fn is_blank(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{00a0}' | '\u{202f}' | '\u{2007}' | '\u{feff}')
}

/// The JSON number a piece of text *is*, or `None` if it is not one.
///
/// A leading `+`, a thousands separator, a currency symbol and a percent sign
/// are all refused rather than interpreted — each would be us deciding what the
/// number means.
///
/// **The conversion happens only when the number reads back exactly as it was
/// typed.** JSON numbers are `f64` below the surface, so `12.30` comes back
/// `12.3`, a leading zero is lost, and an eighteen-digit reference number can
/// come back a different eighteen-digit number. Each of those is us changing a
/// figure in somebody's record while claiming to have tidied its typing, so a
/// text that does not survive the round trip is left as text. What is left
/// behind is a cell the user can still see and fix; what the other choice
/// leaves behind is a silently altered number.
fn numeric_value(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.is_empty() || text.len() > 40 {
        return None;
    }
    let body = text.strip_prefix('-').unwrap_or(text);
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if body.matches('.').count() > 1 || body.starts_with('.') || body.ends_with('.') {
        return None;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Number(number)) if number.to_string() == text => Some(Value::Number(number)),
        _ => None,
    }
}

// ---- writing -------------------------------------------------------------------

/// Puts a formula in a cell of the caller's snapshot, keeping everything else
/// about that cell.
///
/// The cached value and its type go, because they were the *old* formula's
/// answer and leaving them would show a stale number until the sheet is next
/// recalculated. The style stays: a write from a chat message is not a reason
/// for somebody's column to lose its formatting.
/// Returns whether it was written — `false` only for a snapshot that is not an
/// object at all, which [`Workbook::read`] would already have refused.
pub fn set_formula(snapshot: &mut Value, tab: &str, row: u32, col: u32, formula: &str) -> bool {
    let Some(cell) = cell_slot(snapshot, tab, row, col) else {
        return false;
    };
    cell.remove("v");
    cell.remove("t");
    cell.insert("f".to_owned(), Value::String(formula.to_owned()));
    true
}

/// Puts a literal value in a cell of the caller's snapshot, with the type the
/// editor reads. Any formula that was there goes — a value replaces it.
///
/// Returns whether it was written, on the same terms as [`set_formula`].
pub fn set_value(snapshot: &mut Value, tab: &str, row: u32, col: u32, value: &Value) -> bool {
    let kind = match value {
        Value::Number(_) => T_NUMBER,
        Value::Bool(_) => T_BOOLEAN,
        _ => T_STRING,
    };
    let Some(cell) = cell_slot(snapshot, tab, row, col) else {
        return false;
    };
    cell.remove("f");
    cell.insert("v".to_owned(), value.clone());
    cell.insert("t".to_owned(), Value::from(kind));
    true
}

/// The `{v, t, f}` object for one cell, making the path to it as needed.
///
/// Every level is created only when it is missing, and replaced only when what
/// is there is not an object, so a snapshot keeps every other tab, every other
/// cell, and every other key of the cell being written — including the style,
/// the note and whatever a plugin left there.
fn cell_slot<'a>(
    snapshot: &'a mut Value,
    tab: &str,
    row: u32,
    col: u32,
) -> Option<&'a mut Map<String, Value>> {
    let book = snapshot.as_object_mut()?;
    let sheets = child(book, "sheets")?;
    let sheet = child(sheets, tab)?;
    let data = child(sheet, "cellData")?;
    let row_slot = child(data, &row.to_string())?;
    child(row_slot, &col.to_string())
}

/// One level down, as an object.
fn child<'a>(parent: &'a mut Map<String, Value>, key: &str) -> Option<&'a mut Map<String, Value>> {
    let slot = parent
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    slot.as_object_mut()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A small book shaped the way a real one is: a header row, three rows of
    /// figures, one total in a formula, and a column of numbers somebody pasted
    /// in as text with spaces around them.
    fn book() -> Value {
        json!({
            "id": "wb-1",
            "name": "Q1",
            "sheetOrder": ["sheet-1", "sheet-2"],
            "sheets": {
                "sheet-1": {
                    "id": "sheet-1",
                    "name": "Revenue",
                    "rowCount": 100,
                    "columnCount": 26,
                    "cellData": {
                        "0": {
                            "0": {"v": "Region", "t": 1},
                            "1": {"v": "January", "t": 1},
                            "2": {"v": "February", "t": 1}
                        },
                        "1": {
                            "0": {"v": "North", "t": 1},
                            "1": {"v": 1200, "t": 2},
                            "2": {"v": " 1 300 ", "t": 1}
                        },
                        "2": {
                            "0": {"v": "South", "t": 1},
                            "1": {"v": 900, "t": 2},
                            "2": {"v": "1100", "t": 1}
                        },
                        "3": {
                            "0": {"v": "Total", "t": 1},
                            "1": {"f": "=SUM(B2:B3)", "v": 2100, "t": 2}
                        },
                        "4": { "0": {"s": "style-7"} }
                    }
                },
                "sheet-2": { "id": "sheet-2", "name": "Notes", "cellData": {} }
            }
        })
    }

    #[test]
    fn a_snapshot_reads_as_tabs_and_only_the_cells_that_hold_something() {
        let read = Workbook::read(&book()).unwrap();
        assert_eq!(read.name, "Q1");
        assert_eq!(
            read.tabs
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["Revenue", "Notes"]
        );
        let revenue = &read.tabs[0];
        // Eleven filled positions; the styled blank is not one of them.
        assert_eq!(revenue.cells.len(), 11);
        assert!(revenue.cell(4, 0).is_none(), "a style is not a value");
        assert_eq!(revenue.used_range().as_deref(), Some("A1:C4"));
        assert_eq!(read.tabs[1].used_range(), None);

        // The number, the text-that-is-a-number, and the formula are each
        // reported as what they are.
        let january = revenue.cell(1, 1).unwrap();
        assert_eq!((january.text.as_str(), january.numeric), ("1200", true));
        let february = revenue.cell(1, 2).unwrap();
        assert_eq!(
            (february.text.as_str(), february.numeric),
            (" 1 300 ", false)
        );
        // A string the snapshot LABELS a number is still text: that pairing is
        // what a pasted column looks like, and it is what a tidy exists for.
        let mislabelled = Workbook::read(&json!({"sheets": {"s": {"name": "S", "cellData": {
            "0": {"0": {"v": " 42 ", "t": 2}}
        }}}}))
        .unwrap();
        assert!(!mislabelled.tabs[0].cell(0, 0).unwrap().numeric);
        let total = revenue.cell(3, 1).unwrap();
        assert_eq!(total.formula.as_deref(), Some("=SUM(B2:B3)"));
        assert_eq!(total.text, "2100");
        assert_eq!(total.reference(), "B4");
    }

    #[test]
    fn a_workbook_with_nothing_to_read_says_which_kind_of_nothing() {
        assert_eq!(
            Workbook::read(&json!([])).unwrap_err(),
            SheetError::NotAWorkbook
        );
        assert_eq!(
            Workbook::read(&json!({})).unwrap_err(),
            SheetError::NotAWorkbook
        );
        assert_eq!(
            Workbook::read(&json!({"sheets": {}})).unwrap_err(),
            SheetError::NoTabs
        );
        assert_eq!(SheetError::NotJson.as_str(), "notJson");
        // An empty tab is a workbook, not an error: a new sheet has one.
        let blank = json!({"sheets": {"s1": {"name": "Sheet1"}}});
        assert_eq!(Workbook::read(&blank).unwrap().tabs.len(), 1);
    }

    /// A tab the order forgot is still a tab, and an order naming a tab that is
    /// gone does not produce a phantom one. Both happen to real files.
    #[test]
    fn the_stated_order_is_repaired_rather_than_trusted() {
        let odd = json!({
            "sheetOrder": ["gone", "s2"],
            "sheets": { "s1": {"name": "First"}, "s2": {"name": "Second"} }
        });
        let read = Workbook::read(&odd).unwrap();
        assert_eq!(
            read.tabs
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["Second", "First"]
        );
    }

    #[test]
    fn addresses_round_trip_and_a_stranger_is_refused() {
        for (col, label) in [(0, "A"), (25, "Z"), (26, "AA"), (27, "AB"), (701, "ZZ")] {
            assert_eq!(column_label(col), label);
            assert_eq!(parse_column(label), Some(col));
        }
        assert_eq!(cell_ref(0, 0), "A1");
        assert_eq!(cell_ref(6, 1), "B7");
        assert_eq!(parse_a1("B7"), Some((6, 1)));
        assert_eq!(parse_a1("  $b$7 "), Some((6, 1)), "an absolute ref is B7");
        assert_eq!(parse_a1("AA100"), Some((99, 26)));
        for stranger in ["", "7", "B", "B0", "B-1", "7B", "B7B", "B 7"] {
            assert_eq!(parse_a1(stranger), None, "{stranger}");
        }
        // Beyond the bound, an address is not formed rather than guessed at.
        assert_eq!(parse_a1("AAA1"), None);
    }

    #[test]
    fn a_formula_reports_what_it_points_at_and_what_it_calls() {
        let refs = formula_refs("=SUM(B2:B9)+A1-Sheet2!C3+MAX(D4,D5)");
        assert_eq!(
            refs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            ["B2:B9", "A1", "D4", "D5"],
            "another sheet's cell is not one of this tab's"
        );
        assert_eq!(refs[0].from, (1, 1));
        assert_eq!(refs[0].to, (8, 1));
        assert_eq!(refs[0].size(), 8);
        assert_eq!(refs[0].positions(3).len(), 3);
        assert_eq!(
            formula_functions("=SUM(A1:A9)/COUNT(A1:A9)+SUM(B1)"),
            ["SUM", "COUNT"]
        );
        // Text inside quotes is text, not an address and not a function.
        let quoted = formula_refs("=IF(A1>0,\"B2\",\"\")");
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].text, "A1");
        assert_eq!(formula_functions("=\"TOTAL(\"&A1"), Vec::<String>::new());
    }

    #[test]
    fn the_header_row_is_recognised_only_when_it_is_unmistakable() {
        let read = Workbook::read(&book()).unwrap();
        let revenue = &read.tabs[0];
        assert_eq!(revenue.header_row(), Some(0));
        assert_eq!(revenue.header_label(1), Some("January"));
        // A first row with a figure in it is data, not a header — captioning
        // somebody's numbers with a number is worse than no caption.
        let numeric_top = json!({"sheets": {"s": {"name": "S", "cellData": {
            "0": {"0": {"v": "North", "t": 1}, "1": {"v": 12, "t": 2}}
        }}}});
        assert_eq!(
            Workbook::read(&numeric_top).unwrap().tabs[0].header_row(),
            None
        );
        // One word on its own is a title.
        let title = json!({"sheets": {"s": {"name": "S", "cellData": {
            "0": {"0": {"v": "Budget", "t": 1}}
        }}}});
        assert_eq!(Workbook::read(&title).unwrap().tabs[0].header_row(), None);
    }

    #[test]
    fn a_tab_is_named_exactly_or_not_at_all() {
        let read = Workbook::read(&json!({"sheets": {
            "a": {"name": "Q1"}, "b": {"name": "Q1 draft"}, "c": {"name": "Notes"}
        }, "sheetOrder": ["a", "b", "c"]}))
        .unwrap();
        assert_eq!(read.tab(None).map(|t| t.name.as_str()), Some("Q1"));
        assert_eq!(read.tab(Some("q1")).map(|t| t.name.as_str()), Some("Q1"));
        assert_eq!(
            read.tab(Some("draft")).map(|t| t.name.as_str()),
            Some("Q1 draft")
        );
        assert_eq!(
            read.tab(Some("note")).map(|t| t.name.as_str()),
            Some("Notes")
        );
        // Two partial matches is not a match: picking one would answer about
        // the wrong tab.
        assert!(read.tab(Some("Q")).is_none());
        assert!(read.tab(Some("Ledger")).is_none());
    }

    #[test]
    fn a_question_finds_the_rows_that_hold_its_words_and_never_the_header() {
        let read = Workbook::read(&book()).unwrap();
        let revenue = &read.tabs[0];
        let terms = search_terms("What did the North region bring in?");
        assert_eq!(terms, ["north", "region", "bring"]);
        let found = find_rows(revenue, &terms, 5);
        assert_eq!(found.len(), 1, "the header holds 'Region' and is not a row");
        assert_eq!(found[0].row, 1);
        assert_eq!(found[0].matched, [(1, 0)]);
        assert_eq!(found[0].tab, "Revenue");
        // Nothing to search for is no answer, not every row.
        assert!(find_rows(revenue, &search_terms("what is the total?"), 5).is_empty());
        assert!(find_rows(revenue, &[], 5).is_empty());
    }

    #[test]
    fn tidying_touches_the_typing_and_never_the_meaning() {
        let spaced = tidy("  1\u{00a0}300  ", true);
        assert_eq!(spaced.text, "1 300");
        assert!(spaced.trimmed && spaced.collapsed);
        assert_eq!(
            spaced.number, None,
            "a thousands separator is not ours to read"
        );
        assert_eq!(spaced.reasons(), ["trimmed", "collapsed"]);

        let numeric = tidy(" 1100 ", true);
        assert_eq!(numeric.number, Some(json!(1100)));
        assert_eq!(numeric.reasons(), ["trimmed", "storedAsNumber"]);
        assert_eq!(tidy("-0.5", true).number, Some(json!(-0.5)));
        assert_eq!(tidy("12.3", true).number, Some(json!(12.3)));

        // A figure that would not read back as it was typed stays text: JSON
        // numbers are f64, and tidying somebody's typing must not quietly
        // change their number. `12.30` would become `12.3`, `0041` would become
        // `41`, and a nineteen-digit reference would become a different one.
        for exact in ["12.30", "0041", "-0.50", "1234567890123456789012"] {
            assert_eq!(tidy(exact, true).number, None, "{exact}");
        }
        // …but a nineteen-digit whole number that IS exact still converts,
        // because integers do not go through f64.
        assert_eq!(
            tidy("1234567890123456789", true)
                .number
                .map(|v| v.to_string())
                .as_deref(),
            Some("1234567890123456789")
        );

        // Off, a column of codes that look like figures is left alone.
        assert_eq!(tidy("41", false).number, None);
        // Never a number: a currency, a percent, a plus, an exponent, a date.
        for text in [
            "12%",
            "+12",
            "1e5",
            "2026-01-05",
            "12,30",
            "1.2.3",
            ".5",
            "5.",
        ] {
            assert_eq!(tidy(text, true).number, None, "{text}");
        }
        // Nothing to do is reported as nothing to do.
        assert!(!tidy("North", true).changes());
        assert!(tidy("North", true).reasons().is_empty());
    }

    #[test]
    fn a_write_changes_one_cell_and_leaves_the_rest_of_the_document_alone() {
        let mut snapshot = book();
        assert!(set_formula(&mut snapshot, "sheet-1", 3, 2, "=SUM(C2:C3)"));
        let written = &snapshot["sheets"]["sheet-1"]["cellData"]["3"]["2"];
        assert_eq!(written["f"], json!("=SUM(C2:C3)"));
        assert!(
            written.get("v").is_none(),
            "a stale cached answer is removed"
        );

        // The style on a cell being written survives.
        assert!(set_value(&mut snapshot, "sheet-1", 4, 0, &json!(42)));
        let styled = &snapshot["sheets"]["sheet-1"]["cellData"]["4"]["0"];
        assert_eq!(styled["s"], json!("style-7"));
        assert_eq!(styled["v"], json!(42));
        assert_eq!(styled["t"], json!(2));

        // Replacing a formula with a value drops the formula.
        assert!(set_value(&mut snapshot, "sheet-1", 3, 1, &json!("n/a")));
        let replaced = &snapshot["sheets"]["sheet-1"]["cellData"]["3"]["1"];
        assert!(replaced.get("f").is_none());
        assert_eq!(replaced["t"], json!(1));

        // Everything else is untouched: the other tab, the row counts, the
        // workbook id, and every cell that was not addressed.
        assert_eq!(snapshot["id"], json!("wb-1"));
        assert_eq!(snapshot["sheets"]["sheet-2"]["name"], json!("Notes"));
        assert_eq!(snapshot["sheets"]["sheet-1"]["rowCount"], json!(100));
        assert_eq!(
            snapshot["sheets"]["sheet-1"]["cellData"]["1"]["1"]["v"],
            json!(1200)
        );
        // …and the result still reads as a workbook.
        assert!(Workbook::read(&snapshot).is_ok());
    }

    #[test]
    fn a_write_into_a_cell_that_was_never_there_makes_the_path_to_it() {
        let mut snapshot = json!({"sheets": {"s1": {"name": "Sheet1"}}});
        assert!(set_formula(&mut snapshot, "s1", 9, 3, "=NOW()"));
        assert_eq!(
            snapshot["sheets"]["s1"]["cellData"]["9"]["3"]["f"],
            json!("=NOW()")
        );
        let read = Workbook::read(&snapshot).unwrap();
        assert_eq!(
            read.tabs[0].cell(9, 3).unwrap().formula.as_deref(),
            Some("=NOW()")
        );
        // A snapshot that is not an object cannot be written and says so.
        let mut broken = json!("not a workbook");
        assert!(!set_value(&mut broken, "s1", 0, 0, &json!(1)));
    }
}
