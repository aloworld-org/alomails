//! The shipped site-template catalog: that every template in the JSON is
//! actually offered, that the catalog rules hold, and that instantiating one
//! is deterministic, unpublished and private to the tenant that did it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;

use alo_store::{Section, SiteTemplate, check_template, site_template, site_templates};
use serde_json::Value;

use common::test_store;

/// The same bytes the crate embeds, read here so the test can compare what the
/// file declares against what the loader offers.
const CATALOG_JSON: &str = include_str!("../src/site_templates/catalog.json");

fn declared_ids() -> Vec<String> {
    let catalog: Value = serde_json::from_str(CATALOG_JSON).unwrap();
    catalog["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|template| template["id"].as_str().unwrap().to_owned())
        .collect()
}

/// A template dropped by the loader is invisible in production and silent in
/// the log nobody reads; here it is a failing gate.
#[test]
fn every_declared_template_is_offered_in_order() {
    let declared = declared_ids();
    assert!(!declared.is_empty(), "the catalog ships templates");
    let offered: Vec<String> = site_templates()
        .iter()
        .map(|template| template.id.clone())
        .collect();
    assert_eq!(
        offered, declared,
        "every declared template survives loading"
    );
    for template in site_templates() {
        assert_eq!(check_template(template), Ok(()), "{} is valid", template.id);
        assert!(site_template(&template.id).is_some());
    }
    assert!(site_template("no-such-template").is_none());
}

/// The product rules of the catalog, asserted per template rather than trusted
/// to review: a template is a whole small site, it can be published as it
/// stands, and it makes no claim on the tenant's behalf.
#[test]
fn every_template_is_a_complete_honest_site() {
    for template in site_templates() {
        let id = &template.id;
        assert!(template.pages.len() >= 2, "{id} has more than a home page");
        assert!(template.home().is_some(), "{id} has a home page");
        assert_eq!(
            template.theme().preset,
            template.theme_preset,
            "{id} starts on its own preset"
        );

        let paths: BTreeSet<String> = template.page_paths().into_iter().collect();
        assert_eq!(paths.len(), template.pages.len(), "{id} page paths differ");
        assert!(paths.contains("/"), "{id} serves a home path");

        let mut contact_forms = 0_usize;
        for page in &template.pages {
            let kinds: Vec<&str> = page.sections.sections.iter().map(Section::kind).collect();
            assert_eq!(
                kinds.first(),
                Some(&"nav"),
                "{id}/{} opens with navigation",
                page.slug
            );
            assert_eq!(
                kinds.last(),
                Some(&"footer"),
                "{id}/{} ends with a footer",
                page.slug
            );
            for section in &page.sections.sections {
                assert!(
                    section.images().is_empty(),
                    "{id} ships no image it cannot own"
                );
                assert!(
                    !matches!(
                        section,
                        Section::Testimonials(_) | Section::Team(_) | Section::Collection(_)
                    ),
                    "{id} makes no claim only the customer can make"
                );
                if let Section::Pricing(pricing) = section {
                    for tier in &pricing.tiers {
                        assert_eq!(
                            tier.price,
                            alo_store::TEMPLATE_PLACEHOLDER_PRICE,
                            "{id} leaves the price to the owner"
                        );
                    }
                }
                if let Section::ContactForm(form) = section {
                    assert!(form.form_id.is_none(), "{id} ships no form binding");
                    contact_forms += 1;
                }
            }
        }
        assert_eq!(
            contact_forms, 1,
            "{id} offers exactly one way to be written to"
        );
    }
}

/// The loader is the gate, not a formality: each rule refuses a template that
/// breaks it.
#[test]
fn the_catalog_rules_refuse_a_broken_template() {
    let sound = site_templates().first().unwrap();

    let mut unknown_preset = sound.clone();
    unknown_preset.theme_preset = "not-a-preset".to_owned();
    assert!(check_template(&unknown_preset).is_err());

    let mut two_homes: SiteTemplate = sound.clone();
    for page in &mut two_homes.pages {
        page.is_home = true;
        page.slug = String::new();
    }
    assert!(check_template(&two_homes).is_err());

    let mut dead_link = sound.clone();
    dead_link.pages.retain(|page| page.is_home);
    assert!(
        check_template(&dead_link).is_err(),
        "removing a page makes the menu links dead, and that is refused"
    );

    let mut bad_id = sound.clone();
    bad_id.id = "Not A Token".to_owned();
    assert!(check_template(&bad_id).is_err());

    let mut no_version = sound.clone();
    no_version.version = 0;
    assert!(check_template(&no_version).is_err());
}

/// Instantiating a template: the draft is exactly the curated content, the
/// contact form is linked, nothing is published, another tenant sees none of
/// it, and doing it twice produces identical pages.
#[tokio::test]
async fn instantiating_a_template_is_deterministic_and_tenant_private() {
    let store = test_store().await;
    let tenant_a = store.create_tenant("template-a").await.unwrap();
    let tenant_b = store.create_tenant("template-b").await.unwrap();
    let user_a = store
        .for_tenant(tenant_a.clone())
        .create_user("template-a@example.test")
        .await
        .unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("template-b@example.test")
        .await
        .unwrap();
    let a = store.for_account(tenant_a, user_a);
    let b = store.for_account(tenant_b, user_b);

    let template = site_template("consultancy").expect("the consultancy template is shipped");
    let first = a
        .create_generated_site(template.draft("Nordic Advisers".to_owned(), unique_subdomain()))
        .await
        .unwrap();

    let site = a.site(&first.site).await.unwrap().unwrap();
    assert_eq!(site.name, "Nordic Advisers");
    assert_eq!(site.status.as_str(), "draft", "a template never publishes");
    assert_eq!(
        site.theme["preset"],
        Value::from(template.theme_preset.clone())
    );

    let pages = a.site_pages(&first.site).await.unwrap();
    assert_eq!(pages.len(), template.pages.len());
    for (page, curated) in pages.iter().zip(&template.pages) {
        assert_eq!(page.title, curated.title);
        assert_eq!(page.slug, curated.slug);
        assert_eq!(page.is_home, curated.is_home);
        assert_eq!(page.seo_description, curated.seo_description);
    }
    assert!(pages[0].is_home, "the editor opens on the home page");

    // The one contact form is created and linked into the section that asked
    // for it, so a visitor's message has somewhere to land from the first
    // publish.
    let forms = a.site_forms(&first.site).await.unwrap();
    assert_eq!(forms.len(), 1);
    let linked = form_ids(&pages);
    assert_eq!(linked, vec![forms[0].id.as_str().to_owned()]);

    // Wrong tenant: the site, its pages and its form are all invisible.
    assert!(b.site(&first.site).await.unwrap().is_none());
    assert!(b.site_pages(&first.site).await.unwrap().is_empty());
    assert!(b.site_forms(&first.site).await.unwrap().is_empty());
    assert!(
        b.site_page(&first.site, &first.pages[0])
            .await
            .unwrap()
            .is_none()
    );

    // Deterministic: the same template gives the same content twice over, ids
    // aside.
    let second = a
        .create_generated_site(template.draft("Second Site".to_owned(), unique_subdomain()))
        .await
        .unwrap();
    let second_pages = a.site_pages(&second.site).await.unwrap();
    assert_eq!(
        blank_form_ids(&pages),
        blank_form_ids(&second_pages),
        "two instantiations differ only by the ids they mint"
    );
    assert_ne!(
        form_ids(&pages),
        form_ids(&second_pages),
        "each site owns its own form"
    );
}

/// A subdomain no other test run has claimed (the catalog templates carry no
/// subdomain of their own — the owner names the site).
fn unique_subdomain() -> String {
    let raw = format!("tpl-{}", alo_store::SiteId::generate()).to_ascii_lowercase();
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(40)
        .collect()
}

fn form_ids(pages: &[alo_store::SitePage]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|page| {
            page.sections["sections"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|section| {
            section
                .get("form_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// The stored sections with every minted form id removed, so two sites made
/// from one template can be compared for the content itself.
fn blank_form_ids(pages: &[alo_store::SitePage]) -> Vec<Value> {
    pages
        .iter()
        .map(|page| {
            let mut sections = page.sections.clone();
            if let Some(list) = sections["sections"].as_array_mut() {
                for section in list.iter_mut() {
                    if let Some(object) = section.as_object_mut()
                        && object.contains_key("form_id")
                    {
                        object.insert("form_id".to_owned(), Value::Null);
                    }
                }
            }
            sections
        })
        .collect()
}
