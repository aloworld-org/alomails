//! The studio's rich text, made safe for the printed page and flattened for
//! the PDF.
//!
//! A quotation's content blocks are written in a browser's `contentEditable`
//! and stored as the HTML it produced. The studio sanitises that HTML before
//! it renders it (`web/src/billing/quote-studio/richText.ts`), and this module
//! is the **same allow-list** applied on the server, because a printed page is
//! served to a browser too and the server does not trust what a client saved.
//!
//! The rule is simple and total: only the tags in the list survive, with
//! **no attributes at all**; every other tag is dropped and its text kept;
//! every character of text goes through the page's escaper. Nothing that was
//! stored can become markup the list does not name.

use crate::billing_print::esc;

/// What an inline field (a heading, a list item, a table heading) may carry.
const INLINE_TAGS: &[&str] = &["b", "em", "i", "strong"];

/// What a paragraph field may carry: the inline tags plus the block structure
/// the studio's editor produces.
const RICH_TAGS: &[&str] = &[
    "b", "br", "em", "h1", "h2", "h3", "i", "li", "ol", "p", "strong", "ul",
];

/// Tags after which a flattened text starts a new line.
const LINE_BREAKING: &[&str] = &["br", "h1", "h2", "h3", "li", "ol", "p", "ul"];

#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    Text(&'a str),
    Open(String),
    Close(String),
}

/// Splits HTML into text runs and tags. Attributes are read past and thrown
/// away; a `<` that does not begin a tag is text.
fn tokens(html: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        let (text, tail) = rest.split_at(lt);
        if !text.is_empty() {
            out.push(Token::Text(text));
        }
        match tag_end(tail) {
            Some((name, closing, end)) if !name.is_empty() => {
                out.push(if closing {
                    Token::Close(name)
                } else {
                    Token::Open(name)
                });
                rest = &tail[end..];
            }
            _ => {
                // Not a tag: keep the `<` as text and carry on after it.
                out.push(Token::Text("<"));
                rest = &tail[1..];
            }
        }
    }
    if !rest.is_empty() {
        out.push(Token::Text(rest));
    }
    out
}

/// Reads one tag starting at `tail[0] == '<'`: its lower-cased name, whether
/// it closes, and the byte index just past its `>`. Quoted attribute values
/// may contain `>` and are skipped as a unit.
fn tag_end(tail: &str) -> Option<(String, bool, usize)> {
    let mut chars = tail.char_indices().skip(1).peekable();
    let mut closing = false;
    if let Some((_, '/')) = chars.peek() {
        closing = true;
        chars.next();
    }
    let mut name = String::new();
    while let Some((_, c)) = chars.peek() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
            chars.next();
        } else {
            break;
        }
    }
    let mut quote: Option<char> = None;
    for (index, c) in chars {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '>' => return Some((name, closing, index + 1)),
            None => {}
        }
    }
    None
}

/// Decodes the entities a browser's serialiser writes, so the text can be
/// escaped exactly once on the way out. Unknown entities stay as typed.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(semi) = tail.find(';').filter(|&i| i <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => entity
                .strip_prefix('#')
                .and_then(|number| {
                    number.strip_prefix(['x', 'X']).map_or_else(
                        || number.parse::<u32>().ok(),
                        |hex| u32::from_str_radix(hex, 16).ok(),
                    )
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn sanitize(html: &str, allowed: &[&str]) -> String {
    let mut out = String::with_capacity(html.len());
    for token in tokens(html) {
        match token {
            Token::Text(text) => out.push_str(&esc(&decode_entities(text))),
            Token::Open(name) if allowed.contains(&name.as_str()) => {
                out.push('<');
                out.push_str(&name);
                out.push('>');
            }
            Token::Close(name) if allowed.contains(&name.as_str()) && name != "br" => {
                out.push_str("</");
                out.push_str(&name);
                out.push('>');
            }
            Token::Open(_) | Token::Close(_) => {}
        }
    }
    out
}

/// An inline field, safe for the page: bold and emphasis survive, nothing
/// else does, and every character of text is escaped.
#[must_use]
pub fn sanitize_inline(html: &str) -> String {
    sanitize(html, INLINE_TAGS)
}

/// A paragraph field, safe for the page. Plain text with no markup at all
/// keeps its line breaks, as the studio shows it.
#[must_use]
pub fn sanitize_rich(html: &str) -> String {
    let sanitized = sanitize(html, RICH_TAGS);
    if html.contains('<') {
        sanitized
    } else {
        sanitized.replace('\n', "<br>")
    }
}

/// An inline field as one line of plain text — for the PDF, whose fonts set
/// characters, not markup.
#[must_use]
pub fn plain_inline(html: &str) -> String {
    let mut out = String::new();
    for token in tokens(html) {
        if let Token::Text(text) = token {
            out.push_str(&decode_entities(text));
        }
    }
    collapse(&out)
}

/// A paragraph field as lines of plain text, one per block — a paragraph, a
/// heading, a list item (with a `•` or its number in front). Blank lines are
/// dropped; a text with no markup keeps its own line breaks.
#[must_use]
pub fn plain_lines(html: &str) -> Vec<String> {
    if !html.contains('<') {
        return html
            .lines()
            .map(collapse)
            .filter(|line| !line.is_empty())
            .collect();
    }
    let mut lines: Vec<String> = Vec::new();
    // The line being gathered: a list item's indent and marker, kept apart
    // from its text so collapsing the text's whitespace leaves them alone.
    let mut prefix = String::new();
    let mut current = String::new();
    // The open lists, innermost last, with the count of items so far in each.
    let mut lists: Vec<(bool, usize)> = Vec::new();
    let flush = |prefix: &mut String, current: &mut String, lines: &mut Vec<String>| {
        let text = collapse(current);
        if !text.is_empty() {
            lines.push(format!("{prefix}{text}"));
        }
        prefix.clear();
        current.clear();
    };
    for token in tokens(html) {
        match token {
            Token::Text(text) => current.push_str(&decode_entities(text)),
            Token::Open(name) => match name.as_str() {
                "ol" | "ul" => {
                    flush(&mut prefix, &mut current, &mut lines);
                    lists.push((name == "ol", 0));
                }
                "li" => {
                    flush(&mut prefix, &mut current, &mut lines);
                    let depth = lists.len().saturating_sub(1);
                    let marker = match lists.last_mut() {
                        Some((true, count)) => {
                            *count += 1;
                            format!("{count}.")
                        }
                        _ => "\u{2022}".to_owned(),
                    };
                    prefix = format!("{}{marker} ", "    ".repeat(depth));
                }
                tag if LINE_BREAKING.contains(&tag) => {
                    flush(&mut prefix, &mut current, &mut lines);
                }
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "ol" | "ul" => {
                    flush(&mut prefix, &mut current, &mut lines);
                    lists.pop();
                }
                tag if LINE_BREAKING.contains(&tag) => {
                    flush(&mut prefix, &mut current, &mut lines);
                }
                _ => {}
            },
        }
    }
    flush(&mut prefix, &mut current, &mut lines);
    lines
}

/// Whitespace as a browser renders it: runs collapse to one space, and the
/// ends are trimmed. A no-break space is kept as a space.
fn collapse(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_listed_tags_survive_and_never_with_attributes() {
        let stored = "<p onclick=\"steal()\">Hello <strong class=\"x\">world</strong> \
                      <script>alert(1)</script><a href=\"javascript:1\">link</a></p>";
        assert_eq!(
            sanitize_rich(stored),
            "<p>Hello <strong>world</strong> alert(1)link</p>"
        );
        // The inline list is narrower: a paragraph tag inside a heading is
        // unwrapped, the emphasis kept.
        assert_eq!(
            sanitize_inline("<p>Scope <em>2026</em></p>"),
            "Scope <em>2026</em>"
        );
        // Case does not matter, and `<br>` never gets a closing tag.
        assert_eq!(sanitize_rich("a<BR>b</br>"), "a<br>b");
    }

    #[test]
    fn text_is_escaped_exactly_once() {
        // What a browser serialises: entities in, the same entities out —
        // never doubled, never dropped.
        assert_eq!(
            sanitize_rich("<p>Fish &amp; chips &lt; 5&#8364; &quot;x&quot;</p>"),
            "<p>Fish &amp; chips &lt; 5€ &quot;x&quot;</p>"
        );
        // A raw `<` that is not a tag is text and escaped as such.
        assert_eq!(
            sanitize_inline("a < b <strong>c</strong>"),
            "a &lt; b <strong>c</strong>"
        );
        // A quoted `>` inside an attribute does not end the tag early.
        assert_eq!(sanitize_inline("<b title=\"a>b\">x</b>"), "<b>x</b>");
    }

    #[test]
    fn plain_text_keeps_its_line_breaks_and_nothing_else_does() {
        assert_eq!(sanitize_rich("one\ntwo"), "one<br>two");
        assert_eq!(sanitize_rich("<p>one\ntwo</p>"), "<p>one\ntwo</p>");
    }

    #[test]
    fn flattening_gives_the_pdf_one_line_per_block() {
        let html = "<h2>Scope</h2><p>Three   phases &amp; a <strong>test</strong>.</p>\
                    <ol><li>Design</li><li>Build<ul><li>API</li></ul></li></ol><p></p>";
        assert_eq!(
            plain_lines(html),
            [
                "Scope",
                "Three phases & a test.",
                "1. Design",
                "2. Build",
                "    \u{2022} API",
            ]
        );
        assert_eq!(plain_lines("one\n\ntwo"), ["one", "two"]);
        assert_eq!(plain_inline("<em>Acme</em>&nbsp;GmbH"), "Acme GmbH");
    }
}
