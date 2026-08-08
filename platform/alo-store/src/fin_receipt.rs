//! Reading a receipt — the extractor trait, and the deterministic
//! implementation that is the only one this repository ships (alo Finance,
//! ADR 0035, wave B4; `docs/design/finance.md`, "Expenses, receipts and
//! mileage").
//!
//! # Nothing here decides anything
//!
//! This module turns the text of a receipt into **candidate fields with a
//! confidence and the evidence each came from**, and that is the whole of its
//! job. It writes no row, reaches no database and has no tenant: it is a pure
//! function from characters to guesses. Every field it produces is confirmed
//! by a human in the create form before an expense claim exists
//! (`POST /finance/receipts` returns these and writes nothing — B4.06b). The
//! design note's reason for that shape is worth repeating here, where the
//! temptation lives: *a draft in a list is a thing somebody approves without
//! reading, and the whole value of the confirmation step is that the numbers
//! are looked at once by somebody who was there.*
//!
//! Which is also why every field is optional and why nothing is invented. An
//! unreadable receipt yields an empty [`ParsedReceipt`] and the person types
//! what they see — the same form, one step longer.
//!
//! # VAT is stated, never derived
//!
//! [`crate::fin_expenses`]' rule reaches its sharpest point here, because here
//! the arithmetic would be so easy. A receipt that shows a rate and no tax
//! amount yields [`ParsedReceipt::vat_rate_bp`] and **no**
//! [`ParsedReceipt::vat_cents`]: computing `gross × 19 / 119` would put a
//! number a tax inspector asks about on a form a human then confirms, and a
//! confirmed guess is indistinguishable from a read fact once it is stored.
//!
//! The one place a computation appears is *choosing between amounts the
//! receipt itself printed*: a German VAT table row reads `19% 10,00 1,90
//! 11,90`, and knowing which of the three is the tax means checking which one
//! is consistent with the rate. That selects a stated number; it never
//! produces one.
//!
//! # The seam a human wires
//!
//! [`ReceiptExtractor`] is a trait with exactly one implementation today,
//! [`PatternExtractor`] — text patterns, no model, no network, no cost. An AI
//! backend is a second implementation of the same trait (ADR 0029, EU-only
//! inference), wired by a human who has an endpoint and a contract; the
//! autonomous loop never calls a model, and the fixture suite in
//! `tests/fin_receipt_fixtures.rs` is what proves the seam holds a second
//! implementation to the same contract. `Send + Sync` is on the trait for that
//! day: an HTTP-backed extractor lives in application state.
//!
//! # A receipt is a file with somebody's life in it
//!
//! It names a restaurant, a pharmacy, a city on a date. Nothing in this module
//! logs, and [`ParsedReceipt`] carries the receipt's own lines only so the UI
//! can highlight the evidence — the caller must treat them as it treats the
//! file: shown to the claimant and their approver, never to a log.

use time::{Date, Duration};

use crate::billing_field::VAT_RATE_MAX_BP;
use crate::fin_expenses::MERCHANT_MAX;
use crate::money_text::parse_amount_cents;

/// Most lines of a receipt we read. A till roll is tens of lines and a hotel
/// folio is hundreds; beyond this we are reading something that is not a
/// receipt, and the spans would index a page nobody is going to show.
pub const RECEIPT_LINES_MAX: usize = 400;

/// Longest line we keep, in characters. A PDF text layer sometimes runs a
/// whole page together; the tail of such a line holds no field we want and
/// truncating it bounds the scanning work per line.
pub const RECEIPT_LINE_CHARS: usize = 300;

/// Largest amount we will offer as a candidate, in cents (€10,000,000 — the
/// ceiling [`crate::billing_field::UNIT_PRICE_MAX_CENTS`] sets for a price).
/// Above it the token is an order number, a phone number or an IBAN fragment,
/// never a receipt total.
pub const AMOUNT_MAX_CENTS: i64 = 1_000_000_000;

/// How far back a receipt may be dated before we stop believing the reading.
/// A claim submitted for something bought eleven years ago is not a claim, and
/// a "date" that old is a printed VAT number that happened to look like one.
const DATE_MAX_AGE_YEARS: i64 = 10;

/// How sure the extractor is of a field.
///
/// It is a coarse three-step on purpose: a percentage would invite a threshold,
/// and a threshold would invite skipping the confirmation for the values above
/// it. Every field is confirmed by a human whatever this says; the confidence
/// exists so the form can order the person's attention, not so software can
/// act without them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The receipt labelled it: a total on a line that says "Summe", a date
    /// after the word "Datum", a company name with a legal form.
    High,
    /// Read from an unlabelled but well-formed pattern, or chosen between
    /// several candidates by a rule.
    Medium,
    /// A fallback: the largest amount on the receipt, the first plausible
    /// line, something recovered from the file's name.
    Low,
}

/// Where a field was read from, so the form can show the person *why*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// Characters `start..end` (a character range, not bytes) of
    /// `ParsedReceipt::lines[line]`.
    Text {
        /// Index into [`ParsedReceipt::lines`].
        line: usize,
        /// First character of the value, inclusive.
        start: usize,
        /// One past the last character of the value.
        end: usize,
    },
    /// The uploaded file's own name. Nothing in the document said it, which is
    /// why every field with this evidence is [`Confidence::Low`].
    Filename,
}

/// A field the extractor found, what it is sure of, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<T> {
    /// The value, in the store's own units: cents for money, basis points for
    /// a rate, a [`Date`] for a day.
    pub value: T,
    /// How sure the extractor is (see [`Confidence`]).
    pub confidence: Confidence,
    /// The characters this came from.
    pub evidence: Evidence,
}

impl<T> Found<T> {
    fn new(value: T, confidence: Confidence, evidence: Evidence) -> Self {
        Self {
            value,
            confidence,
            evidence,
        }
    }
}

/// What the extractor is given: the receipt's text, its file name, and the day
/// the reading happens on.
///
/// `today` is a parameter rather than a clock read because a date reader that
/// consults the wall clock is a function whose tests pass in March and fail in
/// April. It is used for one thing: refusing a "date" that has not happened
/// yet or is a decade old.
#[derive(Debug, Clone, Copy)]
pub struct ReceiptInput<'a> {
    /// The text layer of the file, as [`crate::extract::extract_text`] returns
    /// it. An image with no text layer is an empty string, which is a valid
    /// input with an empty answer.
    pub text: &'a str,
    /// The uploaded file's name, when there is one. `holiday_receipt.jpg` says
    /// nothing; `REWE_2026-03-14.pdf` says two things.
    pub filename: Option<&'a str>,
    /// The day the upload is happening, in the tenant's reckoning.
    pub today: Date,
}

/// Everything the extractor believes about a receipt. Every field optional,
/// nothing invented.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedReceipt {
    /// The receipt's text as the extractor read it: trimmed, blank lines
    /// dropped, each line cut to [`RECEIPT_LINE_CHARS`] characters and the
    /// whole to [`RECEIPT_LINES_MAX`] lines. [`Evidence::Text`] indexes this,
    /// so the caller can highlight without re-deriving the normalisation.
    pub lines: Vec<String>,
    /// Who was paid.
    pub merchant: Option<Found<String>>,
    /// The day it was spent.
    pub spent_on: Option<Found<Date>>,
    /// What the receipt totals, in cents.
    pub gross_cents: Option<Found<i64>>,
    /// The tax the receipt **shows**, in cents. Absent when it shows none —
    /// never computed from [`Self::gross_cents`] and [`Self::vat_rate_bp`].
    pub vat_cents: Option<Found<i64>>,
    /// The rate the receipt shows, in basis points (19% → 1900). Absent on a
    /// receipt with several rates: the tax total is still a fact, the single
    /// rate is not.
    pub vat_rate_bp: Option<Found<i32>>,
    /// The ISO 4217 code the receipt names, upper case. Absent means the
    /// caller's default applies — most receipts do not name a currency at all.
    pub currency: Option<Found<String>>,
}

impl ParsedReceipt {
    /// Whether anything at all was read. `false` means the person types the
    /// claim from the paper, which is the pre-B4.06 experience and not a
    /// failure.
    #[must_use]
    pub fn found_anything(&self) -> bool {
        self.merchant.is_some()
            || self.spent_on.is_some()
            || self.gross_cents.is_some()
            || self.vat_cents.is_some()
            || self.vat_rate_bp.is_some()
    }
}

/// Reading a receipt's text into candidate fields.
///
/// One implementation ships ([`PatternExtractor`]); an AI backend is a second
/// one, wired by a human (see the module header). Implementations are pure:
/// same input, same output, no I/O — which is what makes the fixture suite a
/// contract rather than a smoke test.
pub trait ReceiptExtractor: Send + Sync {
    /// Read what can be read. Never fails: an unreadable receipt is an empty
    /// [`ParsedReceipt`], not an error, because the person can still type it.
    fn extract(&self, input: &ReceiptInput<'_>) -> ParsedReceipt;
}

/// The deterministic extractor: European receipt patterns, no model.
///
/// Dates in six spellings, the words six languages print above a total, VAT
/// lines with a rate beside them, amounts with either decimal separator. What
/// it cannot read it leaves absent.
#[derive(Debug, Clone, Copy, Default)]
pub struct PatternExtractor;

/// The extractor the application uses today.
///
/// One call site to change on the day an AI backend is wired, and one place
/// for a human to look when asking what reads their receipts.
#[must_use]
pub fn default_extractor() -> &'static dyn ReceiptExtractor {
    &PatternExtractor
}

impl ReceiptExtractor for PatternExtractor {
    fn extract(&self, input: &ReceiptInput<'_>) -> ParsedReceipt {
        let lines = normalise(input.text);
        let chars: Vec<Vec<char>> = lines.iter().map(|line| line.chars().collect()).collect();
        let lower: Vec<String> = lines.iter().map(|line| line.to_lowercase()).collect();

        let dates = scan_dates(&chars);
        let amounts: Vec<Vec<Amount>> = chars
            .iter()
            .enumerate()
            .map(|(index, line)| scan_amounts(line, &dates, index))
            .collect();

        let spent_on = pick_date(&dates, &lower, input.today)
            .or_else(|| filename_date(input.filename, input.today));
        let gross = pick_gross(&lower, &amounts);
        let (vat_cents, vat_rate_bp) = pick_vat(&chars, &lower, &amounts, gross.as_ref());
        let currency = pick_currency(&chars, gross.as_ref());
        let merchant =
            pick_merchant(&lines, &lower, &dates).or_else(|| filename_merchant(input.filename));

        ParsedReceipt {
            lines,
            merchant,
            spent_on,
            gross_cents: gross,
            vat_cents,
            vat_rate_bp,
            currency,
        }
    }
}

/// The receipt's text as the extractor works on it: tabs to spaces, lines
/// trimmed, blanks dropped, both dimensions capped.
fn normalise(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.replace('\t', " ").trim().to_owned())
        .filter(|line| !line.is_empty())
        .map(|line| {
            if line.chars().count() > RECEIPT_LINE_CHARS {
                line.chars().take(RECEIPT_LINE_CHARS).collect()
            } else {
                line
            }
        })
        .take(RECEIPT_LINES_MAX)
        .collect()
}

// ---------------------------------------------------------------- amounts --

/// An amount token found on a line, and where it sits in it.
#[derive(Debug, Clone, Copy)]
struct Amount {
    line: usize,
    start: usize,
    end: usize,
    cents: i64,
    /// Whether the receipt printed a decimal fraction. `11,90` is an amount;
    /// `2026` on a date line, `14` in a time, `4711` in an order number are
    /// digits that merely could be one, and only the labelled paths trust them.
    fraction: bool,
    /// Whether a `%` follows, which makes this a rate rather than money.
    percent: bool,
}

impl Amount {
    fn evidence(self) -> Evidence {
        Evidence::Text {
            line: self.line,
            start: self.start,
            end: self.end,
        }
    }
}

/// Separators that may appear *inside* one number.
fn is_group_sep(c: char) -> bool {
    matches!(c, '.' | ',' | '\'')
}

/// Spaces a European price is grouped with (including the ones a PDF emits).
fn is_space_sep(c: char) -> bool {
    matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}')
}

/// Every number on one line that could be an amount or a rate, in order.
///
/// Numbers inside a date the same line already yielded are skipped: `14.03.2026`
/// must never become €14.03, and a receipt printed on the 2026th of nothing is
/// not a total.
fn scan_amounts(chars: &[char], dates: &[DateHit], line: usize) -> Vec<Amount> {
    let mut out = Vec::new();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        while j < n && chars[j].is_ascii_digit() {
            j += 1;
        }
        let first_group = j - start;
        let mut fraction = false;
        loop {
            // `.`, `,` or `'` immediately followed by digits stays in the number.
            if j < n && is_group_sep(chars[j]) {
                let mut k = j + 1;
                while k < n && chars[k].is_ascii_digit() {
                    k += 1;
                }
                if k > j + 1 {
                    fraction = k - (j + 1) <= 2;
                    j = k;
                    continue;
                }
            }
            // A space groups thousands only when what follows is exactly three
            // digits: `1 234,56` is one amount, `19 1,90` is two numbers.
            if j < n && is_space_sep(chars[j]) && first_group <= 3 {
                let mut k = j + 1;
                while k < n && chars[k].is_ascii_digit() {
                    k += 1;
                }
                if k == j + 4 && !(k < n && is_group_sep(chars[k]) && k + 1 < n) {
                    j = k;
                    continue;
                }
                if k == j + 4 {
                    j = k;
                    continue;
                }
            }
            break;
        }
        let end = j;
        i = end.max(start + 1);

        // A digit run a letter runs into is a code, not a price ("A38", "x2").
        if start > 0 && chars[start - 1].is_alphabetic() {
            continue;
        }
        if dates
            .iter()
            .any(|hit| hit.line == line && start < hit.end && hit.start < end)
        {
            continue;
        }
        let token: String = chars[start..end].iter().collect();
        let Ok(cents) = parse_amount_cents(&token) else {
            continue;
        };
        if cents > AMOUNT_MAX_CENTS {
            continue;
        }
        let mut after = end;
        while after < n && is_space_sep(chars[after]) {
            after += 1;
        }
        out.push(Amount {
            line,
            start,
            end,
            cents,
            fraction,
            percent: after < n && chars[after] == '%',
        });
    }
    out
}

/// The words a receipt prints above what you actually paid.
const TOTAL_WORDS: &[&str] = &[
    "summe",
    "gesamt",
    "betrag",
    "endbetrag",
    "rechnungsbetrag",
    "zu zahlen",
    "total",
    "totaal",
    "totale",
    "te betalen",
    "bedrag",
    "montant",
    "à payer",
    "a payer",
    "amount due",
    "balance due",
    "importe",
];

/// Words that mark a line as the total *before* tax, or a running subtotal —
/// the money on such a line is real, but it is not what the person paid.
const NET_WORDS: &[&str] = &[
    "zwischensumme",
    "subtotal",
    "sous-total",
    "tussentotaal",
    "netto",
    "net total",
    "total ht",
    "hors taxe",
    "excl",
    "exkl",
    "ohne mwst",
];

/// Words that mark a line as the total *with* tax, which wins ties.
const GROSS_WORDS: &[&str] = &["brutto", "ttc", "inkl", "incl", "zu zahlen", "te betalen"];

/// What the receipt says was paid.
///
/// A labelled line wins, and among labelled lines the one that says it
/// includes tax wins, and among those the last one — a till roll prints its
/// total at the bottom. With nothing labelled we fall back to the largest
/// amount that was printed with decimals, at [`Confidence::Low`], which is a
/// guess offered for correction rather than an answer.
fn pick_gross(lower: &[String], amounts: &[Vec<Amount>]) -> Option<Found<i64>> {
    let mut best: Option<(u8, Amount)> = None;
    for (index, line) in lower.iter().enumerate() {
        if !TOTAL_WORDS.iter().any(|word| contains_word(line, word)) {
            continue;
        }
        if NET_WORDS.iter().any(|word| line.contains(word)) {
            continue;
        }
        let Some(amount) = pick_on_line(&amounts[index]) else {
            continue;
        };
        let score = u8::from(GROSS_WORDS.iter().any(|word| line.contains(word)));
        if best.is_none_or(|(previous, _)| score >= previous) {
            best = Some((score, amount));
        }
    }
    if let Some((_, amount)) = best {
        return Some(Found::new(
            amount.cents,
            Confidence::High,
            amount.evidence(),
        ));
    }
    let largest = amounts
        .iter()
        .flatten()
        .filter(|amount| amount.fraction && !amount.percent)
        .max_by_key(|amount| amount.cents)?;
    Some(Found::new(
        largest.cents,
        Confidence::Low,
        largest.evidence(),
    ))
}

/// The one amount on a line that is a total: the largest of those printed with
/// decimals, or — on a receipt that prints round numbers — simply the largest.
fn pick_on_line(amounts: &[Amount]) -> Option<Amount> {
    let money = || amounts.iter().filter(|amount| !amount.percent);
    money()
        .filter(|amount| amount.fraction)
        .max_by_key(|amount| amount.cents)
        .or_else(|| money().max_by_key(|amount| amount.cents))
        .copied()
}

// -------------------------------------------------------------------- VAT --

/// The words six languages print beside the tax.
const VAT_WORDS: &[&str] = &[
    "mwst",
    "ust",
    "umsatzsteuer",
    "mehrwertsteuer",
    "steuer",
    "tva",
    "btw",
    "vat",
    "iva",
    "taxe",
];

/// Lines that name the tax **office's** number rather than a tax amount. Every
/// receipt in Europe prints one, it is full of digit groups, and read as a VAT
/// line it would invent a tax out of a registration number.
const VAT_ID_WORDS: &[&str] = &[
    "ust-id",
    "ust-idnr",
    "ust-nr",
    "ustid",
    "steuernr",
    "steuer-nr",
    "steuernummer",
    "umsatzsteuer-id",
    "vat registration",
    "vat reg",
    "vat no",
    "vat number",
    "vat id",
    "btw-nr",
    "btw nr",
    "btw-id",
    "tva n",
    "tva intracom",
    "tax id",
    "tax no",
    "partita iva",
];

/// The tax the receipt shows and the rate it shows it at.
///
/// Three rules, in the order they matter:
///
/// 1. **Nothing is computed.** A line that names a rate and prints no amount
///    yields a rate and no tax. This is the module's whole reason for caring.
/// 2. **The gross is not the tax.** `Total 11,90 inkl. 19% MwSt` is one line
///    that is both a total line and a VAT line; the amount already read as the
///    total cannot also be read as the tax.
/// 3. **Several rates means no single rate.** A hotel bill with 7% on the room
///    and 19% on dinner has a tax total (the sum of what it printed) and no
///    one rate — reporting either rate would be a statement the paper does not
///    make.
fn pick_vat(
    chars: &[Vec<char>],
    lower: &[String],
    amounts: &[Vec<Amount>],
    gross: Option<&Found<i64>>,
) -> (Option<Found<i64>>, Option<Found<i32>>) {
    let mut taxes: Vec<Amount> = Vec::new();
    let mut rates: Vec<(i32, Amount)> = Vec::new();
    for (index, line) in lower.iter().enumerate() {
        if !VAT_WORDS.iter().any(|word| contains_word(line, word)) {
            continue;
        }
        if VAT_ID_WORDS.iter().any(|word| line.contains(word)) {
            continue;
        }
        let rate = amounts[index]
            .iter()
            .filter(|amount| amount.percent)
            .find_map(|amount| rate_bp(*amount).map(|bp| (bp, *amount)));
        if let Some(rate) = rate {
            rates.push(rate);
        }
        // Money is printed with its cents. A digit group without them, on a
        // line that mentions tax, is a registration number or a table index —
        // never the tax.
        let stated: Vec<Amount> = amounts[index]
            .iter()
            .filter(|amount| !amount.percent && amount.fraction && amount.cents > 0)
            .filter(|amount| gross.is_none_or(|found| found.value != amount.cents))
            .copied()
            .collect();
        if let Some(tax) = pick_tax_on_line(&stated, rate.map(|(bp, _)| bp), gross) {
            taxes.push(tax);
        }
        // A rate on its own line, with the amount on the next one — a layout
        // PDFs produce when a table is flattened.
        if stated.is_empty()
            && rate.is_some()
            && index + 1 < chars.len()
            && !VAT_WORDS
                .iter()
                .any(|word| contains_word(&lower[index + 1], word))
            && !TOTAL_WORDS
                .iter()
                .any(|word| contains_word(&lower[index + 1], word))
            && let [only] = amounts[index + 1]
                .iter()
                .filter(|amount| !amount.percent && amount.fraction && amount.cents > 0)
                .filter(|amount| gross.is_none_or(|found| found.value != amount.cents))
                .copied()
                .collect::<Vec<_>>()[..]
        {
            taxes.push(only);
        }
    }

    let distinct: Vec<i32> = {
        let mut seen: Vec<i32> = rates.iter().map(|(bp, _)| *bp).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    let rate = match (distinct.as_slice(), rates.first()) {
        ([_single], Some((bp, amount))) => Some(Found::new(
            *bp,
            Confidence::High,
            Evidence::Text {
                line: amount.line,
                start: amount.start,
                end: amount.end,
            },
        )),
        // Several rates: the paper states no single one, so neither do we.
        _ => None,
    };

    let vat = match taxes.as_slice() {
        [] => None,
        [only] => Some(Found::new(
            only.cents,
            if rate.is_some() {
                Confidence::High
            } else {
                Confidence::Medium
            },
            only.evidence(),
        )),
        many => {
            let total: i64 = many.iter().map(|amount| amount.cents).sum();
            (total <= AMOUNT_MAX_CENTS).then(|| {
                Found::new(
                    total,
                    Confidence::Medium,
                    many.first()
                        .map_or(Evidence::Filename, |first| first.evidence()),
                )
            })
        }
    };
    (vat, rate)
}

/// Which of the amounts printed on a VAT line is the tax.
///
/// One amount is the tax. Several — the `19% 10,00 1,90 11,90` of a German VAT
/// table — are net, tax and gross, and the one that is consistent with the
/// rate is the tax. This chooses between numbers the receipt printed; it never
/// produces one (see the module header).
fn pick_tax_on_line(
    stated: &[Amount],
    rate_bp: Option<i32>,
    gross: Option<&Found<i64>>,
) -> Option<Amount> {
    match stated {
        [] => None,
        [only] => Some(*only),
        many => {
            if let (Some(bp), Some(gross)) = (rate_bp, gross)
                && bp > 0
                && let Some(expected) = tax_of_gross(gross.value, bp)
                && let Some(matching) = many
                    .iter()
                    .find(|amount| (amount.cents - expected).abs() <= 1)
            {
                return Some(*matching);
            }
            // Without a rate to check against, the tax is the smallest of the
            // numbers on the line: it is smaller than both the net and the
            // gross for every rate under 100%.
            many.iter().min_by_key(|amount| amount.cents).copied()
        }
    }
}

/// What the tax inside `gross` would be at `rate_bp`, for **comparison only**.
fn tax_of_gross(gross_cents: i64, rate_bp: i32) -> Option<i64> {
    let rate = i64::from(rate_bp);
    let numerator = gross_cents.checked_mul(rate)?;
    let denominator = 10_000_i64.checked_add(rate)?;
    // Round half away from zero, as every VAT calculation in the store does.
    Some((numerator + denominator / 2) / denominator)
}

/// A percentage token as basis points, when it is a plausible VAT rate.
fn rate_bp(amount: Amount) -> Option<i32> {
    // `parse_amount_cents` reads `19` as 1900 and `19,5` as 1950, which is
    // exactly basis points — a percentage is hundredths, like a euro.
    let bp = i32::try_from(amount.cents).ok()?;
    (0..=VAT_RATE_MAX_BP).contains(&bp).then_some(bp)
}

// ------------------------------------------------------------------ dates --

/// A date found in the text, and the characters it occupies.
#[derive(Debug, Clone, Copy)]
struct DateHit {
    line: usize,
    start: usize,
    end: usize,
    date: Date,
    /// Whether day and month could be read the other way round (`03/04/2026`).
    ambiguous: bool,
}

/// The words that label a date, in the languages we ship.
const DATE_WORDS: &[&str] = &["datum", "date", "dated"];

/// Month names in the four languages a European receipt is printed in, with
/// the abbreviations tills use and the accent-free spellings OCR produces.
const MONTH_NAMES: &[(&str, u8)] = &[
    ("january", 1),
    ("januar", 1),
    ("januari", 1),
    ("janvier", 1),
    ("jan", 1),
    ("february", 2),
    ("februar", 2),
    ("februari", 2),
    ("février", 2),
    ("fevrier", 2),
    ("feb", 2),
    ("fev", 2),
    ("march", 3),
    ("märz", 3),
    ("maerz", 3),
    ("maart", 3),
    ("mars", 3),
    ("mar", 3),
    ("mrz", 3),
    ("mrt", 3),
    ("april", 4),
    ("avril", 4),
    ("apr", 4),
    ("avr", 4),
    ("may", 5),
    ("mai", 5),
    ("mei", 5),
    ("june", 6),
    ("juni", 6),
    ("juin", 6),
    ("jun", 6),
    ("july", 7),
    ("juli", 7),
    ("juillet", 7),
    ("jul", 7),
    ("august", 8),
    ("augustus", 8),
    ("août", 8),
    ("aout", 8),
    ("aug", 8),
    ("september", 9),
    ("septembre", 9),
    ("sept", 9),
    ("sep", 9),
    ("october", 10),
    ("oktober", 10),
    ("octobre", 10),
    ("oct", 10),
    ("okt", 10),
    ("november", 11),
    ("novembre", 11),
    ("nov", 11),
    ("december", 12),
    ("dezember", 12),
    ("décembre", 12),
    ("decembre", 12),
    ("dec", 12),
    ("dez", 12),
];

/// Every date on every line, in the six spellings Europe writes them in:
/// `2026-03-14`, `14.03.2026`, `14.03.26`, `14/03/2026`, `14-03-2026` and
/// `14. März 2026`.
fn scan_dates(chars: &[Vec<char>]) -> Vec<DateHit> {
    let mut out = Vec::new();
    for (index, line) in chars.iter().enumerate() {
        let n = line.len();
        let mut i = 0;
        while i < n {
            if !line[i].is_ascii_digit() || (i > 0 && line[i - 1].is_ascii_digit()) {
                i += 1;
                continue;
            }
            if let Some(hit) = date_at(line, i, index) {
                i = hit.end;
                out.push(hit);
                continue;
            }
            i += 1;
        }
    }
    out
}

/// Reads a date starting at `start`, if one starts there.
fn date_at(line: &[char], start: usize, index: usize) -> Option<DateHit> {
    let (first, after_first) = digits_at(line, start)?;
    let hit = |end: usize, date: Date, ambiguous: bool| {
        Some(DateHit {
            line: index,
            start,
            end,
            date,
            ambiguous,
        })
    };
    // `2026-03-14` — ISO, and the only shape whose year comes first.
    if after_first - start == 4 {
        if let Some((month, day, end)) = two_more(line, after_first, '-') {
            let date = ymd(first, month, day)?;
            return hit(end, date, false);
        }
        return None;
    }
    if after_first - start > 2 {
        return None;
    }
    // `14.03.2026`, `14/03/26`, `14-03-2026` — day first, as Europe writes it.
    for separator in ['.', '/', '-'] {
        if let Some((month, year, end)) = two_more(line, after_first, separator) {
            let date = ymd(full_year(year), month, first)?;
            // With `/`, a first component of twelve or less could be an
            // American month. The European reading stands and the confidence
            // drops — the person confirms the date either way.
            let ambiguous = separator == '/' && first <= 12 && month <= 12;
            return hit(end, date, ambiguous);
        }
    }
    // `14. März 2026` / `14 mars 2026`.
    let mut cursor = after_first;
    if cursor < line.len() && line[cursor] == '.' {
        cursor += 1;
    }
    let before_gap = cursor;
    while cursor < line.len() && is_space_sep(line[cursor]) {
        cursor += 1;
    }
    if cursor == before_gap {
        return None;
    }
    let (month, after_month) = month_name_at(line, cursor)?;
    let mut cursor = after_month;
    while cursor < line.len() && is_space_sep(line[cursor]) {
        cursor += 1;
    }
    let (year, end) = digits_at(line, cursor)?;
    if !matches!(end - cursor, 2 | 4) {
        return None;
    }
    let date = ymd(full_year(year), month, first)?;
    hit(end, date, false)
}

/// The digits at `at` as a number, and the index after them.
fn digits_at(line: &[char], at: usize) -> Option<(u32, usize)> {
    let mut end = at;
    while end < line.len() && line[end].is_ascii_digit() {
        end += 1;
    }
    if end == at || end - at > 4 {
        return None;
    }
    let value: String = line[at..end].iter().collect();
    value.parse().ok().map(|value| (value, end))
}

/// `<sep>NN<sep>NNNN` after the first component of a numeric date.
fn two_more(line: &[char], at: usize, separator: char) -> Option<(u32, u32, usize)> {
    if at >= line.len() || line[at] != separator {
        return None;
    }
    let (second, after_second) = digits_at(line, at + 1)?;
    if !matches!(after_second - (at + 1), 1 | 2) {
        return None;
    }
    if after_second >= line.len() || line[after_second] != separator {
        return None;
    }
    let (third, end) = digits_at(line, after_second + 1)?;
    if !matches!(end - (after_second + 1), 2 | 4) {
        return None;
    }
    // A date is not the start of a longer number — but `2026-03-14.pdf` is a
    // date, so a separator only disqualifies it when digits follow it.
    if end < line.len()
        && (line[end].is_ascii_digit()
            || (is_group_sep(line[end]) && line.get(end + 1).is_some_and(char::is_ascii_digit)))
    {
        return None;
    }
    Some((second, third, end))
}

/// A month name at `at`, and the index after it. Case-insensitive, and it must
/// end at a word boundary so `jan` does not match inside `januari`.
fn month_name_at(line: &[char], at: usize) -> Option<(u32, usize)> {
    for (name, month) in MONTH_NAMES {
        let length = name.chars().count();
        if at + length > line.len() {
            continue;
        }
        let matches = name
            .chars()
            .zip(&line[at..at + length])
            .all(|(want, got)| want == got.to_lowercase().next().unwrap_or(*got));
        if matches && line.get(at + length).is_none_or(|c| !c.is_alphabetic()) {
            return Some((u32::from(*month), at + length));
        }
    }
    None
}

/// A two-digit year is this century: a receipt from 1998 is not being claimed.
fn full_year(year: u32) -> u32 {
    if year < 100 { 2000 + year } else { year }
}

/// A real calendar day, or nothing — `31.02.2026` is a misreading, not a date.
fn ymd(year: u32, month: u32, day: u32) -> Option<Date> {
    let year = i32::try_from(year).ok()?;
    let month = u8::try_from(month).ok()?;
    let day = u8::try_from(day).ok()?;
    Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
}

/// The day the receipt was issued.
///
/// A labelled date wins ("Datum", "Date"); otherwise the first plausible one,
/// which on a receipt is the one printed at the top. A date in the future is
/// not when the money was spent (it is a card expiry or a "valid until"), and
/// one more than [`DATE_MAX_AGE_YEARS`] old is not a date at all.
fn pick_date(dates: &[DateHit], lower: &[String], today: Date) -> Option<Found<Date>> {
    let oldest = today - Duration::days(DATE_MAX_AGE_YEARS * 366);
    let plausible: Vec<&DateHit> = dates
        .iter()
        .filter(|hit| hit.date <= today && hit.date >= oldest)
        .collect();
    let labelled = plausible.iter().find(|hit| {
        lower
            .get(hit.line)
            .is_some_and(|line| DATE_WORDS.iter().any(|word| contains_word(line, word)))
    });
    let (hit, confidence) = match labelled {
        Some(hit) => (*hit, Confidence::High),
        None => (*plausible.first()?, Confidence::Medium),
    };
    let confidence = if hit.ambiguous {
        weaker(confidence)
    } else {
        confidence
    };
    Some(Found::new(
        hit.date,
        confidence,
        Evidence::Text {
            line: hit.line,
            start: hit.start,
            end: hit.end,
        },
    ))
}

/// One step less sure.
fn weaker(confidence: Confidence) -> Confidence {
    match confidence {
        Confidence::High => Confidence::Medium,
        Confidence::Medium | Confidence::Low => Confidence::Low,
    }
}

/// `REWE_2026-03-14.pdf` and `receipt-20260314.jpg` both say a day. A phone
/// that names its photos `IMG_20260314_181203.jpg` says one too.
fn filename_date(filename: Option<&str>, today: Date) -> Option<Found<Date>> {
    let name = filename?;
    let chars: Vec<char> = name.chars().collect();
    let oldest = today - Duration::days(DATE_MAX_AGE_YEARS * 366);
    let plausible = |date: Date| (date <= today && date >= oldest).then_some(date);
    for hit in scan_dates(std::slice::from_ref(&chars)) {
        if let Some(date) = plausible(hit.date) {
            return Some(Found::new(date, Confidence::Low, Evidence::Filename));
        }
    }
    // `20260314`, the shape a camera writes.
    for start in 0..chars.len().saturating_sub(7) {
        if start > 0 && chars[start - 1].is_ascii_digit() {
            continue;
        }
        let run: String = chars[start..start + 8].iter().collect();
        if !run.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let year = run[0..4].parse().ok()?;
        let month = run[4..6].parse().ok()?;
        let day = run[6..8].parse().ok()?;
        if let Some(date) = ymd(year, month, day).and_then(plausible) {
            return Some(Found::new(date, Confidence::Low, Evidence::Filename));
        }
    }
    None
}

// --------------------------------------------------------------- merchant --

/// Suffixes that make a line a company rather than an address.
const LEGAL_FORMS: &[&str] = &[
    "gmbh",
    "mbh",
    " ag",
    "aktiengesellschaft",
    " kg",
    " ohg",
    " ug",
    "e.k.",
    "gbr",
    " b.v.",
    " bv",
    " n.v.",
    " nv",
    "bvba",
    "sarl",
    " sas",
    " sa",
    " srl",
    "s.r.l.",
    "s.p.a.",
    " spa",
    " ltd",
    "limited",
    " plc",
    " oy",
    " ab",
    " as",
    " aps",
    "sp. z o.o.",
    "d.o.o.",
    " kft",
    " sprl",
    " scs",
    " vof",
    " cv",
];

/// Lines that are the document's title, its contact details or its tax ids —
/// printed near the top, and never the name of who was paid.
const NOT_A_MERCHANT: &[&str] = &[
    "rechnung",
    "quittung",
    "kassenbon",
    "kassenzettel",
    "beleg",
    "invoice",
    "receipt",
    "facture",
    "ticket",
    "factuur",
    "bon ",
    "tel.",
    "tel:",
    "telefon",
    "phone",
    "www.",
    "http",
    "@",
    "ust-id",
    "ust-idnr",
    "steuernr",
    "steuer-nr",
    "vat no",
    "vat id",
    "btw-nr",
    "tva n",
    "iban",
    "bic",
    "kunde",
    "customer",
    "client",
    "datum",
    "date",
];

/// Who was paid.
///
/// The name is at the top of a receipt, and the first line that is not a
/// title, a date, an address of digits or a phone number is it. A legal form
/// in the line ("GmbH", "B.V.", "SARL") makes it certain; otherwise it is the
/// best of a small set of candidates and the person will correct it if the
/// till printed a slogan first.
fn pick_merchant(lines: &[String], lower: &[String], dates: &[DateHit]) -> Option<Found<String>> {
    let candidates: Vec<usize> = (0..lines.len().min(8))
        .filter(|index| is_merchant_line(&lines[*index], &lower[*index], dates, *index))
        .collect();
    let named = candidates
        .iter()
        .find(|index| LEGAL_FORMS.iter().any(|form| lower[**index].contains(form)));
    let (index, confidence) = match named {
        Some(index) => (*index, Confidence::High),
        None => (*candidates.first()?, Confidence::Medium),
    };
    let value: String = lines[index].chars().take(MERCHANT_MAX).collect();
    let end = value.chars().count();
    Some(Found::new(
        value,
        confidence,
        Evidence::Text {
            line: index,
            start: 0,
            end,
        },
    ))
}

/// Whether a line could be the name of who was paid.
fn is_merchant_line(line: &str, lower: &str, dates: &[DateHit], index: usize) -> bool {
    let letters = line.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 3 {
        return false;
    }
    // Mostly digits is an address, a till number or a barcode.
    if line.chars().filter(char::is_ascii_digit).count() > letters {
        return false;
    }
    if dates.iter().any(|hit| hit.line == index) {
        return false;
    }
    if NOT_A_MERCHANT.iter().any(|word| lower.contains(word)) {
        return false;
    }
    if TOTAL_WORDS
        .iter()
        .chain(VAT_WORDS)
        .any(|word| contains_word(lower, word))
    {
        return false;
    }
    true
}

/// What a file name says about who was paid: `REWE_2026-03-14.pdf` says
/// "REWE". A name that is only a date, a number or `scan` says nothing.
fn filename_merchant(filename: Option<&str>) -> Option<Found<String>> {
    let name = filename?;
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let words: Vec<&str> = stem
        .split(['_', '-', ' ', '.'])
        .filter(|word| {
            word.chars().any(char::is_alphabetic)
                && !word.chars().any(|c| c.is_ascii_digit())
                && !matches!(
                    word.to_lowercase().as_str(),
                    "scan" | "img" | "image" | "photo" | "foto" | "receipt" | "beleg" | "bon"
                )
        })
        .collect();
    if words.is_empty() {
        return None;
    }
    let value: String = words.join(" ").chars().take(MERCHANT_MAX).collect();
    (value.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .then(|| Found::new(value, Confidence::Low, Evidence::Filename))
}

// --------------------------------------------------------------- currency --

/// The symbols and codes a European receipt names its money with.
const CURRENCY_SYMBOLS: &[(char, &str)] = &[('€', "EUR"), ('$', "USD"), ('£', "GBP")];
const CURRENCY_CODES: &[&str] = &[
    "EUR", "USD", "GBP", "CHF", "PLN", "SEK", "DKK", "NOK", "CZK", "HUF", "RON", "BGN",
];

/// The currency the receipt names, preferring the one beside the total.
///
/// Absent is the common answer and the right one: a till in Munich prints no
/// currency at all, and the claim then takes the tenant's own.
fn pick_currency(chars: &[Vec<char>], gross: Option<&Found<i64>>) -> Option<Found<String>> {
    let gross_line = gross.and_then(|found| match found.evidence {
        Evidence::Text { line, .. } => Some(line),
        Evidence::Filename => None,
    });
    let order = gross_line
        .into_iter()
        .chain((0..chars.len()).filter(|index| Some(*index) != gross_line));
    for index in order {
        if let Some(found) = currency_on_line(&chars[index], index) {
            return Some(found);
        }
    }
    None
}

/// The first currency named on one line.
fn currency_on_line(line: &[char], index: usize) -> Option<Found<String>> {
    let mut best: Option<Found<String>> = None;
    for (at, c) in line.iter().enumerate() {
        if let Some((_, code)) = CURRENCY_SYMBOLS.iter().find(|(symbol, _)| symbol == c) {
            best = Some(Found::new(
                (*code).to_owned(),
                Confidence::Medium,
                Evidence::Text {
                    line: index,
                    start: at,
                    end: at + 1,
                },
            ));
            break;
        }
    }
    for code in CURRENCY_CODES {
        let length = code.chars().count();
        for at in 0..line.len().saturating_sub(length - 1) {
            let word: String = line[at..at + length].iter().collect();
            if !word.eq_ignore_ascii_case(code) {
                continue;
            }
            let before_ok = at == 0 || !line[at - 1].is_alphabetic();
            let after_ok = line.get(at + length).is_none_or(|c| !c.is_alphabetic());
            if before_ok && after_ok {
                // An explicit ISO code beats a symbol: it is unambiguous.
                return Some(Found::new(
                    (*code).to_owned(),
                    Confidence::High,
                    Evidence::Text {
                        line: index,
                        start: at,
                        end: at + length,
                    },
                ));
            }
        }
    }
    best
}

// ------------------------------------------------------------------ words --

/// Whether `haystack` (already lower case) contains `needle` as a word rather
/// than inside another one: "vat" is in "privat", and a private receipt is not
/// a tax line.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(at) = haystack[from..].find(needle) {
        let at = from + at;
        let before_ok = haystack[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphabetic());
        if before_ok {
            return true;
        }
        from = at + needle.len();
        if from >= haystack.len() {
            break;
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real day")
    }

    fn today() -> Date {
        day(2026, Month::March, 20)
    }

    fn read(text: &str) -> ParsedReceipt {
        PatternExtractor.extract(&ReceiptInput {
            text,
            filename: None,
            today: today(),
        })
    }

    fn read_named(text: &str, filename: &str) -> ParsedReceipt {
        PatternExtractor.extract(&ReceiptInput {
            text,
            filename: Some(filename),
            today: today(),
        })
    }

    #[test]
    fn a_german_till_roll_gives_up_all_four_fields() {
        let parsed = read(
            "REWE Markt GmbH\nHauptstr. 12\n80331 München\nDatum 14.03.2026\n\
             Milch 1,19\nBrot 2,49\nSUMME EUR 11,90\nMwSt 19% 1,90\n",
        );
        assert_eq!(parsed.merchant.as_ref().unwrap().value, "REWE Markt GmbH");
        assert_eq!(
            parsed.merchant.as_ref().unwrap().confidence,
            Confidence::High,
            "a legal form makes the name certain"
        );
        assert_eq!(parsed.spent_on.unwrap().value, day(2026, Month::March, 14));
        assert_eq!(parsed.gross_cents.as_ref().unwrap().value, 1190);
        assert_eq!(parsed.vat_cents.as_ref().unwrap().value, 190);
        assert_eq!(parsed.vat_rate_bp.as_ref().unwrap().value, 1900);
        assert_eq!(parsed.currency.as_ref().unwrap().value, "EUR");
    }

    #[test]
    fn the_vat_is_never_computed_from_the_rate_and_the_total() {
        let parsed = read("Café Central\n14.03.2026\nTotal 11,90\ninkl. 19% MwSt\n");
        assert_eq!(parsed.gross_cents.unwrap().value, 1190);
        assert_eq!(parsed.vat_rate_bp.unwrap().value, 1900);
        assert!(
            parsed.vat_cents.is_none(),
            "the receipt printed no tax amount, so neither do we"
        );
    }

    #[test]
    fn the_total_on_a_line_that_also_names_the_rate_is_not_read_as_the_tax() {
        let parsed = read("Kiosk\n14.03.2026\nZu zahlen 11,90 inkl. 19% MwSt\n");
        assert_eq!(parsed.gross_cents.unwrap().value, 1190);
        assert!(parsed.vat_cents.is_none());
    }

    #[test]
    fn a_vat_table_row_yields_the_tax_and_not_the_net_or_the_gross() {
        let parsed = read(
            "Bürobedarf Meyer GmbH\nDatum 02.03.2026\nGesamtbetrag 11,90\nMwSt 19% 10,00 1,90 11,90\n",
        );
        assert_eq!(parsed.gross_cents.as_ref().unwrap().value, 1190);
        assert_eq!(
            parsed.vat_cents.as_ref().unwrap().value,
            190,
            "the one of the three consistent with 19%"
        );
    }

    #[test]
    fn two_rates_yield_a_tax_total_and_no_single_rate() {
        let parsed = read(
            "Hotel Adler GmbH\nDatum 12.03.2026\nGesamtbetrag 214,00\n\
             MwSt 7% 12,15\nMwSt 19% 6,39\n",
        );
        assert_eq!(parsed.gross_cents.unwrap().value, 21_400);
        assert_eq!(parsed.vat_cents.unwrap().value, 1215 + 639);
        assert!(
            parsed.vat_rate_bp.is_none(),
            "the paper states two rates, so it states no single one"
        );
    }

    #[test]
    fn a_subtotal_is_never_the_total() {
        let parsed = read("Laden\n14.03.2026\nZwischensumme 10,00\nSUMME 11,90\n");
        assert_eq!(parsed.gross_cents.unwrap().value, 1190);
    }

    #[test]
    fn a_net_total_loses_to_the_gross_one() {
        let parsed = read("Werkstatt\n14.03.2026\nTotal netto 100,00\nTotal brutto 119,00\n");
        assert_eq!(parsed.gross_cents.unwrap().value, 11_900);
    }

    #[test]
    fn with_nothing_labelled_the_largest_decimal_amount_is_offered_at_low_confidence() {
        let parsed = read("Parkhaus Mitte\n14.03.2026\n2,50\n4,50\n");
        let gross = parsed.gross_cents.unwrap();
        assert_eq!(gross.value, 450);
        assert_eq!(gross.confidence, Confidence::Low);
    }

    #[test]
    fn digits_that_are_not_amounts_never_become_one() {
        // An order number, a postcode, a time and a date must not out-bid the
        // real total.
        let parsed = read("Kiosk\nBon-Nr. 4711\n80331 München\n14.03.2026 18:35\n1,90\n");
        assert_eq!(parsed.gross_cents.unwrap().value, 190);
    }

    #[test]
    fn every_spelling_of_a_date_reads_as_the_same_day() {
        let expected = day(2026, Month::March, 14);
        for text in [
            "Datum 2026-03-14",
            "Datum 14.03.2026",
            "Datum 14.03.26",
            "Datum 14/03/2026",
            "Datum 14-03-2026",
            "Datum 14. März 2026",
            "Date 14 mars 2026",
            "Datum 14 maart 2026",
        ] {
            let parsed = read(text);
            assert_eq!(
                parsed.spent_on.as_ref().map(|found| found.value),
                Some(expected),
                "{text}"
            );
        }
    }

    #[test]
    fn a_slash_date_that_could_be_read_the_other_way_round_says_so() {
        let parsed = read("Datum 03/04/2025\nTotal 10,00\n");
        let spent = parsed.spent_on.unwrap();
        assert_eq!(
            spent.value,
            day(2025, Month::April, 3),
            "Europe writes the day first"
        );
        assert_eq!(
            spent.confidence,
            Confidence::Medium,
            "labelled, but the day and month could swap"
        );
    }

    #[test]
    fn a_date_that_has_not_happened_is_not_when_the_money_was_spent() {
        let parsed = read("Karte gültig bis 12/2030\nDatum 31.12.2029\nTotal 10,00\n");
        assert!(parsed.spent_on.is_none());
    }

    #[test]
    fn a_day_that_is_not_a_day_is_not_read() {
        let parsed = read("Datum 31.02.2026\nTotal 10,00\n");
        assert!(parsed.spent_on.is_none());
    }

    #[test]
    fn the_file_name_answers_what_the_paper_does_not() {
        // A photograph of a till roll has no text layer at all.
        let parsed = read_named("", "REWE_2026-03-14.pdf");
        assert_eq!(
            parsed.spent_on.as_ref().unwrap().value,
            day(2026, Month::March, 14)
        );
        assert_eq!(
            parsed.spent_on.as_ref().unwrap().evidence,
            Evidence::Filename
        );
        assert_eq!(parsed.merchant.as_ref().unwrap().value, "REWE");
        assert_eq!(
            parsed.merchant.as_ref().unwrap().confidence,
            Confidence::Low
        );
    }

    #[test]
    fn a_camera_file_name_gives_up_its_day() {
        let parsed = read_named("", "IMG_20260314_181203.jpg");
        assert_eq!(parsed.spent_on.unwrap().value, day(2026, Month::March, 14));
        assert!(parsed.merchant.is_none(), "IMG is not the name of a shop");
    }

    #[test]
    fn an_unreadable_receipt_is_empty_and_not_an_error() {
        let parsed = read("");
        assert!(!parsed.found_anything());
        assert!(parsed.lines.is_empty());
        let scribble = read("~~~ ### ~~~\n");
        assert!(!scribble.found_anything());
    }

    #[test]
    fn the_evidence_points_at_the_characters_the_value_came_from() {
        let parsed = read("Kiosk\nDatum 14.03.2026\nSUMME 11,90\n");
        let Evidence::Text { line, start, end } = parsed.gross_cents.unwrap().evidence else {
            panic!("the total came from the text");
        };
        assert_eq!(&parsed.lines[line][start..end], "11,90");
    }

    #[test]
    fn a_receipt_longer_than_we_read_is_cut_and_still_answered() {
        let mut text = String::from("Grosser Laden GmbH\nDatum 14.03.2026\n");
        for line in 0..RECEIPT_LINES_MAX * 2 {
            text.push_str(&format!("Artikel {line} 1,00\n"));
        }
        let parsed = read(&text);
        assert_eq!(parsed.lines.len(), RECEIPT_LINES_MAX);
        assert_eq!(parsed.merchant.unwrap().value, "Grosser Laden GmbH");
    }

    #[test]
    fn a_word_inside_another_word_is_not_a_keyword() {
        assert!(contains_word("mwst 19%", "mwst"));
        assert!(!contains_word("privatentnahme", "vat"));
        assert!(contains_word("summe brutto", "summe"));
        assert!(!contains_word("bezugssumme", "summe"));
    }

    #[test]
    fn the_seam_takes_a_second_implementation() {
        struct AlwaysNothing;
        impl ReceiptExtractor for AlwaysNothing {
            fn extract(&self, _input: &ReceiptInput<'_>) -> ParsedReceipt {
                ParsedReceipt::default()
            }
        }
        let extractors: Vec<&dyn ReceiptExtractor> = vec![default_extractor(), &AlwaysNothing];
        let input = ReceiptInput {
            text: "Kiosk\nSUMME 11,90\n",
            filename: None,
            today: today(),
        };
        let answers: Vec<bool> = extractors
            .iter()
            .map(|extractor| extractor.extract(&input).found_anything())
            .collect();
        assert_eq!(answers, vec![true, false]);
    }
}
