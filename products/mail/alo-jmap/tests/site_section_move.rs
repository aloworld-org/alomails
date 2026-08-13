//! Dragging a section on the page and asking the assistant to move it are ONE
//! path (ADR 0042), proven where the two halves meet: every section the
//! editable preview marks is a section `reorder_section` accepts, and applying
//! one changes the **order and nothing else**.
//!
//! That last clause is the whole reason ADR 0042 can promise a reviewable diff
//! for a gesture. A canvas move changes coordinates nobody can read; a move
//! here permutes a list of typed values, each one byte-identical before and
//! after. The goldens below pin that as text: the order before, the order
//! after, and — on every move — an empty list of changed values.
//!
//! No database, no HTTP: this is the pure seam between `alo-sites`'s renderer
//! and `alo-ai`'s edit operations, which is exactly what the editor puts
//! together. Regenerate the goldens with `UPDATE_GOLDEN=1 cargo test -p
//! alo-jmap --test site_section_move`, then read the diff before committing it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::fmt::Write as _;

use alo_ai::{SiteEditEnvelope, SiteEditOperation, SiteSectionTarget, apply_site_edit};
use alo_sites::render::{
    EN, ImageSources, PageRenderContext, SiteRenderContext, render_page, render_page_editable,
    render_page_preview,
};
use alo_store::site_model::{SECTIONS_SCHEMA_VERSION, SectionsEnvelope};
use alo_store::site_theme::SiteTheme;
use serde_json::{Value, json};

/// One page carrying **every** section type, including the three that render
/// from a publish snapshot and the custom-code block. A section that cannot be
/// rendered from a missing snapshot still renders its empty state, and is
/// still a section somebody may want to move — so it belongs here.
fn page_value() -> Value {
    let image = json!({"blob_id": "9hK3vQ2mR8pT1xWz4bC5dg", "alt": "The roasting drum"});
    json!({
        "schema_version": SECTIONS_SCHEMA_VERSION,
        "sections": [
            {"type": "nav", "links": [{"label": "Home", "href": "/"}], "cta": null},
            {
                "type": "hero",
                "heading": "Coffee roasted the morning it ships",
                "subheading": null,
                "image": null,
                "primary_cta": null,
                "secondary_cta": null
            },
            {
                "type": "features",
                "heading": "Why Nordwind",
                "intro": null,
                "items": [{"title": "Roasted to order", "body": "Your batch goes in after you order.", "icon": null}]
            },
            {
                "type": "text_image",
                "heading": "The roastery",
                "body": "A 1962 Probat drum, rebuilt by hand.",
                "image": image,
                "image_side": "left"
            },
            {"type": "gallery", "heading": "Inside the roastery", "images": [image]},
            {
                "type": "testimonials",
                "heading": "What cafes say",
                "items": [{
                    "quote": "The freshest beans we have pulled shots with.",
                    "author": "Mara Lindqvist",
                    "role": "Head barista"
                }]
            },
            {
                "type": "pricing",
                "heading": "Subscriptions",
                "intro": null,
                "tiers": [{
                    "name": "Weekly",
                    "price": "18 EUR",
                    "period": null,
                    "description": null,
                    "features": ["Free shipping"],
                    "cta": null,
                    "highlighted": false
                }]
            },
            {
                "type": "team",
                "heading": "The roasters",
                "members": [{"name": "Jonas Meer", "role": "Head roaster", "photo": null, "bio": null}]
            },
            {
                "type": "faq",
                "heading": "Questions",
                "items": [{"question": "How fresh is the coffee?", "answer": "It ships the day it is roasted."}]
            },
            {"type": "cta", "heading": "Taste the difference", "body": null, "button": {"label": "Order now", "href": "/order"}},
            {
                "type": "contact_form",
                "heading": "Wholesale enquiries",
                "body": null,
                "form_id": null,
                "success_message": null
            },
            {"type": "collection", "collection_id": "seasonal-roasts", "heading": "Seasonal roasts"},
            {"type": "catalog", "catalog_id": "harbour-menu", "heading": "On the counter", "category": null},
            {"type": "booking", "booking_id": "studio-consultation", "heading": "Come and talk to us"},
            {
                "type": "custom_code",
                "heading": "Roast timer",
                "title": "A timer counting down the current roast",
                "html": "<p>12:00</p>",
                "css": null,
                "js": null,
                "capabilities": {"scripts": false, "inline_images": false},
                "height_px": 220
            },
            {"type": "footer", "text": "(c) Nordwind Coffee Roasters", "links": []}
        ]
    })
}

fn render(editable: bool) -> String {
    let theme = SiteTheme::new();
    let value = page_value();
    let site = SiteRenderContext {
        name: "Nordwind Coffee Roasters",
        base_url: "https://nordwind.alosites.com",
        locale: "en",
        theme: &theme,
        strings: &EN,
        images: ImageSources::PublicPaths,
    };
    let page = PageRenderContext {
        path: "/",
        title: "Home",
        seo_title: None,
        seo_description: None,
        sections: &value,
        collections: &HashMap::new(),
        catalogs: &HashMap::new(),
        bookings: &HashMap::new(),
    };
    if editable {
        render_page_editable(&site, &page, "body{}")
    } else {
        render_page_preview(&site, &page, "body{}")
    }
}

/// Every `data-alo-section` value in the document's **markup**, in document
/// order. The scripts are cut off first: the editing script names the same
/// attribute in a selector, and a coordinate is a thing on an element, not a
/// string in a program.
fn marks(html: &str) -> Vec<usize> {
    let markup = html.split("<script").next().expect("a document");
    markup
        .match_indices(" data-alo-section=\"")
        .map(|(at, needle)| {
            let rest = &markup[at + needle.len()..];
            rest[..rest.find('"').expect("unterminated attribute")]
                .parse()
                .expect("a section mark is an index")
        })
        .collect()
}

/// The kinds of a page's sections, in order — the whole of what a move
/// changes.
fn order(page: &SectionsEnvelope) -> Vec<&'static str> {
    page.sections
        .iter()
        .map(|section| section.kind())
        .collect::<Vec<_>>()
}

fn reorder(index: usize, kind: &str, to: usize) -> SiteEditEnvelope {
    SiteEditEnvelope {
        schema_version: 1,
        operations: vec![SiteEditOperation::ReorderSection {
            target: SiteSectionTarget {
                index,
                kind: kind.to_owned(),
            },
            to,
        }],
    }
}

/// Every section carries its own position, exactly once, in page order — the
/// coordinate a drag reports and `reorder_section` names.
#[test]
fn every_section_is_marked_with_its_index_exactly_once() {
    let page = SectionsEnvelope::from_value(page_value()).unwrap();
    assert_eq!(
        marks(&render(true)),
        (0..page.sections.len()).collect::<Vec<_>>(),
        "the editable preview must mark each section once, in order"
    );
}

/// The property the whole design rests on: a move permutes typed values and
/// alters none of them. Asserted over **every** (from, to) pair on a page
/// carrying every section type — 240 moves — because "only the order changed"
/// is a claim about all of them, not about a sample.
#[test]
fn a_move_permutes_the_sections_and_changes_no_value() {
    let page = SectionsEnvelope::from_value(page_value()).unwrap();
    let before = page.to_value().unwrap();
    let count = page.sections.len();

    for from in 0..count {
        let kind = page.sections[from].kind().to_owned();
        for to in 0..count {
            if to == from {
                continue;
            }
            let moved = apply_site_edit(&page, &reorder(from, &kind, to))
                .unwrap_or_else(|error| panic!("section {from} cannot move to {to}: {error}"));

            // The section landed where it was asked to…
            let mut expected = order(&page);
            let section = expected.remove(from);
            expected.insert(to, section);
            assert_eq!(order(&moved), expected, "moving {from} to {to}");

            // …every value survived the journey byte for byte…
            let after = moved.to_value().unwrap();
            let mut sorted_before = before["sections"].as_array().unwrap().clone();
            let mut sorted_after = after["sections"].as_array().unwrap().clone();
            sorted_before.sort_by_key(|value| value.to_string());
            sorted_after.sort_by_key(|value| value.to_string());
            assert_eq!(sorted_after, sorted_before, "moving {from} to {to}");

            // …and the move back is the exact inverse, which is what undo asks
            // of it: the page is byte-identical again.
            let back = apply_site_edit(&moved, &reorder(to, &kind, from)).unwrap();
            assert_eq!(back.to_value().unwrap(), before, "moving {from} to {to}");
        }
    }
}

/// A destination off the end of the page is refused rather than clamped, and
/// a target naming the wrong type is refused rather than aimed at whatever
/// now sits at that index — the same staleness rule every other operation
/// obeys.
#[test]
fn an_impossible_move_is_refused() {
    let page = SectionsEnvelope::from_value(page_value()).unwrap();
    let count = page.sections.len();
    assert!(apply_site_edit(&page, &reorder(1, "hero", count)).is_err());
    assert!(apply_site_edit(&page, &reorder(1, "faq", 3)).is_err());
    assert!(apply_site_edit(&page, &reorder(count, "hero", 0)).is_err());
}

/// The diff a move produces, pinned as text: the order before, the order
/// after, and the sections whose *value* changed — which is always none.
///
/// This is the golden a reviewer reads when the editor's gesture changes. If
/// a move ever starts rewriting a property (a "helpful" heading renumber, a
/// layout hint written on the way past), it appears here as a line under
/// "values changed" and the test fails.
#[test]
fn the_diff_of_a_move_is_only_a_reordering() {
    let page = SectionsEnvelope::from_value(page_value()).unwrap();
    let before = page.to_value().unwrap();
    let mut out = String::new();

    writeln!(out, "sections: {}", order(&page).join(" ")).unwrap();
    // One move of each shape a person can make: down one, up one, to the very
    // top, to the very end, and the long haul across the page.
    for (from, to) in [(1, 2), (9, 8), (13, 1), (1, 14), (4, 12)] {
        let kind = page.sections[from].kind();
        let moved = apply_site_edit(&page, &reorder(from, kind, to)).unwrap();
        let after = moved.to_value().unwrap();

        writeln!(out).unwrap();
        writeln!(out, "move {kind} from {from} to {to}").unwrap();
        writeln!(out, "  order: {}", order(&moved).join(" ")).unwrap();
        writeln!(out, "  values changed:").unwrap();
        for value in after["sections"].as_array().unwrap() {
            if !before["sections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|original| original == value)
            {
                writeln!(out, "    {value}").unwrap();
            }
        }
    }

    golden("section-move-diff.txt", &out);
}

/// Nothing a visitor receives changes: the section coordinates and the
/// gestures that use them exist in the editable preview and in no other
/// rendering.
#[test]
fn published_and_read_only_renderings_carry_no_section_handles() {
    let theme = SiteTheme::new();
    let value = page_value();
    let site = SiteRenderContext {
        name: "Nordwind Coffee Roasters",
        base_url: "https://nordwind.alosites.com",
        locale: "en",
        theme: &theme,
        strings: &EN,
        images: ImageSources::PublicPaths,
    };
    let page = PageRenderContext {
        path: "/",
        title: "Home",
        seo_title: None,
        seo_description: None,
        sections: &value,
        collections: &HashMap::new(),
        catalogs: &HashMap::new(),
        bookings: &HashMap::new(),
    };

    for document in [render_page(&site, &page), render(false)] {
        assert!(!document.contains("data-alo-section"));
        assert!(!document.contains("site-section-move"));
        assert!(!document.contains("draggable"));
    }
    assert!(render(true).contains("site-section-move"));
}

/// Compares against the file in `tests/golden/`, or writes it when
/// `UPDATE_GOLDEN=1` — an intended change is one command and a diff to read,
/// and an unintended one is a failing test.
fn golden(name: &str, actual: &str) {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, actual).expect("the golden file could not be written");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name} is missing ({e}); regenerate with UPDATE_GOLDEN=1"));
    assert_eq!(
        actual, expected,
        "{name} has changed; if that is intended, regenerate with UPDATE_GOLDEN=1 and read the diff"
    );
}
