//! Editing text on the page and asking the assistant for the same rewrite are
//! ONE path (ADR 0042), proven where the two halves meet: every coordinate the
//! editable preview marks is a coordinate `rewrite_copy` accepts, and applying
//! it changes that property and nothing else.
//!
//! This is the test that stops the two drifting. The renderer could grow a
//! marker on an element whose text is not a typed string; the edit vocabulary
//! could tighten under it. Either would show up on a customer's screen as an
//! outline inviting a click that then fails — so both are asserted here, over
//! a page carrying every section type at once.
//!
//! No database, no HTTP: this is the pure seam between `alo-sites`'s renderer
//! and `alo-ai`'s edit operations, which is exactly what the editor puts
//! together.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use alo_ai::{SiteEditEnvelope, SiteEditOperation, SiteSectionTarget, apply_site_edit};
use alo_sites::render::{
    EN, ImageSources, PageRenderContext, SiteRenderContext, render_page, render_page_editable,
    render_page_preview,
};
use alo_store::site_model::{SECTIONS_SCHEMA_VERSION, Section, SectionsEnvelope};
use alo_store::site_theme::SiteTheme;
use serde_json::{Value, json};

/// One page carrying every section type that renders text, each with its
/// optional properties filled — the widest surface the marks can appear on.
fn page_value() -> Value {
    let image = json!({"blob_id": "9hK3vQ2mR8pT1xWz4bC5dg", "alt": "The roasting drum"});
    json!({
        "schema_version": SECTIONS_SCHEMA_VERSION,
        "sections": [
            {"type": "nav", "links": [{"label": "Home", "href": "/"}], "cta": null},
            {
                "type": "hero",
                "heading": "Coffee roasted the morning it ships",
                "subheading": "Small-batch roastery on the harbour",
                "image": null,
                "primary_cta": {"label": "Shop roasts", "href": "/shop"},
                "secondary_cta": null
            },
            {
                "type": "features",
                "heading": "Why Nordwind",
                "intro": "Three promises on every bag.",
                "items": [
                    {"title": "Roasted to order", "body": "Your batch goes in after you order.", "icon": null},
                    {"title": "Roast-day dispatch", "body": "It leaves the same afternoon.", "icon": null}
                ]
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
                "intro": "Pause or cancel any time.",
                "tiers": [{
                    "name": "Weekly",
                    "price": "18 EUR",
                    "period": "billed weekly",
                    "description": "Two 250g bags every week.",
                    "features": ["Free shipping", "Roast-day dispatch"],
                    "cta": null,
                    "highlighted": true
                }]
            },
            {
                "type": "team",
                "heading": "The roasters",
                "members": [{
                    "name": "Jonas Meer",
                    "role": "Head roaster",
                    "photo": null,
                    "bio": "Twenty years at the drum."
                }]
            },
            {
                "type": "faq",
                "heading": "Questions",
                "items": [{"question": "How fresh is the coffee?", "answer": "It ships the day it is roasted."}]
            },
            {
                "type": "cta",
                "heading": "Taste the difference",
                "body": "First bag ships free.",
                "button": {"label": "Order now", "href": "/order"}
            },
            {
                "type": "contact_form",
                "heading": "Wholesale enquiries",
                "body": "We answer within one business day.",
                "form_id": null,
                "success_message": null
            },
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

/// Every `data-alo-text` value in the document, in document order.
fn marks(html: &str) -> Vec<String> {
    html.match_indices("data-alo-text=\"")
        .map(|(at, needle)| {
            let rest = &html[at + needle.len()..];
            rest[..rest.find('"').expect("unterminated attribute")].to_owned()
        })
        .collect()
}

fn rewrite(key: &str, sections: &[Section], text: &str) -> SiteEditEnvelope {
    let (index, pointer) = key.split_at(key.find('/').expect("a key names a pointer"));
    let index: usize = index.parse().expect("a key starts with a section index");
    SiteEditEnvelope {
        schema_version: 1,
        operations: vec![SiteEditOperation::RewriteCopy {
            target: SiteSectionTarget {
                index,
                kind: sections[index].kind().to_owned(),
            },
            pointer: pointer.to_owned(),
            text: text.to_owned(),
        }],
    }
}

/// The property this test exists for: the editor's outline and the
/// assistant's vocabulary name the same things, and one gesture changes one
/// property.
#[test]
fn every_marked_element_is_a_rewrite_copy_target_and_changes_only_itself() {
    let page = SectionsEnvelope::from_value(page_value()).unwrap();
    let html = render(true);
    let keys = marks(&html);
    assert!(!keys.is_empty(), "the editable preview marked nothing");

    for key in &keys {
        let edit = rewrite(key, &page.sections, "Edited on the page");
        let result = apply_site_edit(&page, &edit)
            .unwrap_or_else(|error| panic!("marked {key} is not an applicable rewrite: {error}"));

        // The change landed where the mark said it would…
        let before = page.to_value().unwrap();
        let after = result.to_value().unwrap();
        let pointer = format!("/sections/{key}");
        assert_eq!(
            after.pointer(&pointer),
            Some(&json!("Edited on the page")),
            "{key} did not receive the new text"
        );

        // …and nowhere else. Put the old value back and the page is identical
        // again, which is the same thing undo asks of it.
        let old = before.pointer(&pointer).unwrap().as_str().unwrap();
        let undone = apply_site_edit(&result, &rewrite(key, &result.sections, old)).unwrap();
        assert_eq!(
            undone.to_value().unwrap(),
            before,
            "{key} changed more than itself"
        );
    }
}

/// The coverage the marks are expected to have, pinned. A section type that
/// loses its editable text — or gains one nobody meant to expose — shows up
/// here as a diff to review rather than as a quiet change on a customer's
/// screen.
#[test]
fn the_marked_coordinates_are_exactly_these() {
    assert_eq!(
        marks(&render(true)),
        [
            "1/heading",
            "1/subheading",
            "2/heading",
            "2/intro",
            "2/items/0/title",
            "2/items/0/body",
            "2/items/1/title",
            "2/items/1/body",
            "3/heading",
            "3/body",
            "4/heading",
            "5/heading",
            "5/items/0/quote",
            "6/heading",
            "6/intro",
            "6/tiers/0/name",
            "6/tiers/0/description",
            "6/tiers/0/features/0",
            "6/tiers/0/features/1",
            "7/heading",
            "7/members/0/name",
            "7/members/0/role",
            "7/members/0/bio",
            "8/heading",
            "8/items/0/question",
            "8/items/0/answer",
            "9/heading",
            "9/body",
            "10/heading",
            "10/body",
            "12/text",
        ]
    );
}

/// Custom code is written by hand and only by hand — the assistant is refused
/// it (`alo_ai::site_edits`), so the page must not offer it either. An outline
/// inviting a click the door would refuse is worse than no outline.
#[test]
fn a_custom_code_block_is_never_offered_for_editing() {
    let page = SectionsEnvelope::from_value(page_value()).unwrap();
    let custom = page
        .sections
        .iter()
        .position(|section| section.kind() == "custom_code")
        .unwrap();
    assert!(
        marks(&render(true))
            .iter()
            .all(|key| !key.starts_with(&format!("{custom}/"))),
        "the custom-code block was marked editable"
    );
    // And the door agrees, which is why it must not be.
    assert!(
        apply_site_edit(
            &page,
            &rewrite(&format!("{custom}/heading"), &page.sections, "Renamed")
        )
        .is_err()
    );
}

/// Nothing a visitor receives changes. The annotations and their script exist
/// in the editable preview and in no other rendering — a published page is
/// byte-for-byte what it was, and the editable one is the same document with
/// the marks added and the editing surface appended.
#[test]
fn published_and_read_only_renderings_carry_no_editing_surface() {
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
        assert!(!document.contains("data-alo-text"));
        assert!(!document.contains("site-text-edit"));
        assert!(!document.contains("contentEditable"));
    }

    let editable = render(true);
    assert!(editable.contains("site-text-edit"));

    // Strip the annotations and the editable document is the read-only one
    // again, with the editing surface appended after `</main>`: what is being
    // edited is exactly what publishing renders, not a lookalike.
    let plain = render(false);
    let body = plain
        .strip_suffix("</body>\n</html>\n")
        .expect("the preview ends with a body");
    assert!(
        strip_marks(&editable).starts_with(body),
        "the editable preview is not the published document plus its marks"
    );
}

/// The document without its annotations — both of them: the text coordinates
/// this file is about, and the section coordinates a move uses (S3.01b).
fn strip_marks(html: &str) -> String {
    let mut out = html.to_owned();
    for attribute in [" data-alo-text=\"", " data-alo-section=\""] {
        let mut stripped = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(at) = rest.find(attribute) {
            stripped.push_str(&rest[..at]);
            let after = &rest[at + attribute.len()..];
            rest = &after[after.find('"').expect("unterminated attribute") + 1..];
        }
        stripped.push_str(rest);
        out = stripped;
    }
    out
}
