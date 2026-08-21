//! Campaign blocks compiled to the plain-text alternative (alo Campaigns,
//! ADR 0044, wave C3.3).
//!
//! Queue item C3.3: *a plain-text alternative from the same blocks, assembled
//! as `multipart/alternative`. **Not optional** — a campaign with no text part
//! is scored as spam by filters older than this project.* This module is the
//! first half of that sentence; [`crate::campaign_mime`] is the second.
//!
//! It is a **sibling** of [`crate::campaign_html`] rather than a second mode of
//! it: the two read the same [`CampaignBlock`] values and answer two different
//! questions — *how does this letter look* and *what does this letter say*. One
//! function that did both would decide the second while thinking about the
//! first, and the text part is the one a spam filter reads.
//!
//! ## What a text part is actually for
//!
//! Not a courtesy for people with old clients. Three separate audiences, and
//! only the first of them is a person:
//!
//! - **The spam filter.** A `text/html`-only mail is a decades-old signal, and
//!   the score is applied before any human sees the letter.
//! - **The reader whose client shows text**, by preference or by policy — a
//!   terminal client, a screen reader set to prefer text, a phone in a low-data
//!   mode, an archive that stripped the HTML on the way in.
//! - **The search index and the quote.** When somebody replies to a campaign,
//!   what is quoted back is usually this part, so a table that arrived as one
//!   long run-on line is what the sender sees in the reply.
//!
//! ## The rules, and what each one costs
//!
//! - **Wrapped at [`CAMPAIGN_TEXT_WIDTH_COLS`] columns**, on whitespace only. A
//!   word longer than the column — a URL, almost always — is left whole on its
//!   own line: a broken link is worse than a ragged margin, and a reader who
//!   cannot click one cannot copy one either.
//! - **A run of spaces the writer typed collapses to one**, exactly as it does
//!   in HTML. The two parts are alternatives of *the same letter*, so where
//!   HTML has no choice the text part follows it rather than quietly saying
//!   something the other part does not.
//! - **Headings are underlined** — `=` for level 1, `-` for level 2 — rather
//!   than upper-cased. Upper-casing is shouting in every language that has
//!   cases and is nothing at all in the ones that do not.
//! - **A table becomes something a monospace reader can follow**, which is the
//!   real work of this item and gets its own section below.
//! - **Code is never wrapped.** Wrapping a sample changes it, and a reader who
//!   pastes a wrapped command gets an error rather than a result. It is
//!   indented by [`CODE_INDENT`] spaces under a `[language]` label, so a block
//!   is visible as a block; a uniform indent is removed by a dedent, where a
//!   wrap cannot be undone at all.
//! - **Control characters are dropped**, the same rule [`crate::campaign_html`]
//!   applies and for a related reason: a NUL in a message body is not something
//!   a mail store, an index or a terminal has any good answer for.
//! - **No trailing whitespace, anywhere.** A trailing space is invisible to the
//!   writer, and in the quoted-printable encoding that carries this part it
//!   becomes a visible `=20` — see [`crate::campaign_mime`].
//!
//! ## A table, in a medium with one font
//!
//! An aligned table needs the whole row to fit the column width, and a price
//! list with three columns of European product names usually does not. So there
//! are two renderings and the width decides which:
//!
//! - **It fits**: columns padded to a common width, two spaces between them, a
//!   rule of dashes under the header. This is the readable one and it is
//!   preferred whenever it is possible.
//! - **It does not fit**: each row becomes a short block of `Header: value`
//!   lines, one per column, with a blank line between rows. Nothing is
//!   truncated and nothing is dropped — a table that was squeezed into the
//!   column would lose the very numbers it exists to carry.
//!
//! Padding counts **characters, not display cells**, so a column of CJK text
//! aligns loosely where a Latin one aligns exactly. That is the honest limit of
//! doing this without a font: it costs alignment, never a value, and the
//! fallback above is what a table reaching that width lands in anyway.
//!
//! ## What this module deliberately does not do
//!
//! - **It carries no preheader.** A preheader is a device for hiding preview
//!   text from a reader who can see the letter; in a text part there is nothing
//!   to hide behind, so it would arrive as a duplicated first line.
//! - **It does not compose the unsubscribe footer**, though it now renders one
//!   (C2.5). The link is per recipient and the words are the recipient's
//!   language, so both arrive as an [`UnsubscribeInvitation`] the caller built.
//!   What this module decides is only how it is set: below the letter, behind
//!   the conventional `--` separator, with the URL written out in full — in a
//!   text part there is nowhere for an address to hide, and a reader has to be
//!   able to copy it.
//! - **It does not personalise**, exactly as the HTML part does not.
//!   [`crate::campaign_merge`] (C3.4) resolves the body once, for one
//!   recipient, and both renderers are handed the same resolved words — a rule
//!   applied here and again there would be a rule that could disagree with
//!   itself between two parts of one mail.
//! - **It emits no string of our own in any language.** The underlines are
//!   punctuation and the labels are the writer's own header cells, so a text
//!   part is in whatever language the letter was typed in and there is nothing
//!   here to translate. The unsubscribe footer keeps that promise rather than
//!   breaking it: its words come in on the invitation, already in the
//!   recipient's language.

use crate::campaign_content::{
    CampaignBlock, CampaignContent, CodeBlock, HeadingBlock, ParagraphBlock, TableBlock,
};
use crate::campaign_unsubscribe_link::UnsubscribeInvitation;
use crate::error::Result;

/// The column the text part wraps at.
///
/// 72 rather than 78: a reply quotes this part with `> ` in front of every
/// line, and a letter written to the edge of the envelope arrives at the sender
/// re-wrapped into ragged halves.
pub const CAMPAIGN_TEXT_WIDTH_COLS: usize = 72;

/// How far a code sample is indented under its label.
const CODE_INDENT: usize = 4;

/// The spacing between the columns of an aligned table.
const COLUMN_GAP: &str = "  ";

/// Compiles a campaign body into the `text/plain` part of a mail.
///
/// The subject and the preheader are deliberately not taken: the subject is a
/// header of the message, and a preheader has no meaning in a part that hides
/// nothing (see the module docs).
///
/// # Errors
/// [`crate::error::StoreError::Validation`] when the body would not pass the
/// write gate — a `schema_version` this build does not speak, a ragged table, a
/// blank heading, a duplicate block id. [`CampaignContent`]'s fields are
/// public, so a value can reach here without ever having passed that gate, and
/// rendering one would produce a letter no writer could have saved. The HTML
/// renderer refuses the same bodies for the same reason, so the two parts of a
/// mail can never disagree about whether the letter was legal.
pub fn render_campaign_text(
    content: &CampaignContent,
    unsubscribe: &UnsubscribeInvitation,
) -> Result<String> {
    content.validate()?;
    // Refused before a line is rendered, so a letter that cannot be left never
    // reaches half-built. The HTML part applies the same rule.
    unsubscribe.validated()?;

    let mut lines: Vec<String> = Vec::new();
    for (index, block) in content.blocks.iter().enumerate() {
        if index > 0 {
            // One blank line between blocks. An empty paragraph contributes no
            // lines of its own, so a writer's deliberate blank line arrives as
            // a second blank line rather than being swallowed.
            lines.push(String::new());
        }
        block_lines(block, &mut lines);
    }

    // A blank line at the end of a letter is nothing a reader can see, and it
    // is whitespace immediately before a MIME boundary. Both reasons point the
    // same way.
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }

    // The way out (C2.5), below the letter behind the conventional `-- `
    // separator — the plain-text equivalent of the footer sitting under the
    // card rather than inside it.
    //
    // The URL is written out in full rather than hidden behind words: in a text
    // part there is nowhere for it to hide, and a reader has to be able to copy
    // it. The words come first so the line reads as a sentence in the
    // recipient's own language, with the address after it.
    if !lines.is_empty() {
        lines.push(String::new());
        lines.push("--".to_owned());
    }
    lines.push(format!(
        "{}: {}",
        unsubscribe.link_text.trim(),
        unsubscribe.page_url.trim()
    ));

    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// One block. A total match over a closed vocabulary, exactly as the HTML
/// renderer's is: a fifth block added to the model is a compile error here
/// rather than a block that silently vanishes from the text part.
fn block_lines(block: &CampaignBlock, out: &mut Vec<String>) {
    match block {
        CampaignBlock::Heading(heading) => heading_lines(heading, out),
        CampaignBlock::Paragraph(paragraph) => paragraph_lines(paragraph, out),
        CampaignBlock::Table(table) => table_lines(table, out),
        CampaignBlock::Code(code) => code_lines(code, out),
    }
}

/// A heading and its underline. The underline is as long as the longest line of
/// the heading, so a heading that wrapped is still underlined as a whole.
fn heading_lines(heading: &HeadingBlock, out: &mut Vec<String>) {
    let wrapped = wrap(&one_line(&heading.text), CAMPAIGN_TEXT_WIDTH_COLS);
    let rule = if heading.level == 1 { '=' } else { '-' };
    let width = wrapped
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    out.extend(wrapped);
    out.push(rule.to_string().repeat(width));
}

/// Prose, wrapped line by line so the writer's own breaks survive.
///
/// Wrapping the paragraph as one run would turn a two-line address into a
/// single flowing sentence, which is the failure people notice immediately.
fn paragraph_lines(paragraph: &ParagraphBlock, out: &mut Vec<String>) {
    for line in text_lines(&paragraph.text) {
        out.extend(wrap(&line, CAMPAIGN_TEXT_WIDTH_COLS));
    }
}

/// The writer's table, in whichever of the two renderings the width allows.
/// See the module docs — this is the item's real work.
fn table_lines(table: &TableBlock, out: &mut Vec<String>) {
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| one_line(cell)).collect())
        .collect();
    // Unreachable after `validate`, which refuses a table with no rows; taken
    // rather than indexed so a renderer can never panic on a body.
    let Some(header) = rows.first() else {
        return;
    };

    let widths: Vec<usize> = (0..header.len())
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let drawn = widths.iter().sum::<usize>() + COLUMN_GAP.len() * widths.len().saturating_sub(1);

    if drawn <= CAMPAIGN_TEXT_WIDTH_COLS {
        aligned_table_lines(&rows, &widths, out);
    } else {
        labelled_table_lines(&rows, header, out);
    }
}

/// The table as columns, when the whole row fits.
fn aligned_table_lines(rows: &[Vec<String>], widths: &[usize], out: &mut Vec<String>) {
    for (index, row) in rows.iter().enumerate() {
        let padded: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let width = widths.get(column).copied().unwrap_or(0);
                let padding = width.saturating_sub(cell.chars().count());
                format!("{cell}{}", " ".repeat(padding))
            })
            .collect();
        // Padding on the last column would be trailing whitespace.
        out.push(padded.join(COLUMN_GAP).trim_end().to_owned());
        if index == 0 {
            let rule: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
            out.push(rule.join(COLUMN_GAP).trim_end().to_owned());
        }
    }
}

/// The table as one short block per row, when the columns cannot fit.
///
/// Truncating instead would lose the numbers the table exists to carry, and a
/// horizontally scrolling table does not exist in a medium with one font.
fn labelled_table_lines(rows: &[Vec<String>], header: &[String], out: &mut Vec<String>) {
    if rows.len() == 1 {
        // A header and nothing under it: the column names are all there is to
        // say, and they are still the writer's words.
        for cell in header {
            out.extend(wrap(cell, CAMPAIGN_TEXT_WIDTH_COLS));
        }
        return;
    }
    for (index, row) in rows.iter().skip(1).enumerate() {
        if index > 0 {
            out.push(String::new());
        }
        for (column, cell) in row.iter().enumerate() {
            let label = header.get(column).map(String::as_str).unwrap_or("").trim();
            let line = if label.is_empty() {
                cell.clone()
            } else {
                format!("{label}: {cell}")
            };
            out.extend(wrap_hanging(&line, CAMPAIGN_TEXT_WIDTH_COLS, 2));
        }
    }
}

/// A code sample: a bracketed language label, then the sample indented and
/// otherwise untouched.
///
/// The brackets are not decoration — `bash:` reads as prose and `c++:` reads as
/// a typo, while `[bash]` cannot be mistaken for either. A tab becomes
/// [`CODE_INDENT`] spaces so the sample is spaced the same way it is in the
/// HTML part, which rebuilds tabs for the same reason. The **end** of a line is
/// trimmed even here: whitespace nobody can see is not part of a sample, and it
/// is the one thing in a code block that the encoding would make visible.
fn code_lines(code: &CodeBlock, out: &mut Vec<String>) {
    out.push(format!("[{}]", code.language.trim()));
    let indent = " ".repeat(CODE_INDENT);
    for line in text_lines(&code.code.replace('\t', &indent)) {
        let line = line.trim_end();
        if line.is_empty() {
            out.push(String::new());
        } else {
            out.push(format!("{indent}{line}"));
        }
    }
}

/// Splits a block's text into the lines the writer wrote.
///
/// `\r\n` folds to `\n` first — a body pasted from a Windows editor would
/// otherwise arrive double-spaced — and a trailing newline is dropped, because
/// it is a line the writer did not mean to send. Text that is empty once
/// cleaned yields no lines at all, which is what makes an empty paragraph a
/// blank line rather than three.
fn text_lines(value: &str) -> Vec<String> {
    let cleaned = clean(value);
    let text = cleaned.strip_suffix('\n').unwrap_or(&cleaned);
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n').map(str::to_owned).collect()
}

/// Folds a value onto one line, collapsing every run of whitespace — for a
/// heading, a table cell and anything else whose shape is a single line.
fn one_line(value: &str) -> String {
    clean(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalises line endings and drops the characters that do not belong in a
/// message body. `\n` and `\t` survive; the callers that care about them say so.
fn clean(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| matches!(c, '\n' | '\t') || !c.is_control())
        .collect()
}

/// Greedy wrapping on whitespace. A word wider than the column is left whole —
/// see the module docs on URLs.
fn wrap(line: &str, width: usize) -> Vec<String> {
    wrap_hanging(line, width, 0)
}

/// Wrapping where every line after the first carries `indent` spaces, so a
/// wrapped `Header: value` still reads as one item.
fn wrap_hanging(line: &str, width: usize, indent: usize) -> Vec<String> {
    let hang = " ".repeat(indent);
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        if current.is_empty() {
            if out.is_empty() {
                current.push_str(word);
            } else {
                current.push_str(&hang);
                current.push_str(word);
            }
            continue;
        }
        if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push(std::mem::take(&mut current));
            current.push_str(&hang);
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        // A line of nothing but whitespace is still a line the writer left.
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::error::StoreError;
    use serde_json::json;

    fn body(blocks: serde_json::Value) -> CampaignContent {
        CampaignContent::from_value(json!({ "schema_version": 1, "blocks": blocks }))
            .expect("the fixture body is valid")
    }

    /// The whole part, footer and all.
    fn render_whole(content: &CampaignContent) -> String {
        render_campaign_text(content, &crate::campaign_unsubscribe_link::an_invitation())
            .expect("a validated body renders")
    }

    /// The letter *without* the unsubscribe footer.
    ///
    /// Every test below was written to hold something about how the **writer's**
    /// blocks are set — wrapping, tables, blank lines, trailing whitespace — and
    /// the footer is not the writer's. Stripping it keeps each of those claims
    /// exactly what it was, instead of restating eight expected strings around a
    /// line none of them is about.
    /// [`tests::the_part_carries_the_way_out`] holds the footer itself, once.
    fn render(content: &CampaignContent) -> String {
        let whole = render_whole(content);
        // Cut at the separator and restore the letter's own trailing newline,
        // so what comes back is byte-for-byte what this renderer produced
        // before the footer existed. Anything looser would quietly relax the
        // trailing-whitespace and blank-line claims these tests exist to hold.
        match whole.find("\n\n--\n") {
            Some(cut) => format!("{}\n", &whole[..cut]),
            // No separator means there was no letter above it.
            None => String::new(),
        }
    }

    fn letter() -> CampaignContent {
        body(json!([
            { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices" },
            { "type": "paragraph", "id": "p1", "text": "Everything below is per litre." },
            { "type": "table", "id": "t1", "rows": [["Product", "Price"], ["Oil", "€12"]] },
            { "type": "code", "id": "c1", "code": "curl https://alo", "language": "bash" },
        ]))
    }

    /// The property every golden file rests on, and the same one the HTML
    /// renderer carries: nothing in the output comes from a clock, a random
    /// source or the iteration order of a hash map.
    #[test]
    fn the_same_blocks_produce_the_same_text_every_time() {
        let content = letter();
        assert_eq!(render(&content), render(&content));
    }

    /// The text part is text. Not stripped HTML, not escaped HTML — the
    /// characters the writer typed, which is why a `&amp;` they wrote stays a
    /// `&amp;` here while the HTML part carries `&amp;amp;`.
    #[test]
    fn the_writers_characters_arrive_as_themselves_and_not_as_entities() {
        let content = body(json!([
            { "type": "heading", "id": "h1", "level": 2, "text": "<script>alert('x')</script> & Co" },
            { "type": "paragraph", "id": "p1", "text": "She said \"it's <b>fine</b>\" &amp; left." },
        ]));
        let text = render(&content);
        assert!(text.contains("<script>alert('x')</script> & Co"), "{text}");
        assert!(text.contains("\"it's <b>fine</b>\" &amp; left."), "{text}");
        assert!(!text.contains("&lt;"), "an HTML escape leaked in: {text}");
        assert!(!text.contains("&#160;"), "an HTML entity leaked in: {text}");
        assert!(!text.contains("<br />"), "markup leaked in: {text}");
    }

    /// A heading is underlined rather than shouted, and the rule is as long as
    /// the heading it belongs to.
    #[test]
    fn a_heading_is_underlined_to_its_own_width() {
        let content = body(json!([
            { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices" },
            { "type": "heading", "id": "h2", "level": 2, "text": "Bestellen" },
        ]));
        let text = render(&content);
        assert!(text.contains("Spring prices\n=============\n"), "{text}");
        assert!(text.contains("Bestellen\n---------"), "{text}");
        // Counted in characters, so an accented heading is not under-ruled.
        let accented = render(&body(json!([
            { "type": "heading", "id": "h1", "level": 2, "text": "Genève" },
        ])));
        assert!(accented.starts_with("Genève\n------\n"), "{accented}");
    }

    /// A table that fits is drawn as columns, and the header keeps its rule.
    #[test]
    fn a_table_that_fits_the_column_is_drawn_as_one() {
        let content = body(json!([
            { "type": "table", "id": "t1", "rows": [
                ["Product", "Prijs"],
                ["Olijfolie", "12,50 €"],
                ["Zonnebloemolie", "6,00 €"],
            ]},
        ]));
        let text = render(&content);
        assert_eq!(
            text,
            "Product         Prijs\n\
             --------------  -------\n\
             Olijfolie       12,50 €\n\
             Zonnebloemolie  6,00 €\n",
            "{text}"
        );
        // The padding stops at the last column: a padded row would carry
        // invisible trailing spaces into the encoder.
        for line in text.lines() {
            assert_eq!(line, line.trim_end(), "a padded line ends in whitespace");
        }
    }

    /// A table too wide to draw becomes one block per row rather than a
    /// truncated table — the numbers are the reason the table is in the letter.
    #[test]
    fn a_table_too_wide_to_draw_becomes_one_block_per_row() {
        let long = "Olijfolie extra vierge uit de eerste koude persing";
        let content = body(json!([
            { "type": "table", "id": "t1", "rows": [
                ["Product", "Omschrijving", "Prijs"],
                ["Olijfolie", long, "12,50 €"],
                ["Zonnebloemolie", "Geraffineerd, per liter", "6,00 €"],
            ]},
        ]));
        let text = render(&content);
        assert!(text.contains("Product: Olijfolie\n"), "{text}");
        assert!(text.contains(&format!("Omschrijving: {long}")), "{text}");
        assert!(
            text.contains("Prijs: 12,50 €\n\nProduct: Zonnebloemolie"),
            "{text}"
        );
        // Nothing was dropped and nothing was cut short.
        assert!(text.contains("Geraffineerd, per liter"), "{text}");
        assert!(!text.contains('…'), "a value was truncated: {text}");
    }

    /// The wrap is on whitespace only. A URL that does not fit is left whole,
    /// because a broken link cannot be clicked or copied.
    #[test]
    fn a_long_url_is_left_whole_rather_than_wrapped_into_two_dead_halves() {
        let url = "https://alo.test/campaigns/spring-prices-2026/orders?utm_source=newsletter&reference=NL-2026-0042";
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": format!("Bestel hier: {url} — tot maandag.") },
        ]));
        let text = render(&content);
        assert!(
            text.lines().any(|line| line == url),
            "the URL must sit alone and whole: {text}"
        );
        for line in text.lines() {
            if line == url {
                continue;
            }
            assert!(
                line.chars().count() <= CAMPAIGN_TEXT_WIDTH_COLS,
                "a line ran past the column: {line:?}"
            );
        }
    }

    /// The writer's own line breaks are meaning, and a Windows paste must not
    /// double them.
    #[test]
    fn a_writers_line_breaks_survive_and_are_not_doubled() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Met vriendelijke groet,\r\nNordwind & Co\n" },
        ]));
        let text = render(&content);
        assert_eq!(text, "Met vriendelijke groet,\nNordwind & Co\n", "{text}");
        assert!(!text.contains('\r'), "a carriage return survived: {text}");
    }

    /// An empty paragraph is a blank line the writer put there on purpose — the
    /// same decision the HTML renderer makes with `&#160;`, in the medium where
    /// a blank line costs nothing to draw.
    #[test]
    fn an_empty_paragraph_is_one_blank_line_and_not_three() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Above" },
            { "type": "paragraph", "id": "p2", "text": "" },
            { "type": "paragraph", "id": "p3", "text": "Below" },
        ]));
        assert_eq!(render(&content), "Above\n\n\nBelow\n");
    }

    /// Code is indented, labelled and otherwise untouched: a wrapped command is
    /// a command that fails when it is pasted.
    #[test]
    fn code_keeps_its_own_shape_and_is_never_wrapped() {
        let long = "curl https://api.alo.test/orders -H 'Accept: application/json' -d '{\"sku\": \"olive-1l\"}'";
        let content = body(json!([
            { "type": "code", "id": "c1", "code": format!("fn main() {{\n\tlet a = 1;  // one\n}}\n{long}"), "language": "rust" },
        ]));
        let text = render(&content);
        assert!(text.starts_with("[rust]\n"), "{text}");
        assert!(text.contains("\n    fn main() {\n"), "{text}");
        // The tab became four spaces, on top of the block's own indent.
        assert!(text.contains("\n        let a = 1;  // one\n"), "{text}");
        assert!(
            text.lines().any(|line| line == format!("    {long}")),
            "a code line was wrapped: {text}"
        );
    }

    /// Whitespace at the end of a line is invisible to the writer and becomes a
    /// visible `=20` in the encoding that carries this part.
    #[test]
    fn no_line_of_a_letter_ends_in_whitespace() {
        let content = body(json!([
            { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices  " },
            { "type": "paragraph", "id": "p1", "text": "Trailing   \nand   spaced   out" },
            { "type": "table", "id": "t1", "rows": [["A", "B"], ["long value here", ""]] },
            { "type": "code", "id": "c1", "code": "one\n   \ntwo   ", "language": "sh" },
            { "type": "paragraph", "id": "p2", "text": "" },
        ]));
        let text = render(&content);
        for line in text.lines() {
            assert_eq!(line, line.trim_end(), "a line ends in whitespace: {line:?}");
        }
        // And a run of spaces inside a line collapses, exactly as it does in
        // the HTML part — the two parts are alternatives of the same letter.
        assert!(text.contains("and spaced out"), "{text}");
        // The trailing empty paragraph left no blank line at the end.
        assert!(text.ends_with("two\n"), "{text}");
    }

    /// A control character is not something a mail store, an index or a
    /// terminal has a good answer for.
    #[test]
    fn a_control_character_is_dropped_rather_than_carried_into_the_part() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "before\u{0}after\u{7}" },
        ]));
        let text = render(&content);
        assert_eq!(text, "beforeafter\n", "{text}");
    }

    /// Accented text and a currency symbol are the ordinary case in a European
    /// product; the encoding that carries them is [`crate::campaign_mime`]'s
    /// problem, and this part holds them as themselves.
    #[test]
    fn european_text_travels_as_itself() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Prijzen per liter — Genève, 12 €" },
        ]));
        assert_eq!(render(&content), "Prijzen per liter — Genève, 12 €\n");
    }

    /// The write gate is the same gate for both parts, so one mail can never
    /// have a legal half and an illegal one.
    #[test]
    fn a_body_that_never_passed_the_write_gate_is_refused_rather_than_rendered() {
        let ragged = CampaignContent {
            schema_version: 1,
            blocks: vec![CampaignBlock::Table(TableBlock {
                id: "t1".to_owned(),
                rows: vec![vec!["a".to_owned(), "b".to_owned()], vec!["a".to_owned()]],
            })],
        };
        match render_campaign_text(&ragged, &crate::campaign_unsubscribe_link::an_invitation()) {
            Err(StoreError::Validation(detail)) => assert!(detail.contains("columns"), "{detail}"),
            other => panic!("expected a refusal, got {other:?}"),
        }

        let newer = CampaignContent {
            schema_version: 2,
            blocks: Vec::new(),
        };
        match render_campaign_text(&newer, &crate::campaign_unsubscribe_link::an_invitation()) {
            Err(StoreError::Validation(detail)) => {
                assert!(detail.contains("schema_version"), "{detail}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// An empty draft is a legitimate campaign, and its text part is empty
    /// rather than a stray blank line.
    #[test]
    fn an_empty_campaign_has_an_empty_text_part() {
        assert_eq!(render(&CampaignContent::empty()), "");
        // The composer's own starting state — one empty paragraph — likewise.
        let starter = body(json!([{ "type": "paragraph", "id": "b1", "text": "" }]));
        assert_eq!(render(&starter), "");
    }

    /// The whole letter, as a shape rather than as fragments: this is what the
    /// golden file pins and what a reader of the reply will see quoted.
    #[test]
    fn a_whole_letter_reads_as_a_letter() {
        assert_eq!(
            render(&letter()),
            "Spring prices\n\
             =============\n\
             \n\
             Everything below is per litre.\n\
             \n\
             Product  Price\n\
             -------  -----\n\
             Oil      €12\n\
             \n\
             [bash]\n\
             \x20   curl https://alo\n",
        );
    }
    #[test]
    fn the_part_carries_the_way_out() {
        // C2.5: the visible link, in every part, for every recipient whose
        // client draws no Unsubscribe button of its own.
        let invitation = crate::campaign_unsubscribe_link::an_invitation();
        let whole = render_whole(&letter());

        assert!(
            whole.contains(&invitation.page_url),
            "the address is written out in full — a text part has nowhere to              hide a link, and a reader has to be able to copy it: {whole}"
        );
        assert!(
            whole.contains(&invitation.link_text),
            "and the words are the caller's, in the recipient's language: {whole}"
        );
        // Below the letter, behind the conventional separator, rather than
        // interrupting the writer's last block.
        let footer_at = whole.find(&invitation.link_text).expect("a footer");
        assert!(
            whole[..footer_at].contains(
                "
--
"
            ),
            "the footer is set off from the letter: {whole}"
        );
    }

    #[test]
    fn a_letter_with_nothing_in_it_still_says_how_to_leave() {
        // An empty campaign is a legal record — a draft somebody is still
        // writing — so this renders rather than refusing. What it must not do
        // is render a part with no way out of it.
        let whole = render_whole(&CampaignContent::empty());
        let invitation = crate::campaign_unsubscribe_link::an_invitation();
        assert!(whole.contains(&invitation.page_url), "{whole}");
        assert!(
            !whole.contains("--"),
            "with no letter above it there is nothing to separate from: {whole}"
        );
    }
}
