//! Deterministic, review-first whole-site translation proposals.
//!
//! The model sees complete source snapshots and must echo them as `before`
//! values alongside complete translated `after` values. Callers can therefore
//! show a meaningful review and reject stale approval without trusting model
//! prose. This module has no store handle: proposing never writes.

use std::collections::HashSet;

use alo_store::{SectionsEnvelope, site_pages::validate_page_slug};
use serde::{Deserialize, Serialize};

use crate::agent::extract_json;
use crate::{AiConfig, ChatMessage, InferenceError, chat};

pub const SITE_TRANSLATION_SCHEMA_VERSION: u64 = 1;
const MAX_TRANSLATION_ITEMS: usize = 400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTranslationPageSnapshot {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub sections: SectionsEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTranslationPostSnapshot {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTranslationSource {
    pub source_locale: String,
    pub target_locale: String,
    pub pages: Vec<SiteTranslationPageSnapshot>,
    pub posts: Vec<SiteTranslationPostSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTranslationPageProposal {
    pub before: SiteTranslationPageSnapshot,
    pub after: SiteTranslationPageSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTranslationPostProposal {
    pub before: SiteTranslationPostSnapshot,
    pub after: SiteTranslationPostSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTranslationEnvelope {
    pub schema_version: u64,
    pub source_locale: String,
    pub target_locale: String,
    pub pages: Vec<SiteTranslationPageProposal>,
    pub posts: Vec<SiteTranslationPostProposal>,
}

#[derive(Debug, thiserror::Error)]
pub enum SiteTranslationError {
    #[error(transparent)]
    Inference(#[from] InferenceError),
    #[error("translation response did not contain one JSON object")]
    MissingObject,
    #[error("unsupported translation schema_version {0}")]
    UnsupportedVersion(u64),
    #[error("translation JSON does not match schema v1: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("translation proposal is invalid: {0}")]
    Invalid(String),
}

const SITE_TRANSLATION_SYSTEM: &str = r#"Translate one complete alo website. Return ONE JSON object only: no prose, markdown, or code fences.

Echo schema_version, source_locale, target_locale. Return every supplied page and post exactly once, in the supplied order. Each item is {"before":<exact source snapshot>,"after":<complete translated snapshot>}. In `after`, keep every id unchanged. Translate human-readable text, SEO text, and URL slugs into target_locale. Preserve all JSON structure, section types, asset ids, form ids, links, numbers, facts, and nulls. Never invent facts, people, claims, prices, URLs, assets, or forms. Slugs must be lowercase letters, digits, and hyphens. The `before` value must be byte-for-byte equivalent JSON to the supplied source snapshot. Output only the JSON object."#;

pub fn site_translation_messages(
    source: &SiteTranslationSource,
) -> Result<Vec<ChatMessage>, SiteTranslationError> {
    validate_source(source)?;
    Ok(vec![
        ChatMessage {
            role: "system".to_owned(),
            content: SITE_TRANSLATION_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: serde_json::to_string(source)?,
        },
    ])
}

pub async fn propose_site_translation(
    config: &AiConfig,
    source: &SiteTranslationSource,
) -> Result<SiteTranslationEnvelope, SiteTranslationError> {
    let messages = site_translation_messages(source)?;
    let reply = chat(config, &messages, 0.0).await?;
    let proposal = parse_site_translation(&reply)?;
    validate_site_translation(source, &proposal)?;
    Ok(proposal)
}

pub fn parse_site_translation(text: &str) -> Result<SiteTranslationEnvelope, SiteTranslationError> {
    let json = extract_json(text).ok_or(SiteTranslationError::MissingObject)?;
    let value: serde_json::Value = serde_json::from_str(json)?;
    if !value.is_object() {
        return Err(SiteTranslationError::MissingObject);
    }
    if let Some(version) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        && version != SITE_TRANSLATION_SCHEMA_VERSION
    {
        return Err(SiteTranslationError::UnsupportedVersion(version));
    }
    let proposal: SiteTranslationEnvelope = serde_json::from_value(value)?;
    validate_envelope_shape(&proposal)?;
    Ok(proposal)
}

pub fn validate_site_translation(
    source: &SiteTranslationSource,
    proposal: &SiteTranslationEnvelope,
) -> Result<(), SiteTranslationError> {
    validate_source(source)?;
    validate_envelope_shape(proposal)?;
    if proposal.source_locale != source.source_locale
        || proposal.target_locale != source.target_locale
    {
        return Err(invalid("the language pair changed"));
    }
    if proposal.pages.len() != source.pages.len() || proposal.posts.len() != source.posts.len() {
        return Err(invalid("the proposal omitted or added content"));
    }
    for (index, (expected, item)) in source.pages.iter().zip(&proposal.pages).enumerate() {
        if &item.before != expected {
            return Err(invalid(format!(
                "page {index} did not preserve its source snapshot"
            )));
        }
        if item.after.id != expected.id {
            return Err(invalid(format!("page {index} changed identity")));
        }
        validate_page_snapshot(&item.after, format!("page {index}"))?;
    }
    for (index, (expected, item)) in source.posts.iter().zip(&proposal.posts).enumerate() {
        if &item.before != expected {
            return Err(invalid(format!(
                "post {index} did not preserve its source snapshot"
            )));
        }
        if item.after.id != expected.id {
            return Err(invalid(format!("post {index} changed identity")));
        }
        validate_post_snapshot(&item.after, format!("post {index}"))?;
    }
    Ok(())
}

fn validate_source(source: &SiteTranslationSource) -> Result<(), SiteTranslationError> {
    if source.source_locale.trim().is_empty() || source.target_locale.trim().is_empty() {
        return Err(invalid("source and target languages are required"));
    }
    if source.source_locale == source.target_locale {
        return Err(invalid("source and target languages must differ"));
    }
    if source.pages.is_empty() && source.posts.is_empty() {
        return Err(invalid("the website has no content to translate"));
    }
    if source.pages.len() + source.posts.len() > MAX_TRANSLATION_ITEMS {
        return Err(invalid("the website has too much content for one proposal"));
    }
    let mut ids = HashSet::new();
    for (index, page) in source.pages.iter().enumerate() {
        validate_page_snapshot(page, format!("page {index}"))?;
        if !ids.insert(format!("page:{}", page.id)) {
            return Err(invalid("a page appears more than once"));
        }
    }
    for (index, post) in source.posts.iter().enumerate() {
        validate_post_snapshot(post, format!("post {index}"))?;
        if !ids.insert(format!("post:{}", post.id)) {
            return Err(invalid("a post appears more than once"));
        }
    }
    Ok(())
}

fn validate_envelope_shape(proposal: &SiteTranslationEnvelope) -> Result<(), SiteTranslationError> {
    if proposal.schema_version != SITE_TRANSLATION_SCHEMA_VERSION {
        return Err(SiteTranslationError::UnsupportedVersion(
            proposal.schema_version,
        ));
    }
    if proposal.pages.len() + proposal.posts.len() > MAX_TRANSLATION_ITEMS {
        return Err(invalid("the proposal contains too many items"));
    }
    Ok(())
}

fn validate_page_snapshot(
    snapshot: &SiteTranslationPageSnapshot,
    label: String,
) -> Result<(), SiteTranslationError> {
    if snapshot.id.trim().is_empty() || snapshot.title.trim().is_empty() {
        return Err(invalid(format!("{label} needs an id and title")));
    }
    if !snapshot.slug.is_empty() {
        validate_page_slug(&snapshot.slug).map_err(|error| invalid(format!("{label}: {error}")))?;
    }
    snapshot
        .sections
        .validate()
        .map_err(|error| invalid(format!("{label}: {error}")))
}

fn validate_post_snapshot(
    snapshot: &SiteTranslationPostSnapshot,
    label: String,
) -> Result<(), SiteTranslationError> {
    if snapshot.id.trim().is_empty() || snapshot.title.trim().is_empty() {
        return Err(invalid(format!("{label} needs an id and title")));
    }
    validate_page_slug(&snapshot.slug).map_err(|error| invalid(format!("{label}: {error}")))
}

fn invalid(detail: impl Into<String>) -> SiteTranslationError {
    SiteTranslationError::Invalid(detail.into())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn source() -> SiteTranslationSource {
        SiteTranslationSource {
            source_locale: "en".into(),
            target_locale: "fr".into(),
            pages: vec![SiteTranslationPageSnapshot {
                id: "page-1".into(),
                title: "Home".into(),
                slug: "".into(),
                seo_title: None,
                seo_description: Some("Welcome".into()),
                sections: SectionsEnvelope::new(),
            }],
            posts: vec![SiteTranslationPostSnapshot {
                id: "post-1".into(),
                title: "News".into(),
                slug: "news".into(),
                excerpt: "Update".into(),
            }],
        }
    }

    #[test]
    fn accepts_complete_before_after_fixture() {
        let source = source();
        let mut page = source.pages[0].clone();
        page.title = "Accueil".into();
        let mut post = source.posts[0].clone();
        post.title = "Actualites".into();
        post.slug = "actualites".into();
        let proposal = SiteTranslationEnvelope {
            schema_version: 1,
            source_locale: "en".into(),
            target_locale: "fr".into(),
            pages: vec![SiteTranslationPageProposal {
                before: source.pages[0].clone(),
                after: page,
            }],
            posts: vec![SiteTranslationPostProposal {
                before: source.posts[0].clone(),
                after: post,
            }],
        };
        validate_site_translation(&source, &proposal).unwrap();
    }

    #[test]
    fn refuses_changed_before_snapshot() {
        let source = source();
        let mut changed = source.pages[0].clone();
        changed.title = "Invented".into();
        let proposal = SiteTranslationEnvelope {
            schema_version: 1,
            source_locale: "en".into(),
            target_locale: "fr".into(),
            pages: vec![SiteTranslationPageProposal {
                before: changed.clone(),
                after: changed,
            }],
            posts: vec![SiteTranslationPostProposal {
                before: source.posts[0].clone(),
                after: source.posts[0].clone(),
            }],
        };
        assert!(
            validate_site_translation(&source, &proposal)
                .unwrap_err()
                .to_string()
                .contains("source snapshot")
        );
    }

    #[test]
    fn strict_parser_rejects_unknown_fields() {
        let text = r#"{"schema_version":1,"source_locale":"en","target_locale":"fr","pages":[],"posts":[],"surprise":true}"#;
        assert!(matches!(
            parse_site_translation(text),
            Err(SiteTranslationError::Shape(_))
        ));
    }
}
