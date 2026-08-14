//! Golden-fixture tests for the sites section schema (v1): every section type
//! has a checked-in JSON fixture that must parse strictly, pass content
//! validation, and re-serialize to the exact same value — pinning the wire
//! shape against accidental drift. Pure tests; no database.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::SectionsEnvelope;
use alo_store::site_model::{ImageCrop, ImageFocalPoint};
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
    (
        "collection",
        include_str!("fixtures/site_sections/collection.json"),
    ),
    (
        "catalog",
        include_str!("fixtures/site_sections/catalog.json"),
    ),
    (
        "booking",
        include_str!("fixtures/site_sections/booking.json"),
    ),
    (
        "custom_code",
        include_str!("fixtures/site_sections/custom_code.json"),
    ),
    ("footer", include_str!("fixtures/site_sections/footer.json")),
];

const FULL_PAGE: &str = include_str!("fixtures/site_sections/full_page.json");

/// The image presentation props (crop, focal point, decorative) on the two
/// section types that carry images differently — one image, and a list.
const IMAGE_PRESENTATION: &str = include_str!("fixtures/site_sections/image_presentation.json");

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
    assert_eq!(names.len(), 16, "one golden per section type, no gaps");
}

/// The published list of section types and the fixture corpus are two
/// spellings of one vocabulary, and everything that offers a section type
/// (the editor's palette, S3.01d) iterates the published one. A variant added
/// to one and not the other is a type nothing offers, or a tile for a type
/// that does not exist.
#[test]
fn the_published_section_type_list_matches_the_goldens() {
    let mut published: Vec<&str> = alo_store::SECTION_KINDS.to_vec();
    let mut fixtures: Vec<&str> = SECTION_FIXTURES.iter().map(|(name, _)| *name).collect();
    published.sort_unstable();
    fixtures.sort_unstable();
    assert_eq!(published, fixtures);
}

/// The custom-code block through the **public write gate** — the same door the
/// editor and the AI ops path use. The unit tests in `site_custom_code` cover
/// the rule matrix; what is proven here is that no hostile shape reaches
/// storage by going around them, and that the refusal names the rule so a UI
/// can show it verbatim.
#[test]
fn the_write_gate_refuses_custom_code_that_would_escape_its_frame() {
    let block = |patch: Value| -> Value {
        let mut section = serde_json::json!({
            "type": "custom_code",
            "title": "Roast timer",
            "html": "<p>12:00</p>",
            "height_px": 220,
        });
        for (key, value) in patch.as_object().expect("patch is an object") {
            section[key] = value.clone();
        }
        serde_json::json!({"schema_version": 1, "sections": [section]})
    };

    for (patch, expected) in [
        // The wrapper's own blocks are not the tenant's to open.
        (
            serde_json::json!({"html": "<script>fetch('/steal')</script>"}),
            "<script>",
        ),
        // Nor the document around them.
        (
            serde_json::json!({"html": "<body onload=\"top.location='/'\">x</body>"}),
            "<body>",
        ),
        // Nothing is loaded from anywhere: the frame has no network.
        (
            serde_json::json!({"html": "<p>see https://tracker.example</p>"}),
            "no network access",
        ),
        // A script without the capability that runs it, and the reverse.
        (
            serde_json::json!({"js": "alert(1)"}),
            "must declare the scripts capability",
        ),
        (
            serde_json::json!({"capabilities": {"scripts": true}}),
            "only be declared by a block that has a script",
        ),
        // A part that would end its own inlined block.
        (
            serde_json::json!({"css": "body{}</style><img>", "js": "1", "capabilities": {"scripts": true}}),
            "`</`",
        ),
        // Bounds the page budget depends on.
        (
            serde_json::json!({"height_px": 4000}),
            "height must be between",
        ),
    ] {
        let error = SectionsEnvelope::from_value(block(patch.clone()))
            .expect_err(&format!("{patch} must be refused"))
            .to_string();
        assert!(
            error.contains(expected),
            "{patch} was refused as {error:?}, expected the {expected:?} rule"
        );
    }

    // And the accepted shape really is accepted, through the same door.
    SectionsEnvelope::from_value(block(serde_json::json!({
        "css": "p { font-size: 3rem; }",
        "js": "document.title;",
        "capabilities": {"scripts": true, "inline_images": true},
    })))
    .expect("a well-formed block must pass the gate");
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

#[test]
fn image_presentation_golden_round_trips_and_carries_crop_focal_and_decorative() {
    let golden: Value = serde_json::from_str(IMAGE_PRESENTATION).unwrap();
    let envelope = SectionsEnvelope::from_value(golden.clone())
        .unwrap_or_else(|e| panic!("image presentation fixture rejected: {e}"));

    let framed = envelope.sections[0].images();
    assert_eq!(framed.len(), 1);
    assert_eq!(
        framed[0].crop,
        Some(ImageCrop {
            x_bp: 1250,
            y_bp: 0,
            width_bp: 7500,
            height_bp: 10_000,
        })
    );
    assert_eq!(
        framed[0].focal,
        Some(ImageFocalPoint {
            x_bp: 4000,
            y_bp: 3500
        })
    );
    assert!(!framed[0].needs_alt_text());

    // The gallery's three images are the three states alt text can be in:
    // written, deliberately decorative, and not yet written.
    let gallery = envelope.sections[1].images();
    assert_eq!(gallery.len(), 3);
    assert!(!gallery[0].needs_alt_text(), "written alt");
    assert!(gallery[0].crop.is_none(), "a focal point needs no crop");
    assert!(!gallery[1].needs_alt_text(), "decorative on purpose");
    assert!(gallery[2].needs_alt_text(), "blank alt, nobody said why");

    let back = envelope.to_value().unwrap();
    assert_eq!(back, golden, "image presentation re-serialization drifted");
}

/// The presentation props are additive: every fixture written before they
/// existed still parses, still re-serializes **without** the new keys (proven
/// byte-for-byte by the golden test above), and reads as the whole image with
/// its centre in frame.
#[test]
fn images_stored_before_the_presentation_props_mean_whole_image_centred() {
    let golden: Value = serde_json::from_str(FULL_PAGE).unwrap();
    let envelope = SectionsEnvelope::from_value(golden).unwrap();
    let legacy: Vec<_> = envelope.sections.iter().flat_map(|s| s.images()).collect();
    assert!(!legacy.is_empty(), "the full page golden carries images");
    for image in legacy {
        assert_eq!(image.crop, None);
        assert_eq!(image.focal, None);
        assert!(!image.decorative);
        assert_eq!(image.crop_or_full(), ImageCrop::full());
        assert_eq!(
            image.focal_or_center(),
            ImageFocalPoint {
                x_bp: 5000,
                y_bp: 5000
            }
        );
    }
}
