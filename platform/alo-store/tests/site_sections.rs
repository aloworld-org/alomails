//! Golden-fixture tests for the sites section schema (v1): every section type
//! has a checked-in JSON fixture that must parse strictly, pass content
//! validation, and re-serialize to the exact same value — pinning the wire
//! shape against accidental drift. Pure tests; no database.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::SectionsEnvelope;
use serde_json::Value;

/// One golden fixture per section type; the name is the expected wire tag.
const SECTION_FIXTURES: &[(&str, &str)] = &[
    ("nav", include_str!("fixtures/site_sections/nav.json")),
    ("hero", include_str!("fixtures/site_sections/hero.json")),
    (
        "features",
        include_str!("fixtures/site_sections/features.json"),
    ),
    (
        "text_image",
        include_str!("fixtures/site_sections/text_image.json"),
    ),
    (
        "gallery",
        include_str!("fixtures/site_sections/gallery.json"),
    ),
    (
        "testimonials",
        include_str!("fixtures/site_sections/testimonials.json"),
    ),
    (
        "pricing",
        include_str!("fixtures/site_sections/pricing.json"),
    ),
    ("team", include_str!("fixtures/site_sections/team.json")),
    ("faq", include_str!("fixtures/site_sections/faq.json")),
    ("cta", include_str!("fixtures/site_sections/cta.json")),
    (
        "contact_form",
        include_str!("fixtures/site_sections/contact_form.json"),
    ),
    ("footer", include_str!("fixtures/site_sections/footer.json")),
];

const FULL_PAGE: &str = include_str!("fixtures/site_sections/full_page.json");

#[test]
fn every_section_golden_parses_validates_and_round_trips() {
    for (name, raw) in SECTION_FIXTURES {
        let golden: Value = serde_json::from_str(raw).expect(name);
        let envelope = SectionsEnvelope::from_value(golden.clone())
            .unwrap_or_else(|e| panic!("{name} fixture rejected: {e}"));
        assert_eq!(
            envelope.sections.len(),
            1,
            "{name} fixture must hold exactly its one section"
        );
        assert_eq!(
            envelope.sections[0].kind(),
            *name,
            "{name} fixture parsed as the wrong section type"
        );
        let back = envelope.to_value().unwrap();
        assert_eq!(
            back, golden,
            "{name}: re-serialization drifted from the golden"
        );
    }
}

#[test]
fn section_goldens_cover_the_whole_vocabulary_exactly_once() {
    let mut names: Vec<&str> = SECTION_FIXTURES.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 12, "one golden per section type, no gaps");
}

#[test]
fn full_page_golden_round_trips_with_all_sections_in_order() {
    let golden: Value = serde_json::from_str(FULL_PAGE).unwrap();
    let envelope = SectionsEnvelope::from_value(golden.clone())
        .unwrap_or_else(|e| panic!("full page fixture rejected: {e}"));
    let kinds: Vec<&str> = envelope.sections.iter().map(|s| s.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            "nav",
            "hero",
            "features",
            "text_image",
            "gallery",
            "testimonials",
            "pricing",
            "team",
            "faq",
            "cta",
            "contact_form",
            "footer",
        ],
        "full page golden must exercise every section type in page order"
    );
    let back = envelope.to_value().unwrap();
    assert_eq!(back, golden, "full page re-serialization drifted");
}
