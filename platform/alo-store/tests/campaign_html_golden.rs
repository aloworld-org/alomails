//! The pinned output of the campaign renderer (alo Campaigns, ADR 0044, C3.2).
//!
//! Queue item C3.2: *golden-file tests: **the same blocks must produce the same
//! HTML**, so a regression is visible rather than discovered by a customer's
//! recipients.* That last clause is the reason this file exists rather than a
//! handful of `contains` assertions. Email HTML fails silently: a dropped
//! `mso-line-height-rule`, a `bgcolor` attribute lost beside its CSS twin, a
//! `<th>` demoted to a `<td>` — none of it throws, none of it looks wrong in a
//! browser, and all of it arrives broken in Outlook. A whole-document diff is
//! the only check that catches a change nobody meant to make.
//!
//! **Each golden is named for the `schema_version` it is written against**,
//! because a golden is only meaningful against one version of the block model:
//! `campaign_html_v1_*.html`. A bump to
//! [`CAMPAIGN_CONTENT_SCHEMA_VERSION`] is a new set of files under a new
//! name, not a re-blessing of these — the old ones still describe what the old
//! model produced, which is what a reader of a two-year-old mail needs.
//! [`the_goldens_are_pinned_to_the_model_they_were_blessed_against`] fails the
//! day that stops being true.
//!
//! To re-bless after a deliberate change: `UPDATE_GOLDENS=1 cargo nextest run
//! -p alo-store campaign_html_golden`, then **read the diff** — that reading is
//! the test, not the green tick afterwards.
//!
//! No database is touched here. The renderer is a pure function of a validated
//! body, which is the property that makes pinning it worth anything at all.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use alo_store::campaign_unsubscribe_link::UnsubscribeInvitation;
use alo_store::{
    CAMPAIGN_CONTENT_SCHEMA_VERSION, CampaignBlock, CampaignContent, CampaignLetter,
    render_campaign_html,
};
use serde_json::{Value, json};

/// The way out the letter must carry (C2.4/C2.5). Not English on purpose: the
/// words belong to the caller, and a golden pinning "Unsubscribe" would read as
/// though they belonged to the renderer.
fn unsub() -> UnsubscribeInvitation {
    UnsubscribeInvitation {
        one_click_url: "https://alo.test/jmap/campaign-unsubscribe/9tOKENx".to_owned(),
        page_url: "https://alo.test/unsubscribe/9tOKENx".to_owned(),
        topic: Some("Nieuwsbrief".to_owned()),
        link_text: "Uitschrijven".to_owned(),
    }
}

fn body(blocks: Value) -> CampaignContent {
    CampaignContent::from_value(json!({
        "schema_version": CAMPAIGN_CONTENT_SCHEMA_VERSION,
        "blocks": blocks,
    }))
    .expect("every fixture in this file must pass the write gate")
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn assert_golden(name: &str, rendered: &str) {
    let path = golden_path(name);
    if std::env::var("UPDATE_GOLDENS").is_ok() {
        fs::create_dir_all(path.parent().expect("a golden lives in a directory"))
            .expect("the golden directory is writable");
        fs::write(&path, rendered).expect("the golden is writable");
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {name}; run with UPDATE_GOLDENS=1"));
    assert_eq!(
        rendered, expected,
        "{name} drifted; re-bless with UPDATE_GOLDENS=1 and read the diff — a mail client will \
         not tell you which half of the change was the mistake"
    );
}

/// A letter with every block a campaign can hold, of the kind somebody would
/// actually send: a heading, prose, a deliberate blank line, a price table with
/// a currency symbol, a sub-heading, and a code sample with indentation and an
/// aligned comment.
fn newsletter() -> CampaignContent {
    body(json!([
        { "type": "heading", "id": "b1", "level": 1, "text": "Spring prices" },
        {
            "type": "paragraph",
            "id": "b2",
            "text": "Beste klant,\nvanaf maandag gelden de prijzen hieronder. Alles is per liter.",
        },
        { "type": "paragraph", "id": "b3", "text": "" },
        {
            "type": "table",
            "id": "b4",
            "rows": [
                ["Product", "Prijs", "Vanaf"],
                ["Olijfolie — extra vierge", "12,50 €", "2 maart"],
                ["Zonnebloemolie", "6,00 €", "2 maart"],
            ],
        },
        { "type": "heading", "id": "b5", "level": 2, "text": "Bestellen via de API" },
        {
            "type": "code",
            "id": "b6",
            "code": "curl https://api.alo.test/orders \\\n  -H 'Accept: application/json' \\\n  -d '{\"sku\": \"olive-1l\"}'   # één liter",
            "language": "bash",
        },
        {
            "type": "paragraph",
            "id": "b7",
            "text": "Met vriendelijke groet,\nNordwind & Co",
        },
    ]))
}

/// Text chosen to break out of the document if anything is left unescaped:
/// tag delimiters, both kinds of quote, an ampersand that is already an entity,
/// and a code sample that tries to close the layout table it sits inside.
fn hostile() -> CampaignContent {
    body(json!([
        { "type": "heading", "id": "b1", "level": 2, "text": "<script>alert('x')</script> & Co" },
        {
            "type": "paragraph",
            "id": "b2",
            "text": "She said \"it's <b>fine</b>\" &amp; left — 5 > 3.",
        },
        {
            "type": "table",
            "id": "b3",
            "rows": [
                ["</th></tr></table>", "\"quoted\""],
                ["<img src=x onerror=alert(1)>", "a & b"],
            ],
        },
        {
            "type": "code",
            "id": "b4",
            "code": "</td></tr></table><script>alert(1)</script>\n\tconst x = '<a href=\"#\">';",
            "language": "html",
        },
    ]))
}

fn render(subject: &str, preheader: Option<&str>, content: &CampaignContent) -> String {
    render_campaign_html(&CampaignLetter {
        subject,
        preheader,
        content,
        unsubscribe: &unsub(),
    })
    .expect("a body that passed the write gate renders")
}

/// The whole letter, pinned. This is the file to read when a change to the
/// renderer needs reviewing.
#[test]
fn a_whole_letter_is_pinned_block_for_block() {
    let html = render(
        "Nieuwe prijzen vanaf maandag",
        Some("Olijfolie 12,50 € — geldig vanaf 2 maart"),
        &newsletter(),
    );
    assert_golden("campaign_html_v1_letter.html", &html);
}

/// The state the composer opens in — one empty paragraph and no preheader.
/// It is pinned because it is the document most recipients of a mistake would
/// receive: an accidental send of an untouched draft still has to be a valid,
/// harmless letter rather than a broken one.
#[test]
fn the_editors_starting_state_is_pinned() {
    let content = body(json!([{ "type": "paragraph", "id": "b1", "text": "" }]));
    assert_golden(
        "campaign_html_v1_starter.html",
        &render("Concept", None, &content),
    );
}

/// Every escape, pinned. A regression here is a cross-site scripting hole in a
/// webmail preview and a broken letter in every client at once, so the diff is
/// worth reading character by character.
#[test]
fn a_hostile_body_is_pinned_so_an_escaping_regression_is_a_diff() {
    let html = render(
        "</title><script>alert(1)</script>",
        Some("<b>preview</b> & \"more\""),
        &hostile(),
    );
    assert_golden("campaign_html_v1_hostile.html", &html);

    // The diff is the test, but these are the two facts a reader should not
    // have to reconstruct from it.
    assert!(!html.contains("<script"), "a script tag survived");
    assert!(!html.contains("<img"), "an image tag survived");
}

/// A golden pins what was rendered, so it cannot notice a block that stopped
/// being rendered at all — the fixture would simply be blessed without it. This
/// is the guard: the pinned letter must exercise every variant the model has.
#[test]
fn the_pinned_letter_exercises_every_block_the_model_has() {
    let kinds: Vec<&str> = newsletter()
        .blocks
        .iter()
        .map(CampaignBlock::kind)
        .collect();
    for kind in ["heading", "paragraph", "table", "code"] {
        assert!(
            kinds.contains(&kind),
            "the pinned letter no longer contains a {kind} block, so the golden would not \
             notice it disappearing from the renderer"
        );
    }
}

/// Walks the document's tags and returns the element nesting, or the first
/// place it breaks.
///
/// Comments are dropped first — the Outlook ghost table opens a `<table>` in
/// one comment and closes it in another, which is well-formed for Word and
/// deliberately invisible to everybody else, so it must not be counted here.
fn nesting_fault(html: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        rest = &rest[open..];
        if rest.starts_with("<!") {
            // The doctype and the comments alike run to the next `>` that ends
            // them; a comment's `>` may sit inside, so prefer `-->` when it is
            // a comment.
            let end = if rest.starts_with("<!--") {
                rest.find("-->").map(|at| at + 3)
            } else {
                rest.find('>').map(|at| at + 1)
            }?;
            rest = &rest[end..];
            continue;
        }
        let end = rest.find('>')?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.ends_with('/') {
            continue; // `<meta … />`, `<br />` — closed where they open.
        }
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name => {}
                Some(open) => return Some(format!("</{name}> closes <{open}>")),
                None => return Some(format!("</{name}> closes nothing")),
            }
        } else {
            stack.push(tag.split([' ', '\n']).next().unwrap_or(tag));
        }
    }
    stack.pop().map(|open| format!("<{open}> is never closed"))
}

/// Every element opens and closes in the right order.
///
/// A golden pins bytes and would happily pin a broken document, and a browser
/// would quietly repair one while Outlook drew the rest of the letter inside a
/// stray cell. The three pinned documents and an empty draft are all walked, so
/// the check covers the paths a fixture happens not to take.
#[test]
fn every_pinned_document_opens_and_closes_its_elements_in_order() {
    let documents = [
        (
            "letter",
            render("Nieuwe prijzen", Some("Vanaf maandag"), &newsletter()),
        ),
        ("hostile", render("</title>", Some("<b>x</b>"), &hostile())),
        ("empty", render("", None, &CampaignContent::empty())),
        (
            "starter",
            render(
                "Concept",
                None,
                &body(json!([{ "type": "paragraph", "id": "b1", "text": "" }])),
            ),
        ),
    ];
    for (name, html) in documents {
        assert_eq!(
            nesting_fault(&html),
            None,
            "the {name} document is not a well-formed tree"
        );
    }
}

/// The goldens are named `v1` because they describe what version 1 of the block
/// model renders to. A bump gets its own files under its own name; re-blessing
/// these in place would destroy the record of what the previous model produced,
/// which is what somebody debugging a two-year-old mail actually needs.
#[test]
fn the_goldens_are_pinned_to_the_model_they_were_blessed_against() {
    assert_eq!(
        CAMPAIGN_CONTENT_SCHEMA_VERSION, 1,
        "the block model moved past the version these goldens are named for — add \
         campaign_html_v{CAMPAIGN_CONTENT_SCHEMA_VERSION}_*.html beside them, do not re-bless \
         the v1 files"
    );
}

/// The property the goldens rest on. If the renderer were not a pure function
/// of its input, a green golden would mean only that this machine's clock and
/// hash seed happened not to move.
#[test]
fn the_same_blocks_produce_the_same_html_on_every_call() {
    let content = newsletter();
    let once = render("Nieuwe prijzen", Some("Vanaf maandag"), &content);
    let twice = render("Nieuwe prijzen", Some("Vanaf maandag"), &content);
    assert_eq!(once, twice);

    // And a body rebuilt from its own stored JSON renders identically — the
    // round trip through the column must not change a single byte of the mail.
    let stored = content.to_json().expect("serialises");
    let reloaded = CampaignContent::parse(&stored).expect("re-reads");
    assert_eq!(
        render("Nieuwe prijzen", Some("Vanaf maandag"), &reloaded),
        once,
        "a saved and reloaded body must render to the same letter"
    );
}
