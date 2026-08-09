//! Structured full-site generation for alo Sites (ADR 0036).
//!
//! The model proposes one complete draft; it never writes to the store. This
//! module owns the prompt and the strict envelope parser. The parser delegates
//! sections and themes to `alo-store`'s authoritative write gates so generated
//! output cannot drift from what the editor and publisher accept.

use std::collections::HashSet;
use std::future::Future;

use alo_store::{Section, SectionsEnvelope, SiteTheme, validate_page_slug, validate_subdomain};
use serde::{Deserialize, Serialize};

use crate::agent::extract_json;
use crate::{AiConfig, ChatMessage, InferenceError, chat};

/// Current version of the complete generated-site envelope.
pub const SITE_DRAFT_SCHEMA_VERSION: u64 = 1;

/// Generation deliberately stays well below the store's administrative cap.
const MAX_GENERATED_PAGES: usize = 20;
const SITE_NAME_MAX_CHARS: usize = 120;
const PAGE_TITLE_MAX_CHARS: usize = 200;
const SEO_TITLE_MAX_CHARS: usize = 200;
const SEO_DESCRIPTION_MAX_CHARS: usize = 500;

/// The site-level portion of a generated draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteDraftSite {
    pub name: String,
    pub subdomain: String,
    pub theme: SiteTheme,
}

/// One generated page, ready for the Sites write path after human approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteDraftPage {
    pub title: String,
    pub slug: String,
    pub is_home: bool,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub sections: SectionsEnvelope,
}

/// A complete proposed website. Parsing this value never creates or publishes
/// anything; S1.28 applies it to a new draft only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteDraft {
    pub schema_version: u64,
    pub site: SiteDraftSite,
    pub pages: Vec<SiteDraftPage>,
}

/// Why a model response could not become a safe draft proposal.
#[derive(Debug, thiserror::Error)]
pub enum SiteDraftError {
    #[error(transparent)]
    Inference(#[from] InferenceError),
    #[error("site draft response did not contain one JSON object")]
    MissingObject,
    #[error(
        "unsupported site draft schema_version {0} (this build speaks {SITE_DRAFT_SCHEMA_VERSION})"
    )]
    UnsupportedVersion(u64),
    #[error("site draft JSON does not match schema v{SITE_DRAFT_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("site draft is invalid: {0}")]
    Invalid(String),
    #[error("site draft was still invalid after one repair: {0}")]
    RepairFailed(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSiteDraft {
    schema_version: u64,
    site: RawSite,
    pages: Vec<RawPage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSite {
    name: String,
    subdomain: String,
    theme: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPage {
    title: String,
    slug: String,
    is_home: bool,
    seo_title: Option<String>,
    seo_description: Option<String>,
    sections: serde_json::Value,
}

const SITE_GENERATION_SYSTEM: &str = r#"You create ONE complete draft marketing website from a business description. Reply with a SINGLE JSON object and nothing else: no prose, no markdown, no code fences.

Top-level schema (all unknown fields are forbidden):
{"schema_version":1,"site":{"name":string,"subdomain":string,"theme":{"schema_version":1,"preset":string}},"pages":[page,...]}
- site.name: concise business/site name, 1-120 characters.
- site.subdomain: lowercase DNS label, 3-40 characters, letters/digits/hyphens only, no leading/trailing hyphen.
- theme.preset: exactly one of north, ink, terra, fern, plum, carbon, midnight. Never add logo or favicon: those require the user's own files.
- pages: 1-20 pages, unique slugs, exactly one home page.
- page: {"title":string,"slug":string,"is_home":boolean,"seo_title":string|null,"seo_description":string|null,"sections":{"schema_version":1,"sections":[section,...]}}
- The home page has is_home true and slug "". Every other slug is a lowercase URL segment using letters/digits/hyphens only. Use short, useful navigation.

Every section is an object with a required "type" and only the fields listed here. Optional means omit it or use null.
- nav: {"type":"nav","links":[{"label":string,"href":string},...],"cta":link|null}
- hero: {"type":"hero","heading":string,"subheading":string|null,"image":null,"primary_cta":link|null,"secondary_cta":link|null}
- features: {"type":"features","heading":string|null,"intro":string|null,"items":[{"title":string,"body":string,"icon":string|null},...]}
- text_image: {"type":"text_image","heading":string|null,"body":string,"image":{"blob_id":string,"alt":string},"image_side":"left"|"right"}. Asset-backed: do not use during generation.
- gallery: {"type":"gallery","heading":string|null,"images":[{"blob_id":string,"alt":string},...]}. Asset-backed: do not use during generation.
- testimonials: {"type":"testimonials","heading":string|null,"items":[{"quote":string,"author":string,"role":string|null},...]}. Use only facts supplied by the user; never invent testimonials.
- pricing: {"type":"pricing","heading":string|null,"intro":string|null,"tiers":[{"name":string,"price":string,"period":string|null,"description":string|null,"features":[string,...],"cta":link|null,"highlighted":boolean},...]}. Use only prices supplied by the user.
- team: {"type":"team","heading":string|null,"members":[{"name":string,"role":string|null,"photo":null,"bio":string|null},...]}. Use only people supplied by the user.
- faq: {"type":"faq","heading":string|null,"items":[{"question":string,"answer":string},...]}
- cta: {"type":"cta","heading":string,"body":string|null,"button":link}
- contact_form: {"type":"contact_form","heading":string|null,"body":string|null,"form_id":null,"success_message":string|null}. Never invent a form id.
- footer: {"type":"footer","text":string|null,"links":[link,...]}
- link is {"label":string,"href":string}; href must be a site path (/about), fragment (#contact), or http(s)/mailto/tel URL. Never use javascript or data URLs.

Write polished, specific copy in the language of the description. Do not invent people, testimonials, prices, addresses, certifications, statistics, URLs, asset ids, or form ids. Prefer a useful small site over empty pages. Output ONLY the JSON object."#;

/// Builds the two-message generation conversation. Pure and fixture-testable;
/// no backend call is made here.
#[must_use]
pub fn site_generation_messages(description: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: SITE_GENERATION_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: format!("Business description:\n{}", description.trim()),
        },
    ]
}

/// Adds the model's refused reply and the validator's own reason to the base
/// conversation. The wording explicitly grants one correction, not a fresh
/// creative attempt.
#[must_use]
pub fn site_repair_messages(
    base: &[ChatMessage],
    reply: &str,
    refusal: &SiteDraftError,
) -> Vec<ChatMessage> {
    const MAX_REFUSAL_CHARS: usize = 1_000;

    let refusal: String = refusal
        .to_string()
        .chars()
        .take(MAX_REFUSAL_CHARS)
        .collect();
    let mut messages = base.to_vec();
    messages.push(ChatMessage {
        role: "assistant".to_owned(),
        content: reply.trim().to_owned(),
    });
    messages.push(ChatMessage {
        role: "user".to_owned(),
        content: format!(
            "That draft was refused by the site schema: {refusal}\n\
             Correct only the refused fields. Reply with ONE complete corrected site JSON object \
             and nothing else. This is your only repair attempt."
        ),
    });
    messages
}

/// Generates and validates one complete site proposal, with exactly one
/// schema-repair attempt. Transport/configuration failures are not retried;
/// only a well-formed model response that the Sites schema refuses earns the
/// correction turn.
///
/// This function never persists or publishes the result. Callers must still
/// present/apply it as a draft.
pub async fn generate_site_draft(
    config: &AiConfig,
    description: &str,
) -> Result<SiteDraft, SiteDraftError> {
    generate_site_draft_with(description, |messages| async move {
        chat(config, &messages, 0.2).await
    })
    .await
}

async fn generate_site_draft_with<T, F>(
    description: &str,
    mut turn: T,
) -> Result<SiteDraft, SiteDraftError>
where
    T: FnMut(Vec<ChatMessage>) -> F,
    F: Future<Output = Result<String, InferenceError>>,
{
    let base = site_generation_messages(description);
    let first = turn(base.clone()).await?;
    let refusal = match parse_site_draft(&first) {
        Ok(draft) => return Ok(draft),
        Err(error) => error,
    };

    let repair = site_repair_messages(&base, &first, &refusal);
    let second = turn(repair).await?;
    parse_site_draft(&second).map_err(|error| SiteDraftError::RepairFailed(error.to_string()))
}

/// Parses one proposed site through the complete, closed v1 schema.
///
/// A surrounding fence or preamble is tolerated because the extracted object
/// is still validated strictly. Unknown fields, section variants, invalid
/// links, fake asset/form references, duplicate slugs, and invalid themes are
/// refused before any persistence path sees the value.
pub fn parse_site_draft(text: &str) -> Result<SiteDraft, SiteDraftError> {
    let json = extract_json(text).ok_or(SiteDraftError::MissingObject)?;
    let value: serde_json::Value = serde_json::from_str(json)?;
    if !value.is_object() {
        return Err(SiteDraftError::MissingObject);
    }
    if let Some(version) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        && version != SITE_DRAFT_SCHEMA_VERSION
    {
        return Err(SiteDraftError::UnsupportedVersion(version));
    }

    let raw: RawSiteDraft = serde_json::from_value(value)?;
    if raw.schema_version != SITE_DRAFT_SCHEMA_VERSION {
        return Err(SiteDraftError::UnsupportedVersion(raw.schema_version));
    }
    check_text("site.name", &raw.site.name, SITE_NAME_MAX_CHARS)?;
    validate_subdomain(&raw.site.subdomain)
        .map_err(|error| invalid(format!("site.subdomain: {error}")))?;

    let theme = SiteTheme::from_value(raw.site.theme)
        .map_err(|error| invalid(format!("site.theme: {error}")))?;
    if theme.logo.is_some() || theme.favicon.is_some() {
        return Err(invalid(
            "site.theme: generated drafts may not claim logo or favicon assets".to_owned(),
        ));
    }
    if raw.pages.is_empty() || raw.pages.len() > MAX_GENERATED_PAGES {
        return Err(invalid(format!(
            "pages must contain 1-{MAX_GENERATED_PAGES} pages"
        )));
    }

    let mut home_count = 0;
    let mut slugs = HashSet::with_capacity(raw.pages.len());
    let mut pages = Vec::with_capacity(raw.pages.len());
    for page in raw.pages {
        check_text("page.title", &page.title, PAGE_TITLE_MAX_CHARS)?;
        if page.is_home {
            home_count += 1;
            if !page.slug.is_empty() {
                return Err(invalid("the home page slug must be empty".to_owned()));
            }
        } else {
            validate_page_slug(&page.slug)
                .map_err(|error| invalid(format!("page.slug: {error}")))?;
        }
        if !slugs.insert(page.slug.clone()) {
            return Err(invalid("page slugs must be unique".to_owned()));
        }
        check_optional_text(
            "page.seo_title",
            page.seo_title.as_deref(),
            SEO_TITLE_MAX_CHARS,
        )?;
        check_optional_text(
            "page.seo_description",
            page.seo_description.as_deref(),
            SEO_DESCRIPTION_MAX_CHARS,
        )?;

        let sections = SectionsEnvelope::from_value(page.sections)
            .map_err(|error| invalid(format!("page.sections: {error}")))?;
        reject_external_references(&sections)?;
        pages.push(SiteDraftPage {
            title: page.title,
            slug: page.slug,
            is_home: page.is_home,
            seo_title: page.seo_title,
            seo_description: page.seo_description,
            sections,
        });
    }
    if home_count != 1 {
        return Err(invalid(
            "pages must contain exactly one home page".to_owned(),
        ));
    }

    Ok(SiteDraft {
        schema_version: raw.schema_version,
        site: SiteDraftSite {
            name: raw.site.name,
            subdomain: raw.site.subdomain,
            theme,
        },
        pages,
    })
}

fn reject_external_references(sections: &SectionsEnvelope) -> Result<(), SiteDraftError> {
    for section in &sections.sections {
        if !section.image_blob_ids().is_empty() {
            return Err(invalid(format!(
                "{} section: generated drafts may not claim image assets",
                section.kind()
            )));
        }
        if let Section::ContactForm(contact) = section
            && contact.form_id.is_some()
        {
            return Err(invalid(
                "contact_form section: generated drafts may not claim a form id".to_owned(),
            ));
        }
    }
    Ok(())
}

fn check_text(field: &str, value: &str, cap: usize) -> Result<(), SiteDraftError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} must not be empty")));
    }
    if value != value.trim() {
        return Err(invalid(format!(
            "{field} must not have surrounding whitespace"
        )));
    }
    if value.chars().count() > cap {
        return Err(invalid(format!("{field} must be at most {cap} characters")));
    }
    Ok(())
}

fn check_optional_text(field: &str, value: Option<&str>, cap: usize) -> Result<(), SiteDraftError> {
    if let Some(value) = value {
        check_text(field, value, cap)?;
    }
    Ok(())
}

fn invalid(detail: String) -> SiteDraftError {
    SiteDraftError::Invalid(detail)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alo_store::THEME_PRESETS;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::future::ready;

    const VALID: &str = include_str!("../tests/fixtures/sites/valid_full_site.json");
    const NEAR_MISS_SECTION: &str =
        include_str!("../tests/fixtures/sites/near_miss_unknown_section.json");
    const NEAR_MISS_ASSET: &str =
        include_str!("../tests/fixtures/sites/near_miss_asset_reference.json");

    #[test]
    fn fixture_is_a_complete_strict_draft() {
        let draft = parse_site_draft(VALID).unwrap();
        assert_eq!(draft.site.name, "Juniper Bakery");
        assert_eq!(draft.pages.len(), 2);
        assert_eq!(draft.pages.iter().filter(|page| page.is_home).count(), 1);
        assert!(
            draft
                .pages
                .iter()
                .all(|page| !page.sections.sections.is_empty())
        );
    }

    #[test]
    fn prompt_documents_every_live_section_and_theme_preset() {
        let prompt = &site_generation_messages("bakery")[0].content;
        for section in [
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
        ] {
            assert!(
                prompt.contains(&format!("- {section}:")),
                "missing {section}"
            );
        }
        for preset in THEME_PRESETS {
            assert!(prompt.contains(preset.id), "missing preset {}", preset.id);
        }
        assert!(prompt.ends_with("Output ONLY the JSON object."));
    }

    #[test]
    fn description_is_trimmed_but_never_mixed_into_the_system_contract() {
        let messages = site_generation_messages("  A quiet bakery.  ");
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[1].content,
            "Business description:\nA quiet bakery."
        );
        assert!(!messages[0].content.contains("A quiet bakery"));
    }

    #[test]
    fn unknown_fields_and_section_types_are_refused() {
        let unknown_top = VALID.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"surprise\": true,",
            1,
        );
        assert!(matches!(
            parse_site_draft(&unknown_top),
            Err(SiteDraftError::Shape(_))
        ));

        let unknown_section = VALID.replacen("\"type\": \"features\"", "\"type\": \"carousel\"", 1);
        assert!(matches!(
            parse_site_draft(&unknown_section),
            Err(SiteDraftError::Invalid(_))
        ));
    }

    #[test]
    fn duplicate_or_missing_home_pages_are_refused() {
        let duplicate = VALID.replacen(
            "\"slug\": \"contact\",\n      \"is_home\": false",
            "\"slug\": \"\",\n      \"is_home\": true",
            1,
        );
        assert!(matches!(
            parse_site_draft(&duplicate),
            Err(SiteDraftError::Invalid(_))
        ));

        let missing = VALID.replacen("\"is_home\": true", "\"is_home\": false", 1);
        assert!(matches!(
            parse_site_draft(&missing),
            Err(SiteDraftError::Invalid(_))
        ));
    }

    #[test]
    fn generated_output_cannot_claim_tenant_assets_or_forms() {
        let asset = VALID.replacen(
            "\"image\": null",
            "\"image\": {\"blob_id\": \"pretend-blob\", \"alt\": \"Bread\"}",
            1,
        );
        assert!(matches!(
            parse_site_draft(&asset),
            Err(SiteDraftError::Invalid(_))
        ));

        let form = VALID.replacen("\"form_id\": null", "\"form_id\": \"pretend-form\"", 1);
        assert!(matches!(
            parse_site_draft(&form),
            Err(SiteDraftError::Invalid(_))
        ));
    }

    #[test]
    fn fences_are_tolerated_but_versions_and_non_objects_are_not() {
        let fenced = format!("Here is the draft:\n```json\n{VALID}\n```");
        assert!(parse_site_draft(&fenced).is_ok());
        assert!(matches!(
            parse_site_draft("[]"),
            Err(SiteDraftError::MissingObject)
        ));
        let future = VALID.replacen("\"schema_version\": 1", "\"schema_version\": 9", 1);
        assert!(matches!(
            parse_site_draft(&future),
            Err(SiteDraftError::UnsupportedVersion(9))
        ));
    }

    #[test]
    fn repair_conversation_keeps_the_base_and_names_the_schema_refusal() {
        let base = site_generation_messages("a bakery");
        let refusal = parse_site_draft(NEAR_MISS_SECTION).unwrap_err();
        let repaired = site_repair_messages(&base, NEAR_MISS_SECTION, &refusal);

        assert_eq!(repaired.len(), 4);
        assert_eq!(repaired[0].content, base[0].content);
        assert_eq!(repaired[1].content, base[1].content);
        assert_eq!(repaired[2].role, "assistant");
        assert_eq!(repaired[2].content, NEAR_MISS_SECTION.trim());
        assert_eq!(repaired[3].role, "user");
        assert!(repaired[3].content.contains("unknown variant `carousel`"));
        assert!(repaired[3].content.contains("only repair attempt"));
    }

    #[tokio::test]
    async fn a_near_miss_gets_one_repair_and_returns_the_corrected_fixture() {
        let replies = RefCell::new(VecDeque::from([
            NEAR_MISS_SECTION.to_owned(),
            VALID.to_owned(),
        ]));
        let conversations = RefCell::new(Vec::new());

        let draft = generate_site_draft_with("a bakery", |messages| {
            conversations.borrow_mut().push(messages);
            ready(Ok(replies.borrow_mut().pop_front().unwrap()))
        })
        .await
        .unwrap();

        assert_eq!(draft.site.name, "Juniper Bakery");
        let conversations = conversations.into_inner();
        assert_eq!(conversations.len(), 2, "one repair, never two");
        assert_eq!(conversations[0].len(), 2);
        assert_eq!(conversations[1].len(), 4);
        assert!(replies.into_inner().is_empty());
    }

    #[tokio::test]
    async fn a_second_refusal_is_typed_and_never_gets_a_third_turn() {
        let replies = RefCell::new(VecDeque::from([
            NEAR_MISS_SECTION.to_owned(),
            NEAR_MISS_ASSET.to_owned(),
            VALID.to_owned(),
        ]));
        let turns = RefCell::new(0_u8);

        let error = generate_site_draft_with("a bakery", |_| {
            *turns.borrow_mut() += 1;
            ready(Ok(replies.borrow_mut().pop_front().unwrap()))
        })
        .await
        .unwrap_err();

        assert!(matches!(error, SiteDraftError::RepairFailed(_)));
        assert_eq!(turns.into_inner(), 2, "a third model turn is forbidden");
        assert_eq!(
            replies.into_inner().len(),
            1,
            "the third fixture stays unused"
        );
    }

    #[tokio::test]
    async fn inference_failures_are_not_retried() {
        let turns = RefCell::new(0_u8);
        let error = generate_site_draft_with("a bakery", |_| {
            *turns.borrow_mut() += 1;
            ready(Err(InferenceError::NotConfigured))
        })
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            SiteDraftError::Inference(InferenceError::NotConfigured)
        ));
        assert_eq!(turns.into_inner(), 1);
    }
}
