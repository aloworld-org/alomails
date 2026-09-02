//! What a new section starts as, drawn from the tenant's own website
//! (ADR 0042 §4, S3.01d) — the palette's seed.
//!
//! ADR 0042 asks for "a palette to drag new sections from, **showing what each
//! one looks like with the tenant's own content rather than lorem ipsum**".
//! This module is the half of that sentence a renderer cannot supply: given
//! what the tenant has already written on this website, it answers, per
//! section type, the section they would get — filled with their own words,
//! their own pictures, their own pages.
//!
//! Three rules decide what a seed may contain, and each is a decision rather
//! than an implementation detail.
//!
//! - **Every string in a seed is one the tenant already wrote.** Not a
//!   translated placeholder, not a shipped example, not "Lorem ipsum": the
//!   site's name, its page titles and paths, its meta descriptions, and the
//!   text of the sections already on it. The module's own test builds that set
//!   out of a [`SeedContext`] and asserts that every string in every seed is a
//!   member of it — so a well-meaning `"Your headline here"` fails the gate
//!   rather than reaching a customer's page.
//! - **A seed never makes a claim only the customer can make.** The same rule
//!   [`crate::site_templates`] holds curated templates to, for the same
//!   reason: no invented testimonial, no invented team member, no invented
//!   price. Where the tenant has written none of those, the palette says so
//!   ([`SectionSeed::NeedsInput`]) instead of inventing one, and the editor
//!   opens the prop form the way it always did.
//! - **Anything the palette offers, the store accepts.** Every
//!   [`SectionSeed::Ready`] section passes the same
//!   [`SectionsEnvelope`](crate::site_model::SectionsEnvelope) gate a save
//!   goes through — proven by a test over every kind — because a palette tile
//!   that produces a 422 on drop is worse than no tile.
//!
//! The seed is a pure function of a context the caller gathers; nothing here
//! reads a database, so the whole vocabulary is golden-testable without a
//! tenant.

use crate::id::{SiteBookingId, SiteCatalogId, SiteCollectionId};
use crate::site_model::{
    BookingSection, CatalogSection, CollectionSection, ContactFormSection, CtaSection, FeatureItem,
    FeaturesSection, FooterSection, GallerySection, HeroSection, ImageSide, Link,
    MAX_LONG_TEXT_CHARS, MAX_SHORT_TEXT_CHARS, NavSection, Section, ShopSection, SiteImage,
    TextImageSection, TicketsSection,
};

/// How many of the site's own pages a seeded menu or footer lists. A nav with
/// every page of a forty-page site is not a menu; the owner trims or extends
/// it, and the first handful in navigation order is the honest guess.
const SEEDED_NAV_LINKS: usize = 8;

/// How many pictures a seeded gallery starts with. Enough to show a grid,
/// few enough that adding one does not dump the tenant's whole image library
/// onto the page.
const SEEDED_GALLERY_IMAGES: usize = 6;

/// How many cards a seeded feature grid starts with, when they are built from
/// the site's own pages rather than copied from an existing grid.
const SEEDED_FEATURE_CARDS: usize = 3;

/// One page of the site, as the seed sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedPage {
    /// The page's title, as the owner named it.
    pub title: String,
    /// Site-relative path — `/` for the home page, `/<slug>` otherwise.
    pub path: String,
    pub is_home: bool,
    /// The page's meta description, when it has one.
    pub description: Option<String>,
}

/// A tenant object a section binds to by id — a catalog, a collection, a
/// bookable service — and the name the tenant gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedBinding {
    pub id: String,
    pub name: String,
}

/// Everything a seed may draw on: this website as its owner has built it so
/// far. The caller gathers it through the tenant-scoped store door, so a seed
/// can never carry another tenant's word.
#[derive(Debug, Clone, Default)]
pub struct SeedContext {
    /// The website's name.
    pub site_name: String,
    /// Its pages, in navigation order.
    pub pages: Vec<SeedPage>,
    /// Every section already on the site, in page order — the tenant's own
    /// writing, and the first place a seed looks.
    pub sections: Vec<Section>,
    /// The catalog a `catalog` section would show, when the tenant has one.
    pub catalog: Option<SeedBinding>,
    /// The collection a `collection` section would show, when there is one.
    pub collection: Option<SeedBinding>,
    /// The service a `booking` section would offer, when there is one.
    pub booking: Option<SeedBinding>,
}

/// Why a section type cannot be seeded from this website yet. The editor turns
/// it into a sentence in the editing language; nothing here is user-visible
/// text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedNeed {
    /// Words only the owner can write — a quote, a price, a person, an answer.
    Writing,
    /// A picture: the site carries no image this section could show.
    Picture,
    /// No catalog exists on this site yet.
    Catalog,
    /// No collection is connected to this site yet.
    Collection,
    /// No bookable service exists on this site yet.
    Booking,
    /// Code, which is never guessed on the owner's behalf.
    Code,
}

impl SeedNeed {
    /// The wire word — the token the editor branches on.
    pub const fn as_str(self) -> &'static str {
        match self {
            SeedNeed::Writing => "writing",
            SeedNeed::Picture => "picture",
            SeedNeed::Catalog => "catalog",
            SeedNeed::Collection => "collection",
            SeedNeed::Booking => "booking",
            SeedNeed::Code => "code",
        }
    }
}

/// What the palette offers for one section type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionSeed {
    /// A section made of the tenant's own content, ready to drop onto the page
    /// exactly as it stands.
    Ready(Box<Section>),
    /// Nothing of the tenant's own fills this type yet; the reason says what
    /// is missing, and the editor falls back to the prop form.
    NeedsInput(SeedNeed),
}

impl SectionSeed {
    /// The section, when this seed is ready.
    pub fn section(&self) -> Option<&Section> {
        match self {
            SectionSeed::Ready(section) => Some(section),
            SectionSeed::NeedsInput(_) => None,
        }
    }

    fn ready(section: Section) -> Self {
        SectionSeed::Ready(Box::new(section))
    }
}

impl SeedContext {
    /// The site's pages as menu links, in navigation order.
    fn page_links(&self) -> Vec<Link> {
        self.pages
            .iter()
            .filter_map(|page| {
                Some(Link {
                    label: short(&page.title)?,
                    href: page.path.clone(),
                })
            })
            .take(SEEDED_NAV_LINKS)
            .collect()
    }

    /// The first page that is not the home page — where a call to action
    /// points when the tenant has not written one.
    fn away_link(&self) -> Option<Link> {
        self.pages
            .iter()
            .find(|page| !page.is_home)
            .and_then(|page| {
                Some(Link {
                    label: short(&page.title)?,
                    href: page.path.clone(),
                })
            })
    }

    /// Every picture already on the site, in document order, each blob once.
    fn images(&self) -> Vec<SiteImage> {
        let mut seen: Vec<&str> = Vec::new();
        let mut images = Vec::new();
        for section in &self.sections {
            for image in section.images() {
                if seen.contains(&image.blob_id.as_str()) {
                    continue;
                }
                seen.push(image.blob_id.as_str());
                images.push(image.clone());
            }
        }
        images
    }

    /// The first section of one kind already on the site — the tenant's own
    /// version of the block being added.
    fn first<'a, T>(&'a self, pick: impl Fn(&'a Section) -> Option<&'a T>) -> Option<&'a T> {
        self.sections.iter().find_map(pick)
    }

    /// A line of the tenant's own prose to open a block with: what they wrote
    /// under their headline, or the description of their home page.
    fn own_line(&self) -> Option<String> {
        self.first(|section| match section {
            Section::Hero(hero) => hero.subheading.as_ref(),
            _ => None,
        })
        .and_then(|line| long(line))
        .or_else(|| {
            self.pages
                .iter()
                .find(|page| page.is_home)
                .and_then(|page| page.description.as_deref())
                .and_then(long)
        })
    }
}

/// A short display string the schema would accept, or `None` — blank and
/// over-long candidates are dropped rather than trimmed, so a seeded string is
/// always exactly a string the tenant wrote.
fn short(value: &str) -> Option<String> {
    bounded(value, MAX_SHORT_TEXT_CHARS)
}

/// The same for a long-form string (a body, a quote, an answer).
fn long(value: &str) -> Option<String> {
    bounded(value, MAX_LONG_TEXT_CHARS)
}

fn bounded(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max_chars {
        return None;
    }
    Some(trimmed.to_owned())
}

/// The seed for section type `kind` (the wire tag: `hero`, `text_image`, …)
/// on this website, or `None` for a type this build does not have.
pub fn seed_section(kind: &str, ctx: &SeedContext) -> Option<SectionSeed> {
    Some(match kind {
        "nav" => seed_nav(ctx),
        "hero" => seed_hero(ctx),
        "features" => seed_features(ctx),
        "text_image" => seed_text_image(ctx),
        "gallery" => seed_gallery(ctx),
        "testimonials" => seed_testimonials(ctx),
        "pricing" => seed_pricing(ctx),
        "team" => seed_team(ctx),
        "faq" => seed_faq(ctx),
        "cta" => seed_cta(ctx),
        "contact_form" => seed_contact_form(ctx),
        "collection" => seed_collection(ctx),
        "catalog" => seed_catalog(ctx),
        "booking" => seed_booking(ctx),
        "tickets" => seed_tickets(ctx),
        "shop" => seed_shop(ctx),
        "transition" => SectionSeed::Ready(Box::new(Section::Transition(
            crate::site_model::TransitionSection {
                effect: crate::site_model::TransitionEffect::Fade,
                direction: crate::site_model::TransitionDirection::Up,
                speed: crate::site_model::TransitionSpeed::Smooth,
                trigger: crate::site_model::TransitionTrigger::Balanced,
                animate_out: false,
            },
        ))),
        // A code block is the one section whose content is not writing but
        // behaviour. Nothing in a website's copy tells us what script the
        // owner meant, and guessing one would put bytes on their page that
        // nobody chose.
        "custom_code" => SectionSeed::NeedsInput(SeedNeed::Code),
        "footer" => seed_footer(ctx),
        _ => return None,
    })
}

/// The menu the site already has, or one built from its own pages.
fn seed_nav(ctx: &SeedContext) -> SectionSeed {
    let existing = ctx.first(|section| match section {
        Section::Nav(nav) => Some(nav),
        _ => None,
    });
    let links = match existing {
        Some(nav) if !nav.links.is_empty() => nav.links.clone(),
        _ => ctx.page_links(),
    };
    SectionSeed::ready(Section::Nav(NavSection {
        links,
        cta: existing.and_then(|nav| nav.cta.clone()),
        appearance: existing.and_then(|nav| nav.appearance.clone()),
    }))
}

/// The banner: the website's own name, the line under it the owner already
/// wrote, and the first picture they uploaded.
fn seed_hero(ctx: &SeedContext) -> SectionSeed {
    let Some(heading) = short(&ctx.site_name) else {
        return SectionSeed::NeedsInput(SeedNeed::Writing);
    };
    let existing = ctx.first(|section| match section {
        Section::Hero(hero) => Some(hero),
        _ => None,
    });
    SectionSeed::ready(Section::Hero(HeroSection {
        heading,
        subheading: ctx.own_line().and_then(|line| short(&line)),
        image: ctx.images().into_iter().next(),
        video_url: existing.and_then(|hero| hero.video_url.clone()),
        primary_cta: existing
            .and_then(|hero| hero.primary_cta.clone())
            .or_else(|| ctx.away_link()),
        secondary_cta: None,
        appearance: existing.and_then(|hero| hero.appearance.clone()),
        layout: existing.and_then(|hero| hero.layout),
        height: existing.and_then(|hero| hero.height),
        alignment: existing.and_then(|hero| hero.alignment),
        content_width: existing.and_then(|hero| hero.content_width),
        text_animation: existing.and_then(|hero| hero.text_animation),
        media_animation: existing.and_then(|hero| hero.media_animation),
        animation_speed: existing.and_then(|hero| hero.animation_speed),
    }))
}

/// The feature grid the site already has, or one card per page from the pages
/// whose descriptions the owner has written.
fn seed_features(ctx: &SeedContext) -> SectionSeed {
    if let Some(existing) = ctx.first(|section| match section {
        Section::Features(features) => Some(features),
        _ => None,
    }) {
        return SectionSeed::ready(Section::Features(existing.clone()));
    }
    let items: Vec<_> = ctx
        .pages
        .iter()
        .filter_map(|page| {
            Some(FeatureItem {
                title: short(&page.title)?,
                body: page.description.as_deref().and_then(long)?,
                icon: None,
            })
        })
        .take(SEEDED_FEATURE_CARDS)
        .collect();
    if items.is_empty() {
        return SectionSeed::NeedsInput(SeedNeed::Writing);
    }
    SectionSeed::ready(Section::Features(FeaturesSection {
        heading: None,
        intro: None,
        items,
        columns: None,
        layout: None,
        presentation: None,
    }))
}

/// Words beside a picture — both of them the tenant's own, or neither.
fn seed_text_image(ctx: &SeedContext) -> SectionSeed {
    let Some(image) = ctx.images().into_iter().next() else {
        return SectionSeed::NeedsInput(SeedNeed::Picture);
    };
    let existing = ctx.first(|section| match section {
        Section::TextImage(text_image) => Some(text_image),
        _ => None,
    });
    let body = existing
        .and_then(|text_image| long(&text_image.body))
        .or_else(|| ctx.own_line());
    let Some(body) = body else {
        return SectionSeed::NeedsInput(SeedNeed::Writing);
    };
    SectionSeed::ready(Section::TextImage(TextImageSection {
        heading: existing.and_then(|text_image| text_image.heading.clone()),
        body,
        image,
        image_side: ImageSide::Left,
        split: None,
        layout: None,
        presentation: None,
    }))
}

/// The tenant's own pictures, as many as a grid shows at once.
fn seed_gallery(ctx: &SeedContext) -> SectionSeed {
    let images: Vec<_> = ctx
        .images()
        .into_iter()
        .take(SEEDED_GALLERY_IMAGES)
        .collect();
    if images.is_empty() {
        return SectionSeed::NeedsInput(SeedNeed::Picture);
    }
    SectionSeed::ready(Section::Gallery(GallerySection {
        heading: None,
        images,
        columns: None,
        layout: None,
        presentation: None,
    }))
}

/// Quotes are a claim only the customer can make: copied from theirs, or asked
/// for.
fn seed_testimonials(ctx: &SeedContext) -> SectionSeed {
    ctx.first(|section| match section {
        Section::Testimonials(testimonials) => Some(testimonials),
        _ => None,
    })
    .map_or(SectionSeed::NeedsInput(SeedNeed::Writing), |existing| {
        SectionSeed::ready(Section::Testimonials(existing.clone()))
    })
}

/// So is a price.
fn seed_pricing(ctx: &SeedContext) -> SectionSeed {
    ctx.first(|section| match section {
        Section::Pricing(pricing) => Some(pricing),
        _ => None,
    })
    .map_or(SectionSeed::NeedsInput(SeedNeed::Writing), |existing| {
        SectionSeed::ready(Section::Pricing(existing.clone()))
    })
}

/// And so is who works there.
fn seed_team(ctx: &SeedContext) -> SectionSeed {
    ctx.first(|section| match section {
        Section::Team(team) => Some(team),
        _ => None,
    })
    .map_or(SectionSeed::NeedsInput(SeedNeed::Writing), |existing| {
        SectionSeed::ready(Section::Team(existing.clone()))
    })
}

/// And what the answers are.
fn seed_faq(ctx: &SeedContext) -> SectionSeed {
    ctx.first(|section| match section {
        Section::Faq(faq) => Some(faq),
        _ => None,
    })
    .map_or(SectionSeed::NeedsInput(SeedNeed::Writing), |existing| {
        SectionSeed::ready(Section::Faq(existing.clone()))
    })
}

/// The banner that asks for the next step: the one the site already has, or
/// the website's name over a button to one of its own pages.
fn seed_cta(ctx: &SeedContext) -> SectionSeed {
    if let Some(existing) = ctx.first(|section| match section {
        Section::Cta(cta) => Some(cta),
        _ => None,
    }) {
        return SectionSeed::ready(Section::Cta(existing.clone()));
    }
    match (short(&ctx.site_name), ctx.away_link()) {
        (Some(heading), Some(button)) => SectionSeed::ready(Section::Cta(CtaSection {
            heading,
            body: None,
            button,
            secondary_button: None,
            layout: None,
            presentation: None,
        })),
        _ => SectionSeed::NeedsInput(SeedNeed::Writing),
    }
}

/// A contact form always works: every one of its props is optional, and the
/// form record itself is created by the write path when the section lands
/// without one (S1.16c2). The seed deliberately does NOT copy an existing
/// section's `form_id` — two sections pointing at one form would file two
/// pages' submissions into a single inbox.
fn seed_contact_form(ctx: &SeedContext) -> SectionSeed {
    let existing = ctx.first(|section| match section {
        Section::ContactForm(form) => Some(form),
        _ => None,
    });
    SectionSeed::ready(Section::ContactForm(ContactFormSection {
        heading: existing.and_then(|form| form.heading.clone()),
        body: existing.and_then(|form| form.body.clone()),
        form_id: None,
        success_message: existing.and_then(|form| form.success_message.clone()),
        presentation: existing.and_then(|form| form.presentation.clone()),
    }))
}

fn seed_collection(ctx: &SeedContext) -> SectionSeed {
    ctx.collection
        .as_ref()
        .map_or(SectionSeed::NeedsInput(SeedNeed::Collection), |binding| {
            SectionSeed::ready(Section::Collection(CollectionSection {
                collection_id: SiteCollectionId::new(binding.id.clone()),
                heading: short(&binding.name),
                presentation: None,
            }))
        })
}

fn seed_catalog(ctx: &SeedContext) -> SectionSeed {
    ctx.catalog
        .as_ref()
        .map_or(SectionSeed::NeedsInput(SeedNeed::Catalog), |binding| {
            SectionSeed::ready(Section::Catalog(CatalogSection {
                catalog_id: SiteCatalogId::new(binding.id.clone()),
                heading: short(&binding.name),
                category: None,
                presentation: None,
            }))
        })
}

fn seed_booking(ctx: &SeedContext) -> SectionSeed {
    ctx.booking
        .as_ref()
        .map_or(SectionSeed::NeedsInput(SeedNeed::Booking), |binding| {
            SectionSeed::ready(Section::Booking(BookingSection {
                booking_id: SiteBookingId::new(binding.id.clone()),
                heading: short(&binding.name),
                presentation: None,
            }))
        })
}

/// The ticket-shop door always works: both props are optional, and what the
/// link leads to is the site's own live shop. Any words come from an existing
/// tickets section, never from us.
fn seed_tickets(ctx: &SeedContext) -> SectionSeed {
    let existing = ctx.first(|section| match section {
        Section::Tickets(tickets) => Some(tickets),
        _ => None,
    });
    SectionSeed::ready(Section::Tickets(TicketsSection {
        heading: existing.and_then(|tickets| tickets.heading.clone()),
        body: existing.and_then(|tickets| tickets.body.clone()),
        presentation: existing.and_then(|tickets| tickets.presentation.clone()),
    }))
}

/// The stock-shop door works the same way: both props are optional, the link
/// leads to the site's own live shop, and any words come from an existing
/// shop section, never from us.
fn seed_shop(ctx: &SeedContext) -> SectionSeed {
    let existing = ctx.first(|section| match section {
        Section::Shop(shop) => Some(shop),
        _ => None,
    });
    SectionSeed::ready(Section::Shop(ShopSection {
        heading: existing.and_then(|shop| shop.heading.clone()),
        body: existing.and_then(|shop| shop.body.clone()),
        presentation: existing.and_then(|shop| shop.presentation.clone()),
    }))
}

/// The foot of the page: the line the site already carries, over its own
/// pages.
fn seed_footer(ctx: &SeedContext) -> SectionSeed {
    let existing = ctx.first(|section| match section {
        Section::Footer(footer) => Some(footer),
        _ => None,
    });
    let links = match existing {
        Some(footer) if !footer.links.is_empty() => footer.links.clone(),
        _ => ctx.page_links(),
    };
    SectionSeed::ready(Section::Footer(FooterSection {
        text: existing.and_then(|footer| footer.text.clone()),
        links,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::*;
    use crate::id::BlobId;
    use crate::site_model::{
        FaqItem, FaqSection, HeroAnimationSpeed, HeroLayout, HeroMediaAnimation, HeroTextAnimation,
        PricingSection, PricingTier, SECTION_KINDS, SectionsEnvelope, TeamMember, TeamSection,
        Testimonial, TestimonialsSection,
    };

    /// Keys whose values are the schema's own closed vocabulary rather than
    /// anybody's writing — the section tag, which side an image sits on, which
    /// of its declared shapes it was resized to. They are excluded from the
    /// "did this come from the tenant" check because they are words the build
    /// ships, and there are only ever finitely many of them.
    const VOCABULARY_KEYS: &[&str] = &[
        "type",
        "image_side",
        "split",
        "columns",
        "shape",
        "decorative",
        "highlighted",
        "schema_version",
    ];

    fn image(blob: &str, alt: &str) -> SiteImage {
        SiteImage::new(BlobId::new(blob), alt)
    }

    fn link(label: &str, href: &str) -> Link {
        Link {
            label: label.to_owned(),
            href: href.to_owned(),
        }
    }

    /// A brand-new website: a name and a home page, and not one word written.
    fn blank_site() -> SeedContext {
        SeedContext {
            site_name: "Nordwind Coffee Roasters".to_owned(),
            pages: vec![SeedPage {
                title: "Home".to_owned(),
                path: "/".to_owned(),
                is_home: true,
                description: None,
            }],
            ..SeedContext::default()
        }
    }

    /// A website its owner has actually written: pages with descriptions, a
    /// hero, a picture, quotes, prices, people, answers — and a catalog.
    fn written_site() -> SeedContext {
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
                    title: "Visit us".to_owned(),
                    path: "/visit".to_owned(),
                    is_home: false,
                    description: Some("Open every day from seven.".to_owned()),
                },
            ],
            sections: vec![
                Section::Hero(HeroSection {
                    heading: "Coffee roasted the morning it ships".to_owned(),
                    subheading: Some("Small-batch roastery on the harbour".to_owned()),
                    image: Some(image("9hK3vQ2mR8pT1xWz4bC5dg", "Roasting drum mid-batch")),
                    video_url: Some("https://media.example/roastery.webm".to_owned()),
                    primary_cta: Some(link("Visit us", "/visit")),
                    secondary_cta: None,
                    appearance: None,
                    layout: Some(HeroLayout::VideoBackground),
                    height: None,
                    alignment: None,
                    content_width: None,
                    text_animation: Some(HeroTextAnimation::WordReveal),
                    media_animation: Some(HeroMediaAnimation::SlowZoom),
                    animation_speed: Some(HeroAnimationSpeed::Smooth),
                }),
                Section::Testimonials(TestimonialsSection {
                    heading: Some("What the neighbourhood says".to_owned()),
                    items: vec![Testimonial {
                        quote: "The only beans I buy.".to_owned(),
                        author: "Ines Kortekaas".to_owned(),
                        role: Some("Regular since 2019".to_owned()),
                    }],
                    layout: None,
                    presentation: None,
                }),
                Section::Pricing(PricingSection {
                    heading: Some("Subscriptions".to_owned()),
                    intro: None,
                    tiers: vec![PricingTier {
                        name: "Every fortnight".to_owned(),
                        price: "€18".to_owned(),
                        period: Some("per delivery".to_owned()),
                        description: None,
                        features: vec!["250g, roasted to order".to_owned()],
                        cta: Some(link("Subscribe", "/visit")),
                        highlighted: false,
                    }],
                    layout: None,
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
                    layout: None,
                    presentation: None,
                }),
                Section::Faq(FaqSection {
                    heading: None,
                    items: vec![FaqItem {
                        question: "Do you ship abroad?".to_owned(),
                        answer: "Anywhere in the EU, in two days.".to_owned(),
                    }],
                    layout: None,
                    presentation: None,
                }),
                Section::ContactForm(ContactFormSection {
                    heading: Some("Say hello".to_owned()),
                    body: None,
                    form_id: Some("frm-existing".to_owned()),
                    success_message: Some("We answer within a day.".to_owned()),
                    presentation: None,
                }),
            ],
            catalog: Some(SeedBinding {
                id: "cat-1".to_owned(),
                name: "The bar".to_owned(),
            }),
            collection: None,
            booking: None,
        }
    }

    /// Every seed of a context, in palette order.
    fn seeds(ctx: &SeedContext) -> Vec<(&'static str, SectionSeed)> {
        SECTION_KINDS
            .iter()
            .map(|kind| (*kind, seed_section(kind, ctx).unwrap()))
            .collect()
    }

    /// Every string in a JSON value that is somebody's writing rather than the
    /// schema's own vocabulary.
    fn words(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::String(text) => {
                out.insert(text.clone());
            }
            Value::Array(entries) => {
                for entry in entries {
                    words(entry, out);
                }
            }
            Value::Object(fields) => {
                for (key, field) in fields {
                    if VOCABULARY_KEYS.contains(&key.as_str()) {
                        continue;
                    }
                    words(field, out);
                }
            }
            _ => {}
        }
    }

    /// Everything this tenant has written on this website — the only source a
    /// seed may draw a string from.
    fn corpus(ctx: &SeedContext) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        out.insert(ctx.site_name.clone());
        for page in &ctx.pages {
            out.insert(page.title.clone());
            out.insert(page.path.clone());
            if let Some(description) = &page.description {
                out.insert(description.clone());
            }
        }
        for section in &ctx.sections {
            words(&serde_json::to_value(section).unwrap(), &mut out);
        }
        for binding in [&ctx.catalog, &ctx.collection, &ctx.booking]
            .into_iter()
            .flatten()
        {
            out.insert(binding.id.clone());
            out.insert(binding.name.clone());
        }
        out
    }

    #[test]
    fn every_section_type_has_a_seed_and_nothing_else_does() {
        for kind in SECTION_KINDS {
            assert!(
                seed_section(kind, &written_site()).is_some(),
                "no seed for {kind}"
            );
        }
        assert!(seed_section("parallax_hero", &written_site()).is_none());
        assert!(seed_section("", &written_site()).is_none());
    }

    /// The palette's promise: what it offers, the store takes. A tile that
    /// produced a 422 on drop would be worse than no tile at all.
    #[test]
    fn every_ready_seed_is_a_section_the_store_accepts() {
        for ctx in [written_site(), blank_site()] {
            let sections: Vec<Section> = seeds(&ctx)
                .into_iter()
                .filter_map(|(_, seed)| seed.section().cloned())
                .collect();
            assert!(!sections.is_empty());
            let envelope = SectionsEnvelope {
                schema_version: crate::site_model::SECTIONS_SCHEMA_VERSION,
                sections,
            };
            SectionsEnvelope::from_value(envelope.to_value().unwrap())
                .expect("every seeded section passes the write gate");
        }
    }

    /// The rule ADR 0042 asks for in four words: the tenant's own content
    /// rather than lorem ipsum. Every string a seed carries is one this
    /// website already held.
    #[test]
    fn nothing_in_a_seed_is_invented() {
        for ctx in [written_site(), blank_site()] {
            let known = corpus(&ctx);
            for (kind, seed) in seeds(&ctx) {
                let Some(section) = seed.section() else {
                    continue;
                };
                let mut used = BTreeSet::new();
                words(&serde_json::to_value(section).unwrap(), &mut used);
                for word in used {
                    assert!(
                        known.contains(&word),
                        "the {kind} seed invented {word:?}; every word must be the tenant's"
                    );
                }
            }
        }
    }

    /// A website with nothing on it yet cannot be given quotes, prices, people
    /// or answers — those are claims only its owner can make — nor pictures it
    /// has not uploaded. It says so, per type, rather than inventing them.
    #[test]
    fn a_new_site_asks_rather_than_invents() {
        let ctx = blank_site();
        let need = |kind: &str| match seed_section(kind, &ctx).unwrap() {
            SectionSeed::NeedsInput(need) => Some(need),
            SectionSeed::Ready(_) => None,
        };
        assert_eq!(need("testimonials"), Some(SeedNeed::Writing));
        assert_eq!(need("pricing"), Some(SeedNeed::Writing));
        assert_eq!(need("team"), Some(SeedNeed::Writing));
        assert_eq!(need("faq"), Some(SeedNeed::Writing));
        assert_eq!(need("features"), Some(SeedNeed::Writing));
        assert_eq!(need("gallery"), Some(SeedNeed::Picture));
        assert_eq!(need("text_image"), Some(SeedNeed::Picture));
        assert_eq!(need("catalog"), Some(SeedNeed::Catalog));
        assert_eq!(need("collection"), Some(SeedNeed::Collection));
        assert_eq!(need("booking"), Some(SeedNeed::Booking));
        assert_eq!(need("custom_code"), Some(SeedNeed::Code));
        // A CTA needs somewhere to send the visitor, and a one-page site has
        // nowhere yet.
        assert_eq!(need("cta"), Some(SeedNeed::Writing));
        // What a site always has: its name, its pages, and a way to be
        // written to.
        for kind in ["nav", "hero", "contact_form", "footer"] {
            assert_eq!(need(kind), None, "{kind} should be ready on a new site");
        }
    }

    /// The frame of a page is built from the pages themselves, so a menu
    /// dropped onto a new site is already the site's menu.
    #[test]
    fn the_frame_is_built_from_the_sites_own_pages() {
        let ctx = written_site();
        let Some(Section::Nav(nav)) = seed_section("nav", &ctx).unwrap().section().cloned() else {
            panic!("nav seed is not a nav");
        };
        assert_eq!(
            nav.links,
            vec![link("Home", "/"), link("Visit us", "/visit")]
        );
        let Some(Section::Footer(footer)) =
            seed_section("footer", &ctx).unwrap().section().cloned()
        else {
            panic!("footer seed is not a footer");
        };
        assert_eq!(footer.links, nav.links);
        assert_eq!(footer.text, None);
    }

    /// The banner is the site's own name over its own line and its own
    /// picture — not the headline of the hero it already has, which would drop
    /// a duplicate headline onto the page.
    #[test]
    fn the_banner_is_the_websites_own_name() {
        let ctx = written_site();
        let Some(Section::Hero(hero)) = seed_section("hero", &ctx).unwrap().section().cloned()
        else {
            panic!("hero seed is not a hero");
        };
        assert_eq!(hero.heading, "Nordwind Coffee Roasters");
        assert_eq!(
            hero.subheading.as_deref(),
            Some("Small-batch roastery on the harbour")
        );
        assert_eq!(
            hero.image.map(|i| i.blob_id.as_str().to_owned()),
            Some("9hK3vQ2mR8pT1xWz4bC5dg".to_owned())
        );
        assert_eq!(hero.primary_cta, Some(link("Visit us", "/visit")));
        assert_eq!(
            hero.video_url.as_deref(),
            Some("https://media.example/roastery.webm")
        );
        assert_eq!(hero.layout, Some(HeroLayout::VideoBackground));
        assert_eq!(hero.text_animation, Some(HeroTextAnimation::WordReveal));
        assert_eq!(hero.media_animation, Some(HeroMediaAnimation::SlowZoom));
        assert_eq!(hero.animation_speed, Some(HeroAnimationSpeed::Smooth));
    }

    /// Pictures come from the site, each blob once, in the order it uses them
    /// — including the ones inside a team member's portrait.
    #[test]
    fn pictures_come_from_the_site() {
        let ctx = written_site();
        let Some(Section::Gallery(gallery)) =
            seed_section("gallery", &ctx).unwrap().section().cloned()
        else {
            panic!("gallery seed is not a gallery");
        };
        let blobs: Vec<&str> = gallery
            .images
            .iter()
            .map(|image| image.blob_id.as_str())
            .collect();
        assert_eq!(blobs, ["9hK3vQ2mR8pT1xWz4bC5dg", "2wQ8xL4nV6yB0aC7dE9fgh"]);
        // The alt text rides along: a seeded picture is never one nobody has
        // described.
        assert_eq!(gallery.images[0].alt, "Roasting drum mid-batch");
    }

    /// Two contact sections must never share one form, or two pages' messages
    /// land in a single inbox. The seed copies the words and leaves the
    /// binding to the write path.
    #[test]
    fn a_seeded_contact_form_never_borrows_another_sections_form() {
        let ctx = written_site();
        let Some(Section::ContactForm(form)) = seed_section("contact_form", &ctx)
            .unwrap()
            .section()
            .cloned()
        else {
            panic!("contact form seed is not a contact form");
        };
        assert_eq!(form.form_id, None);
        assert_eq!(form.heading.as_deref(), Some("Say hello"));
        assert_eq!(
            form.success_message.as_deref(),
            Some("We answer within a day.")
        );
    }

    /// A written site's quotes, prices, people and answers are its own, so
    /// adding a second block of them starts from what is already true.
    #[test]
    fn a_written_site_seeds_its_own_claims() {
        let ctx = written_site();
        for kind in ["testimonials", "pricing", "team", "faq"] {
            assert!(
                seed_section(kind, &ctx).unwrap().section().is_some(),
                "{kind} should seed from the site's own content"
            );
        }
        let seeded = seed_section("catalog", &ctx).unwrap();
        let Some(Section::Catalog(catalog)) = seeded.section() else {
            panic!("catalog seed is not a catalog");
        };
        assert_eq!(catalog.catalog_id.as_str(), "cat-1");
        assert_eq!(catalog.heading.as_deref(), Some("The bar"));
    }

    /// A string the schema would refuse is not a string the palette offers:
    /// an over-long site name leaves the banner asking rather than producing a
    /// section the save would reject.
    #[test]
    fn a_string_the_schema_would_refuse_is_never_seeded() {
        let ctx = SeedContext {
            site_name: "n".repeat(MAX_SHORT_TEXT_CHARS + 1),
            ..blank_site()
        };
        assert_eq!(
            seed_section("hero", &ctx).unwrap(),
            SectionSeed::NeedsInput(SeedNeed::Writing)
        );
    }
}
