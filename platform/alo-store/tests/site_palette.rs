//! The palette's goldens (ADR 0042 §4, S3.01d): what a whole section palette
//! offers on one fixed website, pinned as JSON.
//!
//! The unit tests beside [`alo_store::site_seed`] prove the *rules* — nothing
//! invented, everything the store accepts, a claim only the owner can make is
//! asked for rather than guessed. This file pins the *result*: the exact
//! sixteen tiles a tenant with this content is offered, so a change to any
//! seeding rule shows up as a reviewable diff instead of as a page that
//! quietly starts carrying different words.
//!
//! Run with `UPDATE_GOLDENS=1` to re-bless after a deliberate change, then
//! read the diff like any other code change. Pure test; no database.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use alo_store::id::BlobId;
use alo_store::site_model::{
    ContactFormSection, FaqItem, FaqSection, FeatureItem, FeaturesSection, GallerySection,
    HeroSection, Link, NavSection, SECTION_KINDS, Section, SiteImage, TeamMember, TeamSection,
    Testimonial, TestimonialsSection,
};
use alo_store::{SectionSeed, SeedBinding, SeedContext, SeedPage, seed_section};
use serde_json::{Value, json};

fn image(blob: &str, alt: &str) -> SiteImage {
    SiteImage::new(BlobId::new(blob), alt)
}

fn link(label: &str, href: &str) -> Link {
    Link {
        label: label.to_owned(),
        href: href.to_owned(),
    }
}

/// One tenant's website, mid-build: three pages, a written home page, a hero,
/// a picture in two places, quotes, people, answers — and a catalog, but no
/// collection and no bookable service, so the goldens carry both a tile that
/// is ready and a tile that says what is missing.
fn fixture_site() -> SeedContext {
    SeedContext {
        site_name: "Nordwind Coffee Roasters".to_owned(),
        pages: vec![
            SeedPage {
                title: "Home".to_owned(),
                path: "/".to_owned(),
                is_home: true,
                description: Some("Small-batch roastery on the harbour.".to_owned()),
            },
            SeedPage {
                title: "The roastery".to_owned(),
                path: "/roastery".to_owned(),
                is_home: false,
                description: Some("A 1962 Probat drum, rebuilt by hand.".to_owned()),
            },
            SeedPage {
                title: "Visit us".to_owned(),
                path: "/visit".to_owned(),
                is_home: false,
                description: None,
            },
        ],
        sections: vec![
            Section::Nav(NavSection {
                links: vec![link("Home", "/"), link("Visit us", "/visit")],
                cta: Some(link("Order beans", "/visit")),
                appearance: None,
            }),
            Section::Hero(HeroSection {
                heading: "Coffee roasted the morning it ships".to_owned(),
                subheading: Some("Small-batch roastery on the harbour".to_owned()),
                image: Some(image("9hK3vQ2mR8pT1xWz4bC5dg", "Roasting drum mid-batch")),
                video_url: None,
                primary_cta: Some(link("Visit us", "/visit")),
                secondary_cta: None,
                appearance: None,
                layout: None,
                height: None,
                alignment: None,
                content_width: None,
                text_animation: None,
                media_animation: None,
                animation_speed: None,
            }),
            Section::Features(FeaturesSection {
                heading: Some("Why Nordwind".to_owned()),
                intro: None,
                items: vec![FeatureItem {
                    title: "Roasted to order".to_owned(),
                    body: "Your batch goes in the drum after you order.".to_owned(),
                    icon: Some("flame".to_owned()),
                }],
                columns: None,
                layout: None,
                presentation: None,
            }),
            Section::Gallery(GallerySection {
                heading: None,
                images: vec![image(
                    "4tR7yU1iO3pA5sD8fGhJkl",
                    "The counter at opening time",
                )],
                columns: None,
                layout: None,
                presentation: None,
            }),
            Section::Testimonials(TestimonialsSection {
                heading: Some("What the neighbourhood says".to_owned()),
                items: vec![Testimonial {
                    quote: "The only beans I buy.".to_owned(),
                    author: "Ines Kortekaas".to_owned(),
                    role: Some("Regular since 2019".to_owned()),
                }],
                presentation: None,
            }),
            Section::Team(TeamSection {
                heading: Some("Behind the drum".to_owned()),
                members: vec![TeamMember {
                    name: "Jonas Weber".to_owned(),
                    role: Some("Roaster".to_owned()),
                    photo: Some(image("2wQ8xL4nV6yB0aC7dE9fgh", "Jonas at the roaster")),
                    bio: None,
                }],
                columns: None,
                presentation: None,
            }),
            Section::Faq(FaqSection {
                heading: None,
                items: vec![FaqItem {
                    question: "Do you ship abroad?".to_owned(),
                    answer: "Anywhere in the EU, in two days.".to_owned(),
                }],
                presentation: None,
            }),
            Section::ContactForm(ContactFormSection {
                heading: Some("Say hello".to_owned()),
                body: None,
                form_id: Some("frmExisting1234567890".to_owned()),
                success_message: Some("We answer within a day.".to_owned()),
                presentation: None,
            }),
        ],
        catalog: Some(SeedBinding {
            id: "catBarMenu1234567890".to_owned(),
            name: "The bar".to_owned(),
        }),
        collection: None,
        booking: None,
    }
}

/// The palette as the edit API serves it: one entry per section type, in the
/// order the editor shows them.
fn palette(ctx: &SeedContext) -> Value {
    Value::Array(
        SECTION_KINDS
            .iter()
            .map(
                |kind| match seed_section(kind, ctx).expect("every kind seeds") {
                    SectionSeed::Ready(section) => json!({
                        "kind": kind,
                        "ready": true,
                        "section": serde_json::to_value(&*section).unwrap(),
                    }),
                    SectionSeed::NeedsInput(need) => json!({
                        "kind": kind,
                        "ready": false,
                        "needs": need.as_str(),
                    }),
                },
            )
            .collect::<Vec<_>>(),
    )
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn assert_golden(name: &str, actual: &Value) {
    let rendered = format!("{}\n", serde_json::to_string_pretty(actual).unwrap());
    let path = golden_path(name);
    if std::env::var("UPDATE_GOLDENS").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &rendered).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {name}; run with UPDATE_GOLDENS=1"));
    assert_eq!(
        rendered, expected,
        "{name} drifted; re-bless with UPDATE_GOLDENS=1 and review the diff"
    );
}

#[test]
fn the_palette_of_a_written_website_is_pinned() {
    assert_golden("site_palette_written.json", &palette(&fixture_site()));
}

#[test]
fn the_palette_of_a_brand_new_website_is_pinned() {
    let ctx = SeedContext {
        site_name: "Nordwind Coffee Roasters".to_owned(),
        pages: vec![SeedPage {
            title: "Home".to_owned(),
            path: "/".to_owned(),
            is_home: true,
            description: None,
        }],
        ..SeedContext::default()
    };
    assert_golden("site_palette_new.json", &palette(&ctx));
}

/// The goldens are only worth what the schema says they are: every section a
/// pinned palette offers must still be one a save would accept.
#[test]
fn every_pinned_section_still_passes_the_write_gate() {
    for ctx in [fixture_site(), SeedContext::default()] {
        for kind in SECTION_KINDS {
            let Some(SectionSeed::Ready(section)) = seed_section(kind, &ctx) else {
                continue;
            };
            let envelope = alo_store::SectionsEnvelope {
                schema_version: alo_store::SECTIONS_SCHEMA_VERSION,
                sections: vec![*section],
            };
            alo_store::SectionsEnvelope::from_value(envelope.to_value().unwrap()).unwrap_or_else(
                |error| panic!("the {kind} tile offers a refused section: {error}"),
            );
        }
    }
}
